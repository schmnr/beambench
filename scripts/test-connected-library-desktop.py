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

    def delete_disposable(self, proof):
        if proof.get('label') != self.account['label'] or not re.fullmatch(r'[a-f0-9]{64}', proof.get('token','')):
            raise RuntimeError('disposable_deletion_proof_invalid')
        status, _, body = self.request('/api/auth/delete-account/confirm?token=' + proof['token'])
        if status != 200 or b'Permanently delete your account?' not in body:
            raise RuntimeError('deletion_confirmation_missing')
        status, _, body = self.request('/api/auth/session')
        if status != 200 or json.loads(body).get('user',{}).get('id') != self.account['id']:
            raise RuntimeError('deletion_get_changed_account')
        form = urllib.parse.urlencode({'token':proof['token']}).encode()
        headers = {'Content-Type':'application/x-www-form-urlencoded'}
        status, _, body = self.request('/api/auth/delete-account/confirm',form,headers)
        if status != 200 or b'Your account has been deleted' not in body:
            raise RuntimeError('disposable_deletion_failed')
        status, _, _ = self.request('/api/auth/delete-account/confirm',form,headers)
        if status != 400:
            raise RuntimeError('deletion_proof_replay_accepted')
        status, _, body = self.request('/api/auth/session')
        if status != 200 or json.loads(body).get('user',{}).get('id'):
            raise RuntimeError('deleted_website_session_still_authenticated')


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

    def button_enabled(self, name):
        element = self.element('//button[normalize-space(.)=' + json.dumps(name) + ']')
        return self.command('GET','/element/'+element+'/enabled')

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
        try:
            path.write_bytes(base64.b64decode(self.command('GET', '/screenshot')))
        except (RuntimeError, TypeError):
            subprocess.run(['scrot',str(path)],check=True,timeout=10)

    def ready(self):
        return self.element('//button[@aria-label="Library" or @title="Library"]')

    def open_connections(self):
        if not self.command('POST', '/elements', {'using':'css selector', 'value':'[data-testid="library-drawer"]'}):
            self.click('//button[@aria-label="Library" or @title="Library"]')
        self.button('Manage connections')

    def set_remember(self, remember):
        checkbox = self.element('//section[@aria-label="Craftgineer connection"]//input[@type="checkbox"]')
        if self.command('GET', '/element/'+checkbox+'/selected') != remember:
            self.command('POST', '/element/'+checkbox+'/click', {})

    def quit(self):
        self.command('DELETE', '')


class WindowsDriver(Driver):
    """Use Windows accessibility on the installed app without a debug port."""
    def __init__(self, application, root):
        from pywinauto import Application
        with (root/'desktop.log').open('a') as log:
            self.process = subprocess.Popen([str(application)], stdout=log, stderr=log)
        self.app = Application(backend='uia').connect(process=self.process.pid, timeout=40)

    def controls(self, kind=None):
        from pywinauto.findwindows import ElementNotFoundError
        try:
            window = self.app.top_window()
        except (ElementNotFoundError, RuntimeError):
            return []
        return [c for c in window.descendants(control_type=kind) if c.is_visible()]

    def text(self):
        return '\n'.join(c.window_text() for c in self.controls())

    def button(self, name):
        targets = [c for c in self.controls('Button') if c.window_text() == name]
        if len(targets) != 1:
            raise RuntimeError('native_button_not_unique')
        targets[0].invoke()

    def button_enabled(self, name):
        return any(c.window_text() == name and c.is_enabled() for c in self.controls('Button'))

    def ready(self):
        return any(c.window_text() == 'Library' for c in self.controls('Button'))

    def open_connections(self):
        if not any(c.window_text() == 'Manage connections' for c in self.controls('Button')):
            self.button('Library')
        self.button('Manage connections')

    def set_remember(self, remember):
        choices = [c for c in self.controls('CheckBox') if 'Remember this account' in c.window_text()]
        if len(choices) != 1:
            raise RuntimeError('native_remember_checkbox_missing')
        if bool(choices[0].get_toggle_state()) != remember:
            choices[0].toggle()

    def screenshot(self, path):
        self.app.top_window().capture_as_image().save(str(path))

    def quit(self):
        if self.process.poll() is None:
            self.app.top_window().close()
            try:
                self.process.wait(timeout=15)
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
        # Shell builtins avoid loading the host Python runtime with AppImage
        # library paths inherited by the desktop's normal URL opener.
        opener.write_text('#!/bin/sh\ncase "$1" in\n'
            'https://craftgineer.com/integrations/desktop/authorize\\?*)\n'
            'umask 077\nprintf "%s" "$1" > "' + str(capture) + '";;\n'
            '*) exit 1;;\nesac\n')
        opener.chmod(0o700)
        os.environ['PATH'] = str(binary) + os.pathsep + os.environ['PATH']
        os.environ['BROWSER'] = str(opener)
        # AppImage launchers may prepend their own xdg-open. Register the test
        # browser through the desktop's normal URL association as well.
        data = root/'xdg-data'
        config = root/'xdg-config'
        (data/'applications').mkdir(parents=True,exist_ok=True)
        config.mkdir(exist_ok=True)
        (data/'applications/beambench-pilot.desktop').write_text(
            '[Desktop Entry]\nType=Application\nName=Beam Bench pilot browser\n'
            f'Exec="{opener}" %u\nNoDisplay=true\n'
            'MimeType=x-scheme-handler/https;x-scheme-handler/http;\n')
        (config/'mimeapps.list').write_text('[Default Applications]\n'
            'x-scheme-handler/https=beambench-pilot.desktop\n'
            'x-scheme-handler/http=beambench-pilot.desktop\n')
        os.environ.update(XDG_DATA_HOME=str(data),XDG_CONFIG_HOME=str(config))
    return capture


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--application', type=Path, required=True)
    parser.add_argument('--driver', type=Path)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--scenario', choices=['lifecycle', 'session-only', 'wallet-unlock', 'wallet-deny', 'wallet-unavailable', 'deletion'], default='lifecycle')
    parser.add_argument('--wallet-provider', choices=['gnome','kde'])
    args = parser.parse_args()
    accounts = json.loads(os.environ.pop('BEAMBENCH_PILOT_ACCOUNTS'))
    deletion_input = os.environ.pop('BEAMBENCH_PILOT_DELETE_PROOF','')
    deletion_proof = json.loads(deletion_input) if deletion_input else None
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
    capabilities = {'webkitgtk:browserOptions': {'binary': str(application)}}
    if args.scenario == 'wallet-unavailable':
        wrapper = root/'without-wallet'
        wrapper.write_text('#!/bin/sh\nexport DBUS_SESSION_BUS_ADDRESS=unix:path=/missing-beambench-test-bus\nexec /test.AppImage "$@"\n')
        wrapper.chmod(0o700)
        capabilities['webkitgtk:browserOptions']['binary'] = str(wrapper)
    log = (root/'driver.log').open('w')
    process = None
    if sys.platform != 'win32':
        if args.driver is None:
            raise RuntimeError('native_driver_required')
        process = subprocess.Popen([str(args.driver), '--port=17556'], stdout=log, stderr=log)
    driver = None
    results = []
    try:
        for _ in range(100 if process else 0):
            try:
                urllib.request.urlopen('http://127.0.0.1:17556/status', timeout=1).close()
                break
            except urllib.error.URLError:
                time.sleep(0.1)

        def launch():
            active = (WindowsDriver(application,root) if sys.platform == 'win32'
                else Driver('http://127.0.0.1:17556', capabilities))
            active.wait(active.ready, 'frontend')
            try:
                active.button('Close welcome')
            except RuntimeError:
                pass
            active.open_connections()
            return active

        def passed(name):
            results.append(name)
            print('passed=' + name, flush=True)

        def connect(active, account, remember):
            if capture.exists():
                capture.unlink()
            active.set_remember(remember)
            web = HostedAccount(account)
            web.login()
            active.button('Connect Craftgineer')
            print('checkpoint=native_connect_clicked',flush=True)
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
        wallet_tool = Path(__file__).parent/'connected-library-smoke/wallet.py'

        def wallet(action, background=False):
            if not args.wallet_provider:
                raise RuntimeError('wallet_provider_required')
            command = [sys.executable,str(wallet_tool),action,'--provider',args.wallet_provider,
                '--output',str(root/('wallet-'+action+'.json'))]
            if background:
                return subprocess.Popen(command)
            subprocess.run(command,check=True,timeout=35)

        if args.scenario in ['wallet-unlock','wallet-deny']:
            connect(driver,first,True)
            driver.quit()
            wallet('lock')
            answer = wallet('unlock' if args.scenario == 'wallet-unlock' else 'deny',True)
            driver = launch()
            if answer.wait(timeout=30) != 0:
                raise RuntimeError('native_wallet_response_failed')
            if args.scenario == 'wallet-unlock':
                driver.wait(lambda: first['name'] in driver.text(),'locked_wallet_restored')
                passed('native_prompt_unlock_and_account_restore')
            else:
                driver.wait(lambda: 'Secure credential storage is unavailable' in driver.text(),'denied_restore_fallback')
                if first['name'] in driver.text():
                    raise RuntimeError('account_restored_after_wallet_denial')
                passed('native_prompt_denial_rejected_restore')
                answer = wallet('deny',True)
                connect(driver,second,True)
                if answer.wait(timeout=30) != 0:
                    raise RuntimeError('native_wallet_response_failed')
                driver.wait(lambda: 'Secure credential storage is unavailable' in driver.text(),'denied_save_session_only')
                passed('native_prompt_denied_save_session_only')
                # Unlock for cleanup of any previously stored credential, then
                # disconnect normally. The UI must not silently leave it behind.
                answer = wallet('unlock',True)
                driver.button('Disconnect')
                if answer.wait(timeout=30) != 0:
                    raise RuntimeError('wallet_cleanup_unlock_failed')
                driver.wait(lambda: driver.button_enabled('Connect Craftgineer'),'denied_store_disconnect')
                driver.quit()
                driver = launch()
                driver.wait(lambda: driver.button_enabled('Connect Craftgineer'),'denied_store_restart')
                if first['name'] in driver.text() or second['name'] in driver.text():
                    raise RuntimeError('denied_store_old_account_restored')
                passed('denied_store_cleanup_and_restart')
            if args.scenario == 'wallet-unlock':
                driver.button('Disconnect')
                driver.wait(lambda: driver.button_enabled('Connect Craftgineer'),'unlocked_store_disconnect')
            driver.screenshot(root/'wallet-result.png')
            # Continue through the normal explicit session-only restart check.
        if args.scenario == 'wallet-unavailable':
            connect(driver,first,True)
            driver.wait(lambda: 'Secure credential storage is unavailable' in driver.text(),'missing_bus_session_only')
            passed('unavailable_store_session_only')
            driver.quit()
            driver = launch()
            driver.wait(lambda: driver.button_enabled('Connect Craftgineer'),'missing_bus_not_restored')
            if first['name'] in driver.text():
                raise RuntimeError('unavailable_store_restored')
            passed('unavailable_store_not_restored')
        if args.scenario == 'lifecycle':
            connect(driver, first, True)
            driver.screenshot(root/'account-a.png')
            passed('paid_connect_and_listing_a')
            driver.quit()
            driver = launch()
            driver.wait(lambda: first['name'] in driver.text(), 'remembered_account_a')
            passed('remembered_restart_a')
            driver.button('Disconnect')
            driver.wait(lambda: driver.button_enabled('Connect Craftgineer'), 'disconnect_a')
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
            driver.wait(lambda: driver.button_enabled('Connect Craftgineer'), 'disconnect_b')
            driver.quit()
            driver = launch()
            driver.wait(lambda: driver.button_enabled('Connect Craftgineer'), 'disconnect_persisted')
            if first['name'] in driver.text() or second['name'] in driver.text():
                raise RuntimeError('account_restored_after_disconnect')
            passed('disconnect_and_restart_without_account')
        if args.scenario == 'deletion':
            connect(driver,second,True)
            driver.button('Browse library')
            driver.wait(lambda: second['fixture'] in driver.text(),'cached_library_before_delete')
            if not deletion_proof:
                raise RuntimeError('disposable_deletion_proof_required')
            web = HostedAccount(second)
            web.login()
            web.delete_disposable(deletion_proof)
            passed('hosted_confirmation_delete_and_session_revocation')
            driver.button('Refresh')
            driver.wait(lambda: driver.button_enabled('Connect Craftgineer'),'deleted_account_disconnected')
            if second['name'] in driver.text() or second['fixture'] in driver.text():
                raise RuntimeError('deleted_account_visible')
            passed('deleted_account_revoked_native_library_and_credential')
            driver.quit()
            driver = launch()
            driver.wait(lambda: driver.button_enabled('Connect Craftgineer'),'deleted_account_not_restored')
            if second['name'] in driver.text():
                raise RuntimeError('deleted_account_restored')
            passed('deleted_account_not_restored_after_restart')
        else:
            connect(driver, second, False)
            passed('explicit_session_only_connection')
            driver.quit()
            driver = launch()
            driver.wait(lambda: driver.button_enabled('Connect Craftgineer'), 'session_only_not_restored')
            if second['name'] in driver.text():
                raise RuntimeError('session_only_restored')
            passed('session_only_not_restored')
        driver.screenshot(root/'disconnected.png')
        if hashlib.sha256(application.read_bytes()).hexdigest() != original_hash:
            raise RuntimeError('signed_application_changed')
        (root/'results.json').write_text(json.dumps({'scenario':args.scenario,
            'platform':sys.platform,'applicationSha256':original_hash,
            'passed':results,'ipcMocked':False,'tlsVerificationBypassed':False,
            'nativePasswordPromptsTested':args.scenario in ['wallet-unlock','wallet-deny']},indent=2)+'\n')
        print(json.dumps({'passed':results,'platform':sys.platform}))
    except Exception:
        (root/'failure.json').write_text(json.dumps({'platform':sys.platform,
            'scenario':args.scenario,'completed':results},indent=2)+'\n')
        if driver:
            try:
                (root/'failure.txt').write_text(driver.text())
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
        if process:
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
