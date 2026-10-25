#!/usr/bin/env python3
"""Exercise an unchanged signed desktop with owner-approved disposable accounts.

Uses the native platform WebDriver and real hosted PKCE consent. No IPC mocking,
token injection, TLS overrides, or personal accounts. Account credentials are
provided privately in BEAMBENCH_PILOT_ACCOUNTS and never written to evidence.
"""
import argparse
import base64
import hashlib
from html.parser import HTMLParser
import http.cookiejar
import json
import os
from pathlib import Path
import re
import secrets
import subprocess
import sys
import time
import urllib.error
import urllib.parse
import urllib.request

ORIGIN = 'https://craftgineer.com'
ELEMENT = 'element-6066-11e4-a52e-4f735466cecf'


class Forms(HTMLParser):
    def __init__(self):
        super().__init__()
        self.forms = []
        self.current = None

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if tag == 'form':
            self.current = {'action': attrs.get('action', ''), 'inputs': [], 'allow': False}
        elif self.current is not None:
            if tag == 'input' and attrs.get('name'):
                self.current['inputs'].append((attrs['name'], attrs.get('value', '')))
            if tag == 'button' and attrs.get('name') == 'decision' and attrs.get('value') == 'allow':
                self.current['allow'] = True

    def handle_endtag(self, tag):
        if tag == 'form' and self.current is not None:
            self.forms.append(self.current)
            self.current = None


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *_):
        return None


class HostedAccount:
    def __init__(self, account):
        self.account = account
        self.opener = urllib.request.build_opener(
            urllib.request.HTTPCookieProcessor(http.cookiejar.CookieJar()), NoRedirect())

    def request(self, path, data=None, headers=None):
        url = urllib.parse.urljoin(ORIGIN, path)
        if urllib.parse.urlsplit(url).netloc != 'craftgineer.com':
            raise RuntimeError('hosted_request_origin_rejected')
        request = urllib.request.Request(url, data=data, headers={
            'Origin': ORIGIN, 'Accept-Language': 'en',
            'User-Agent': 'BeamBench-DisposableDesktopValidation/1.0', **(headers or {})})
        try:
            response = self.opener.open(request, timeout=35)
        except urllib.error.HTTPError as error:
            response = error
        with response:
            body = response.read(2 * 1024 * 1024 + 1)
            if len(body) > 2 * 1024 * 1024:
                raise RuntimeError('hosted_body_limit')
            return response.status, response.headers, body

    def login(self):
        status, _, body = self.request('/api/auth/csrf')
        if status != 200:
            raise RuntimeError('csrf_request_failed')
        csrf = json.loads(body)['csrfToken']
        form = urllib.parse.urlencode({'csrfToken': csrf,
            'email': self.account['email'], 'password': self.account['password'],
            'callbackUrl': ORIGIN + '/settings', 'json': 'true'}).encode()
        self.request('/api/auth/callback/credentials', form, {
            'Content-Type': 'application/x-www-form-urlencoded', 'X-Auth-Return-Redirect': '1'})
        status, _, body = self.request('/api/auth/session')
        session = json.loads(body)
        if status != 200 or session.get('user', {}).get('id') != self.account['id']:
            raise RuntimeError('disposable_account_signin_failed')

    def consent(self, url):
        parsed = urllib.parse.urlsplit(url)
        query = urllib.parse.parse_qs(parsed.query)
        if parsed.scheme != 'https' or parsed.netloc != 'craftgineer.com' or parsed.path != '/integrations/desktop/authorize':
            raise RuntimeError('consent_origin_rejected')
        if query.get('client_id') != ['beambench-desktop'] or query.get('scope') != ['library:read']:
            raise RuntimeError('consent_scope_rejected')
        callback = urllib.parse.urlsplit(query['redirect_uri'][0])
        if callback.scheme != 'http' or callback.hostname != '127.0.0.1' or callback.path != '/oauth/craftgineer/callback' or not callback.port:
            raise RuntimeError('callback_rejected')
        status, _, body = self.request(url)
        forms = Forms()
        forms.feed(body.decode())
        consent = [f for f in forms.forms if f['allow']]
        if status != 200 or len(consent) != 1:
            raise RuntimeError('paid_consent_form_missing')
        form = consent[0]
        boundary = 'BeamBenchPilot' + secrets.token_hex(16)
        fields = form['inputs'] + [('decision', 'allow')]
        data = b''.join((f'--{boundary}\r\nContent-Disposition: form-data; name="{name}"\r\n\r\n{value}\r\n').encode() for name, value in fields)
        data += f'--{boundary}--\r\n'.encode()
        action = urllib.parse.urljoin(url, form['action'] or url)
        status, headers, _ = self.request(action, data, {'Content-Type': 'multipart/form-data; boundary=' + boundary})
        target = urllib.parse.urlsplit(headers.get('Location', ''))
        result = urllib.parse.parse_qs(target.query)
        if status != 303 or target._replace(query='', fragment='') != callback or result.get('state') != query.get('state') or 'code' not in result:
            raise RuntimeError('consent_callback_invalid')
        # Normal browser callback to the real app's listener. Never print the URL.
        with urllib.request.urlopen(target.geturl(), timeout=10) as response:
            if response.status != 204:
                raise RuntimeError('native_callback_failed')


class Driver:
    def __init__(self, endpoint, capabilities):
        self.endpoint = endpoint
        self.session = None
        self.process = None
        result = self.request('POST', '/session', {'capabilities': {'alwaysMatch': capabilities}})
        self.session = result['sessionId']

    def request(self, method, path, data=None):
        request = urllib.request.Request(self.endpoint + path, method=method,
            data=None if data is None else json.dumps(data).encode(),
            headers={'Content-Type': 'application/json'})
        try:
            with urllib.request.urlopen(request, timeout=75) as response:
                value = json.load(response)['value']
        except urllib.error.HTTPError as error:
            try:
                failure = json.load(error)['value']
                code = failure['error']
                if path == '/session':
                    print('webdriver_startup_detail=' + failure.get('message', '')[:800], flush=True)
            except Exception:
                code = 'invalid_response'
            raise RuntimeError('webdriver_' + code.replace(' ', '_')) from None
        if isinstance(value, dict) and 'error' in value:
            raise RuntimeError('webdriver_' + value['error'].replace(' ', '_'))
        return value

    def command(self, method, path, data=None):
        return self.request(method, '/session/' + self.session + path, data)

    def text(self):
        return self.command('POST', '/execute/sync', {'script': 'return document.body.innerText', 'args': []})

    def element(self, xpath):
        return self.command('POST', '/element', {'using': 'xpath', 'value': xpath})[ELEMENT]

    def click(self, xpath):
        self.command('POST', '/element/' + self.element(xpath) + '/click', {})

    def button(self, name):
        self.click('//button[normalize-space(.)=' + json.dumps(name) + ' or @aria-label=' + json.dumps(name) + ']')

    def wait(self, predicate, label, timeout=40):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            try:
                if predicate():
                    return
            except RuntimeError:
                pass
            time.sleep(0.2)
        raise RuntimeError('timeout_' + label)

    def screenshot(self, path):
        path.write_bytes(base64.b64decode(self.command('GET', '/screenshot')))

    def quit(self):
        try:
            self.command('DELETE', '')
        finally:
            if self.process is not None and self.process.poll() is None:
                # A driver attached to WebView2 does not own the app. Close only
                # this test process's native window, as a user clicking X would.
                import ctypes
                user32 = ctypes.windll.user32
                callback_type = ctypes.WINFUNCTYPE(ctypes.c_bool, ctypes.c_void_p, ctypes.c_void_p)

                def close(window, _):
                    owner = ctypes.c_ulong()
                    user32.GetWindowThreadProcessId(window, ctypes.byref(owner))
                    if owner.value == self.process.pid:
                        user32.PostMessageW(window, 0x0010, 0, 0)
                    return True

                user32.EnumWindows(callback_type(close), 0)
                try:
                    self.process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    self.process.terminate()
                    self.process.wait(timeout=10)
                    raise RuntimeError('native_window_failed_to_close')


def capture_opener(root):
    capture = root / 'authorize-url.txt'
    program = root / 'capture-url.py'
    program.write_text('import pathlib,sys,urllib.parse\n'
        'url=sys.argv[1]\np=urllib.parse.urlsplit(url)\n'
        'assert p.scheme=="https" and p.netloc=="craftgineer.com" and p.path=="/integrations/desktop/authorize"\n'
        'pathlib.Path(' + repr(str(capture)) + ').write_text(url)\n')
    if sys.platform == 'win32':
        if os.environ.get('GITHUB_ACTIONS') != 'true':
            raise RuntimeError('windows_runner_must_be_disposable')
        import winreg
        try:
            key = winreg.OpenKey(winreg.HKEY_CURRENT_USER, r'Software\Microsoft\Windows\Shell\Associations\UrlAssociations\https\UserChoice')
            progid = winreg.QueryValueEx(key, 'ProgId')[0]
            key.Close()
        except FileNotFoundError:
            progid = 'MSEdgeHTM'
        for handler in [progid, 'https', 'http']:
            key = winreg.CreateKey(winreg.HKEY_CURRENT_USER, 'Software\\Classes\\' + handler)
            winreg.SetValueEx(key, 'URL Protocol', 0, winreg.REG_SZ, '')
            command = winreg.CreateKey(key, r'shell\open\command')
            winreg.SetValueEx(command, '', 0, winreg.REG_SZ, f'"{sys.executable}" "{program}" "%1"')
            winreg.SetValueEx(command, 'DelegateExecute', 0, winreg.REG_SZ, '')
            command.Close()
            key.Close()
    else:
        binary = root / 'bin'
        binary.mkdir(exist_ok=True)
        opener = binary / 'xdg-open'
        opener.write_text('#!/bin/sh\nexec /usr/bin/python3 ' + str(program) + ' "$1"\n')
        opener.chmod(0o700)
        os.environ['PATH'] = str(binary) + os.pathsep + os.environ['PATH']
    return capture


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--application', type=Path, required=True)
    parser.add_argument('--driver', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--scenario', choices=['lifecycle', 'session-only'], default='lifecycle')
    args = parser.parse_args()
    accounts = json.loads(os.environ.pop('BEAMBENCH_PILOT_ACCOUNTS'))
    if accounts['run'] != 'beambench-phase2-final-20261025' or len(accounts['accounts']) != 2:
        raise RuntimeError('invalid_disposable_accounts')
    for account in accounts['accounts']:
        if not re.fullmatch(r'bbp2_[a-f0-9]{16}_[ab]', account['id']) or not re.fullmatch(r'bbp2-[a-f0-9]{16}-[ab]@example\.invalid', account['email']):
            raise RuntimeError('personal_account_rejected')
    from datetime import datetime, timezone
    if datetime.fromisoformat(accounts['expiresAt'].replace('Z', '+00:00')) <= datetime.now(timezone.utc):
        raise RuntimeError('disposable_accounts_expired')
    root = args.output.resolve()
    root.mkdir(parents=True, exist_ok=True)
    capture = capture_opener(root)
    os.environ.update(RUST_LOG='info', TAURI_WEBVIEW_AUTOMATION='true',
        BEAMBENCH_CONFIG_DIR=str(root/'config'), BEAMBENCH_DATA_DIR=str(root/'data'))
    application = args.application.resolve(strict=True)
    original_hash = hashlib.sha256(application.read_bytes()).hexdigest()
    capabilities = ({'browserName': 'webview2', 'ms:edgeChromium': True,
        'ms:edgeOptions': {'debuggerAddress': '127.0.0.1:17557'}}
        if sys.platform == 'win32' else
        {'browserName': 'MiniBrowser', 'webkitgtk:browserOptions': {'binary': str(application)}})
    log = (root/'driver.log').open('w')
    process = subprocess.Popen([str(args.driver), '--port=17556'], stdout=log, stderr=log)
    driver = None
    results = []
    try:
        for _ in range(100):
            try:
                urllib.request.urlopen('http://127.0.0.1:17556/status', timeout=1).close()
                break
            except urllib.error.URLError:
                time.sleep(0.1)

        def launch():
            owned = None
            if sys.platform == 'win32':
                # Tauri chooses its own WebView data folder. Attaching to its
                # explicit loopback port avoids EdgeDriver looking for the port
                # file in EdgeDriver's unrelated temporary profile.
                app_env = dict(os.environ,
                    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS='--remote-debugging-port=17557')
                with (root/'desktop.log').open('a') as app_log:
                    owned = subprocess.Popen([str(application)], env=app_env,
                        stdout=app_log, stderr=app_log)
                deadline = time.monotonic() + 45
                while time.monotonic() < deadline:
                    if owned.poll() is not None:
                        raise RuntimeError('signed_desktop_exited')
                    try:
                        urllib.request.urlopen('http://127.0.0.1:17557/json/version',timeout=1).close()
                        break
                    except urllib.error.URLError:
                        time.sleep(0.1)
                else:
                    owned.terminate()
                    owned.wait(timeout=10)
                    raise RuntimeError('webview_debug_port_unavailable')
            try:
                active = Driver('http://127.0.0.1:17556', capabilities)
            except Exception:
                if owned is not None and owned.poll() is None:
                    owned.terminate()
                    owned.wait(timeout=10)
                raise
            active.process = owned
            active.wait(lambda: active.element('//button[@aria-label="Library" or @title="Library"]'), 'frontend')
            try:
                active.button('Close welcome')
            except RuntimeError:
                pass
            if not active.command('POST', '/elements', {'using':'css selector', 'value':'[data-testid="library-drawer"]'}):
                active.click('//button[@aria-label="Library" or @title="Library"]')
            active.button('Manage connections')
            return active

        def passed(name):
            results.append(name)
            print('passed=' + name, flush=True)

        def connect(active, account, remember):
            if capture.exists():
                capture.unlink()
            checkbox = active.element('//section[@aria-label="Craftgineer connection"]//input[@type="checkbox"]')
            checked = active.command('GET', '/element/'+checkbox+'/selected')
            if checked != remember:
                active.command('POST', '/element/'+checkbox+'/click', {})
            web = HostedAccount(account)
            web.login()
            active.button('Connect Craftgineer')
            active.wait(capture.exists, 'external_consent')
            web.consent(capture.read_text())
            capture.unlink()
            active.wait(lambda: account['name'] in active.text(), 'account_connected')
            active.button('Browse library')
            active.wait(lambda: account['fixture'] in active.text(), 'account_listing')
            other = next(a for a in accounts['accounts'] if a['id'] != account['id'])
            if other['fixture'] in active.text():
                raise RuntimeError('cross_account_file_visible')
            active.button('Manage connections')

        driver = launch()
        first, second = accounts['accounts']
        if args.scenario == 'lifecycle':
            connect(driver, first, True)
            driver.screenshot(root/'account-a.png')
            passed('paid_connect_and_listing_a')
            driver.quit()
            driver = launch()
            driver.wait(lambda: first['name'] in driver.text(), 'remembered_account_a')
            passed('remembered_restart_a')
            driver.button('Disconnect')
            driver.wait(lambda: 'Connect Craftgineer' in driver.text(), 'disconnect_a')
            if first['name'] in driver.text():
                raise RuntimeError('old_account_visible_after_disconnect')
            connect(driver, second, True)
            driver.screenshot(root/'account-b.png')
            passed('switch_to_b_without_a_files')
            driver.quit()
            driver = launch()
            driver.wait(lambda: second['name'] in driver.text(), 'remembered_account_b')
            if first['name'] in driver.text():
                raise RuntimeError('wrong_account_restored')
            passed('remembered_restart_b')
            driver.button('Disconnect')
            driver.wait(lambda: 'Connect Craftgineer' in driver.text(), 'disconnect_b')
            driver.quit()
            driver = launch()
            driver.wait(lambda: 'Connect Craftgineer' in driver.text(), 'disconnect_persisted')
            if first['name'] in driver.text() or second['name'] in driver.text():
                raise RuntimeError('account_restored_after_disconnect')
            passed('disconnect_and_restart_without_account')
        connect(driver, second, False)
        passed('explicit_session_only_connection')
        driver.quit()
        driver = launch()
        driver.wait(lambda: 'Connect Craftgineer' in driver.text(), 'session_only_not_restored')
        if second['name'] in driver.text():
            raise RuntimeError('session_only_restored')
        passed('session_only_not_restored')
        driver.screenshot(root/'disconnected.png')
        if hashlib.sha256(application.read_bytes()).hexdigest() != original_hash:
            raise RuntimeError('signed_application_changed')
        (root/'results.json').write_text(json.dumps({'scenario':args.scenario,
            'platform':sys.platform,'applicationSha256':original_hash,
            'passed':results,'ipcMocked':False,'tlsVerificationBypassed':False,
            'nativePasswordPromptsTested':False},indent=2)+'\n')
        print(json.dumps({'passed':results,'platform':sys.platform}))
    except Exception:
        (root/'failure.json').write_text(json.dumps({'platform':sys.platform,
            'scenario':args.scenario,'completed':results},indent=2)+'\n')
        if driver:
            try:
                driver.screenshot(root/'failure.png')
            except Exception:
                pass
        raise
    finally:
        if driver:
            try:
                driver.quit()
            except Exception:
                pass
        process.terminate()
        process.wait(timeout=10)
        log.close()
        if capture.exists():
            capture.unlink()


if __name__ == '__main__':
    try:
        main()
    except Exception as error:
        # Driver/server errors can contain cookies, codes or submitted credentials.
        # Print only our fixed validation identifiers, never arbitrary responses.
        message = str(error)
        safe = message if message.replace('_', '').isalnum() and len(message) < 100 else type(error).__name__
        print('desktop_validation_failed=' + safe, file=sys.stderr)
        sys.exit(1)
