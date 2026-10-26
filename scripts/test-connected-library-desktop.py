#!/usr/bin/env python3
"""Exercise an unchanged signed desktop with owner-approved disposable accounts.

Uses the native platform WebDriver and real hosted PKCE consent. No IPC mocking,
token injection, TLS overrides, or personal accounts. Account credentials are
provided privately in BEAMBENCH_PILOT_ACCOUNTS and never written to evidence.
"""
import argparse
import hashlib
from html.parser import HTMLParser
import http.cookiejar
import json
import os
from pathlib import Path
import re
import secrets
import socket
import subprocess
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
import zipfile

PROVIDER = 'craftgineer'
HOST = 'craftgineer.com'
ORIGIN = 'https://' + HOST
PROVIDER_NAME = 'Craftgineer'
EXPECTED_CALLBACK_STATUS = 200
ELEMENT = 'element-6066-11e4-a52e-4f735466cecf'


def project_geometry(path):
    try:
        with zipfile.ZipFile(path) as archive:
            info = archive.getinfo('project.json')
            if info.file_size > 128 * 1024 * 1024:
                raise RuntimeError('test_project_json_limit')
            project = json.loads(archive.read(info))
        return {key: project[key] for key in ['objects', 'layers', 'assets']}
    except (OSError, zipfile.BadZipFile, KeyError, ValueError):
        return None


def xpath_literal(value):
    if "'" not in value:
        return "'" + value + "'"
    if '"' not in value:
        return '"' + value + '"'
    return 'concat(' + ',"\'",'.join("'" + part + "'" for part in value.split("'")) + ')'


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
        self.cookies = http.cookiejar.CookieJar()
        self.opener = urllib.request.build_opener(
            urllib.request.HTTPCookieProcessor(self.cookies), NoRedirect())

    def request(self, path, data=None, headers=None):
        url = urllib.parse.urljoin(ORIGIN, path)
        destination = urllib.parse.urlsplit(url)
        if destination.scheme != 'https' or destination.netloc != HOST:
            raise RuntimeError('hosted_request_origin_rejected')
        request = urllib.request.Request(url, data=data, headers={
            'Origin': ORIGIN, 'Accept-Language': 'en',
            'User-Agent': 'BeamBench-DisposableDesktopValidation/1.0', **(headers or {})})
        try:
            response = self.opener.open(request, timeout=35)
        except urllib.error.HTTPError as error:
            response = error
        except urllib.error.URLError as error:
            raise RuntimeError('hosted_network_' + type(error.reason).__name__) from None
        with response:
            body = response.read(2 * 1024 * 1024 + 1)
            if len(body) > 2 * 1024 * 1024:
                raise RuntimeError('hosted_body_limit')
            return response.status, response.headers, body

    def login(self):
        if PROVIDER == 'printcutcarve':
            # Approved disposable DB sessions avoid sending email or using a real
            # Google identity. Native PKCE consent and token exchange stay real.
            value = self.account.get('sessionToken', '')
            if not re.fullmatch(r'[a-f0-9]{64}', value):
                raise RuntimeError('disposable_session_invalid')
            self.cookies.set_cookie(http.cookiejar.Cookie(0, '__Secure-next-auth.session-token',
                value, None, False, HOST, False, False, '/', True, True,
                None, True, None, None, {'HttpOnly': None}, False))
            status, _, body = self.request('/api/auth/session')
            if status != 200 or json.loads(body).get('user', {}).get('id') != self.account['id']:
                raise RuntimeError('disposable_session_rejected')
            return
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
        if parsed.scheme != 'https' or parsed.netloc != HOST or parsed.path != '/integrations/desktop/authorize':
            raise RuntimeError('consent_origin_rejected')
        if query.get('client_id') != ['beambench-desktop'] or query.get('scope') != ['library:read']:
            raise RuntimeError('consent_scope_rejected')
        callback = urllib.parse.urlsplit(query['redirect_uri'][0])
        if callback.scheme != 'http' or callback.hostname != '127.0.0.1' or callback.path != '/oauth/' + PROVIDER + '/callback' or not callback.port:
            raise RuntimeError('callback_rejected')
        status, _, body = self.request(url)
        forms = Forms()
        forms.feed(body.decode())
        consent = [f for f in forms.forms if f['allow']]
        if status != 200 or len(consent) != 1:
            raise RuntimeError('consent_form_missing')
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
        try:
            with urllib.request.urlopen(target.geturl(), timeout=10) as response:
                if response.status != EXPECTED_CALLBACK_STATUS:
                    raise RuntimeError('native_callback_failed')
        except urllib.error.URLError as error:
            raise RuntimeError('callback_network_' + type(error.reason).__name__) from None

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
        self.closed = False
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
        except urllib.error.URLError as error:
            raise RuntimeError('driver_network_' + type(error.reason).__name__) from None
        if isinstance(value, dict) and 'error' in value:
            raise RuntimeError('webdriver_' + value['error'].replace(' ', '_'))
        return value

    def command(self, method, path, data=None):
        return self.request(method, '/session/' + self.session + path, data)

    def text(self):
        return self.command('GET','/element/'+self.element('/html/body')+'/text')

    def library_text(self):
        return self.command('GET','/element/'+self.element('//div[@data-testid="library-drawer"]')+'/text')

    def library_button(self, name):
        self.click('//div[@data-testid="library-drawer"]//button[normalize-space(.)=' + xpath_literal(name) + ']')

    def element(self, xpath):
        return self.command('POST', '/element', {'using': 'xpath', 'value': xpath})[ELEMENT]

    def click(self, xpath):
        self.command('POST', '/element/' + self.element(xpath) + '/click', {})

    def button(self, name):
        self.click('//button[normalize-space(.)=' + json.dumps(name) + ' or @aria-label=' + json.dumps(name) + ']')

    def button_enabled(self, name):
        element = self.element('//button[normalize-space(.)=' + json.dumps(name) + ' or @aria-label=' + json.dumps(name) + ']')
        return self.command('GET','/element/'+element+'/enabled')

    def insert_file(self, name):
        xpath = '//p[normalize-space(.)=' + xpath_literal(name) + ']/parent::div/button[normalize-space(.)="Insert"]'
        self.wait(lambda: self.command('GET', '/element/' + self.element(xpath) + '/enabled'), 'file_insert_enabled')
        self.click(xpath)

    def file_dialog(self, title, path):
        # GTK's native chooser appends the selected project filter extension.
        # Enter the stem so the saved path has one .lzrproj suffix.
        destination = path.with_suffix('') if title.startswith('Save') and path.suffix == '.lzrproj' else path
        def window():
            result = subprocess.run(['xdotool','search','--onlyvisible','--name','^'+re.escape(title)+'$'],capture_output=True,text=True)
            matches = result.stdout.split()
            return matches[0] if len(matches) == 1 else None
        self.wait(window, 'native_file_dialog')
        handle = window()
        subprocess.run(['xdotool','windowactivate','--sync',handle,'key','--clearmodifiers','ctrl+l'],check=True,timeout=10)
        subprocess.run(['xdotool','type','--clearmodifiers','--delay','1',str(destination)],check=True,timeout=20)
        # Accept exactly once. Return followed by an accessibility click can
        # queue two saves and leave GTK's overwrite confirmation open.
        import pyatspi
        def nodes(node):
            yield node
            for child in node:
                yield from nodes(child)
        def accept():
            for application in pyatspi.Registry.getDesktop(0):
                for dialog in nodes(application):
                    if dialog.name != title or not dialog.getState().contains(pyatspi.STATE_SHOWING):
                        continue
                    name = 'Save' if title.startswith('Save') else 'Open'
                    buttons = [node for node in nodes(dialog) if node.getRoleName() == 'push button'
                        and node.name.lstrip('_') == name and node.getState().contains(pyatspi.STATE_SHOWING)]
                    if len(buttons) == 1:
                        return buttons[0].queryAction().doAction(0)
            return False
        self.wait(accept, 'native_file_dialog_accept')
        self.wait(lambda: not window(), 'native_file_dialog_closed')

    def welcome_visible(self):
        elements = self.command('POST', '/elements', {'using': 'xpath',
            'value': '//button[@aria-label="Close welcome"]'})
        return any(self.command('GET', '/element/' + item[ELEMENT] + '/displayed') for item in elements)

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
        subprocess.run(['scrot',str(path)],check=True,timeout=10)

    def ready(self):
        return self.element('//button[@aria-label="Library" or @title="Library"]')

    def project_loaded(self, filename):
        # WebKit's body-text endpoint omits the toolbar's project-name span.
        return subprocess.run(['xdotool','search','--onlyvisible','--name',
            '^'+re.escape(filename)+r'( \*)? - Beam Bench$'],capture_output=True).returncode == 0

    def open_connections(self):
        if not self.command('POST', '/elements', {'using':'css selector', 'value':'[data-testid="library-drawer"]'}):
            self.click('//button[@aria-label="Library" or @title="Library"]')
        self.button('Manage connections')

    def choose_provider(self):
        self.click('//button[@aria-pressed and normalize-space(.)=' + json.dumps(PROVIDER_NAME) + ']')

    def search_packs(self, value):
        element = self.element('//input[@aria-label="Search your packs"]')
        self.command('POST', '/element/' + element + '/clear', {})
        self.command('POST', '/element/' + element + '/value', {'text': value})

    def set_remember(self, remember):
        checkbox = self.element('//section[@aria-label=' + json.dumps(PROVIDER_NAME + ' connection') + ']//input[@type="checkbox"]')
        if self.command('GET', '/element/'+checkbox+'/selected') != remember:
            self.command('POST', '/element/'+checkbox+'/click', {})

    def quit(self):
        if self.closed:
            return
        # WebKitWebDriver cannot reliably reap an AppImage's runtime wrapper.
        # Close the actual test window normally, verify its process exits, then
        # restart the disposable driver for the next actual app launch.
        windows = subprocess.check_output(['xdotool','search','--onlyvisible',
            '--name','.*Beam Bench'],text=True).split()
        if len(windows) != 1:
            raise RuntimeError('native_test_window_not_unique')
        pid = int(subprocess.check_output(['xdotool','getwindowpid',windows[0]],text=True))
        ancestor = pid
        while ancestor > 1 and ancestor != self.process.pid:
            status = Path('/proc/'+str(ancestor)+'/status').read_text()
            ancestor = int(next(line.split()[1] for line in status.splitlines() if line.startswith('PPid:')))
        if ancestor != self.process.pid:
            raise RuntimeError('native_window_not_owned_by_test_driver')
        # Ask the window manager to close normally, like its title-bar button.
        # Synthetic shortcuts after native choosers depend on keyboard focus.
        # wmctrl sends _NET_CLOSE_WINDOW, retaining the app's close handling.
        subprocess.run(['wmctrl','-i','-c',hex(int(windows[0]))],check=True,timeout=10)
        deadline = time.monotonic()+15
        while Path('/proc/'+str(pid)).exists() and time.monotonic()<deadline:
            time.sleep(0.1)
        if Path('/proc/'+str(pid)).exists():
            raise RuntimeError('native_window_failed_to_close')
        self.process.terminate()
        self.process.wait(timeout=10)
        self.closed = True


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

    def library_region(self):
        panels = [c for c in self.controls('Group') if c.window_text() == PROVIDER_NAME + ' connection']
        if len(panels) != 1:
            raise RuntimeError('native_library_region_not_unique')
        return panels[0]

    def library_text(self):
        return '\n'.join(c.window_text() for c in self.library_region().descendants() if c.is_visible())

    def library_button(self, name):
        targets = [c for c in self.library_region().descendants(control_type='Button') if c.window_text() == name and c.is_visible()]
        if len(targets) != 1:
            raise RuntimeError('native_library_button_not_unique')
        targets[0].invoke()

    def button(self, name):
        targets = [c for c in self.controls('Button') if c.window_text() == name]
        if len(targets) != 1:
            raise RuntimeError('native_button_not_unique')
        targets[0].invoke()

    def button_enabled(self, name):
        return any(c.window_text() == name and c.is_enabled() for c in self.controls('Button'))

    def insert_file(self, name):
        labels = [c for c in self.controls('Text') if c.window_text() == name]
        if len(labels) != 1:
            raise RuntimeError('native_file_card_not_unique')
        label = labels[0].rectangle()
        buttons = [c for c in self.controls('Button') if c.window_text() == 'Insert' and c.is_enabled()
            and c.rectangle().top >= label.bottom - 4 and c.rectangle().left < label.right and c.rectangle().right > label.left]
        if not buttons:
            raise RuntimeError('native_file_insert_not_unique')
        min(buttons, key=lambda c: c.rectangle().top - label.bottom).invoke()

    def file_dialog(self, title, path):
        from pywinauto import Desktop
        from pywinauto.keyboard import send_keys
        dialog = Desktop(backend='win32').window(title=title, class_name='#32770', process=self.process.pid)
        dialog.wait('visible enabled', timeout=30)
        dialog.set_focus()
        # Focus the standard filename field for both Open and Save choosers.
        send_keys('%n')
        send_keys('^a')
        send_keys(str(path), with_spaces=True, pause=0.001)
        send_keys('{ENTER}')
        dialog.wait_not('visible', timeout=30)

    def welcome_visible(self):
        return any(c.window_text() == 'Close welcome' for c in self.controls('Button'))

    def ready(self):
        return any(c.window_text() == 'Library' for c in self.controls('Button'))

    def project_loaded(self, filename):
        title = self.app.top_window().window_text()
        return title.startswith(filename + ' - ') or title.startswith(filename + ' * - ')

    def open_connections(self):
        if not any(c.window_text() == 'Manage connections' for c in self.controls('Button')):
            self.button('Library')
        self.button('Manage connections')

    def choose_provider(self):
        self.button(PROVIDER_NAME)

    def search_packs(self, value):
        choices = [c for c in self.controls('Edit') if c.window_text() == 'Search your packs']
        if len(choices) != 1:
            raise RuntimeError('native_pack_search_missing')
        choices[0].set_edit_text(value)

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
        f'assert p.scheme=="https" and p.netloc=={HOST!r} and p.path=="/integrations/desktop/authorize"\n'
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
            f'https://{HOST}/integrations/desktop/authorize\\?*)\n'
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
    parser.add_argument('--provider', choices=['craftgineer', 'printcutcarve'], default='craftgineer')
    parser.add_argument('--expected-callback-status', type=int, choices=[200, 204], default=200, help='Use 204 only when reproducing an older signed candidate.')
    parser.add_argument('--application', type=Path, required=True)
    parser.add_argument('--driver', type=Path)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--scenario', choices=['lifecycle', 'session-only', 'wallet-unlock', 'wallet-deny', 'wallet-unavailable', 'deletion'], default='lifecycle')
    parser.add_argument('--wallet-provider', choices=['gnome','kde'])
    parser.add_argument('--check-startup-ad', action='store_true', help='Check free and paid startup behavior using explicit disposable-account entitlement metadata.')
    parser.add_argument('--check-project-import', action='store_true', help='Insert an original file, save/undo/redo, then reopen and save locally with cloud access blocked.')
    args = parser.parse_args()
    if args.check_startup_ad and args.scenario not in ['lifecycle', 'session-only']:
        parser.error('--check-startup-ad requires lifecycle or session-only')
    if args.check_project_import and args.scenario != 'lifecycle':
        parser.error('--check-project-import requires lifecycle')
    global PROVIDER, HOST, ORIGIN, PROVIDER_NAME, EXPECTED_CALLBACK_STATUS
    PROVIDER = args.provider
    HOST = PROVIDER + '.com'
    ORIGIN = 'https://' + HOST
    PROVIDER_NAME = 'Craftgineer' if PROVIDER == 'craftgineer' else 'PrintCutCarve'
    EXPECTED_CALLBACK_STATUS = args.expected_callback_status
    if PROVIDER == 'printcutcarve' and args.scenario == 'deletion':
        raise RuntimeError('printcutcarve_hosted_deletion_not_supported_by_driver')
    accounts = json.loads(os.environ.pop('BEAMBENCH_PILOT_ACCOUNTS'))
    deletion_input = os.environ.pop('BEAMBENCH_PILOT_DELETE_PROOF','')
    deletion_proof = json.loads(deletion_input) if deletion_input else None
    expected_run = 'beambench-phase2-final-20261025' if PROVIDER == 'craftgineer' else 'beambench-pcc-20261025'
    if accounts['run'] != expected_run or len(accounts['accounts']) != 2:
        raise RuntimeError('invalid_disposable_accounts')
    for account in accounts['accounts']:
        if PROVIDER == 'craftgineer':
            valid = re.fullmatch(r'bbp2_[a-f0-9]{16}_[ab]', account['id']) and re.fullmatch(r'bbp2-[a-f0-9]{16}-[ab]@example\.invalid', account['email'])
        else:
            valid = re.fullmatch(r'[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}', account['id']) and re.fullmatch(r'pccbb-[a-f0-9]{16}-[ab]@example\.invalid', account['email']) and bool(account.get('pack'))
        if not valid:
            raise RuntimeError('personal_account_rejected')
        if args.check_startup_ad and not isinstance(account.get('paidSubscription'), bool):
            raise RuntimeError('disposable_paid_subscription_metadata_required')
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
    capabilities = {'pageLoadStrategy':'none',
        'webkitgtk:browserOptions': {'binary': str(application)}}
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

        def launch(remembered_account=None, offline=False):
            nonlocal process
            if sys.platform != 'win32' and process.poll() is not None:
                process = subprocess.Popen([str(args.driver),'--port=17556'],stdout=log,stderr=log)
                for _ in range(100):
                    try:
                        urllib.request.urlopen('http://127.0.0.1:17556/status',timeout=1).close()
                        break
                    except urllib.error.URLError:
                        time.sleep(0.1)
            active = (WindowsDriver(application,root) if sys.platform == 'win32'
                else Driver('http://127.0.0.1:17556', capabilities))
            if sys.platform != 'win32':
                active.process = process
            active.wait(active.ready, 'frontend')
            if args.check_startup_ad:
                expected_ad = not (remembered_account and remembered_account['paidSubscription'])
                if expected_ad:
                    active.wait(active.welcome_visible, 'free_startup_ad')
                    active.screenshot(root/('startup-ad-' + str(len(results)) + '.png'))
                    active.button('Close welcome')
                    passed('startup_ad_without_paid_subscription')
                elif active.welcome_visible():
                    raise RuntimeError('paid_startup_ad_visible')
            else:
                try:
                    active.button('Close welcome')
                except RuntimeError:
                    pass
            active.open_connections()
            active.choose_provider()
            if remembered_account and (args.check_startup_ad or offline):
                if offline:
                    active.wait(lambda: active.button_enabled('Connect ' + PROVIDER_NAME), 'offline_restore_finished')
                    if remembered_account['name'] in active.text():
                        raise RuntimeError('cloud_access_not_blocked')
                else:
                    active.wait(lambda: remembered_account['name'] in active.text(), 'ad_check_account_restored')
            if args.check_startup_ad and remembered_account:
                if active.welcome_visible():
                    raise RuntimeError('paid_startup_ad_visible')
                if remembered_account['paidSubscription']:
                    passed('paid_remembered_startup_without_ad')
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
            active.button('Connect ' + PROVIDER_NAME)
            print('checkpoint=native_connect_clicked',flush=True)
            active.wait(capture.exists, 'external_consent')
            web.consent(capture.read_text())
            print('checkpoint=hosted_consent_completed',flush=True)
            capture.unlink()
            active.wait(lambda: account['name'] in active.text(), 'account_connected')
            print('checkpoint=connected_account_visible',flush=True)
            active.button('Browse library')
            print('checkpoint=library_opened',flush=True)
            if PROVIDER == 'printcutcarve':
                active.search_packs(account['pack'])
                active.wait(lambda: account['pack'] in active.text() and active.button_enabled('Open'), 'pack_listing')
                active.library_button('Open')
            active.wait(lambda: account['fixture'] in active.library_text(), 'account_listing')
            other = next(a for a in accounts['accounts'] if a['id'] != account['id'])
            if other['fixture'] in active.library_text():
                raise RuntimeError('cross_account_file_visible')
            if args.check_project_import and account['label'] == 'a':
                active.button('New')
                active.wait(lambda: active.button_enabled('Undo') is False, 'fresh_project')
                active.insert_file(account['fixture'])
                active.wait(lambda: active.button_enabled('Undo'), 'native_original_import', timeout=90)
                active.button('Save')
                active.file_dialog('Save Project As', root/'imported.lzrproj')
                active.wait(lambda: project_geometry(root/'imported.lzrproj'), 'saved_import')
                original = project_geometry(root/'imported.lzrproj')
                if not original['objects']:
                    raise RuntimeError('import_saved_without_geometry')
                passed('native_original_insert_and_project_save')
                active.button('Undo')
                active.wait(lambda: active.button_enabled('Redo'), 'native_import_undo')
                active.button('Save')
                active.wait(lambda: (project_geometry(root/'imported.lzrproj') or {}).get('objects') == [], 'saved_undo')
                active.button('Redo')
                active.wait(lambda: not active.button_enabled('Redo'), 'native_import_redo')
                active.button('Save')
                active.wait(lambda: project_geometry(root/'imported.lzrproj') == original, 'saved_redo')
                passed('native_import_undo_redo_matches_saved_geometry')
            active.button('Manage connections')

        driver = launch()
        first, second = accounts['accounts']
        wallet_tool = Path(__file__).parent/'connected-library-smoke/wallet.py'

        def wallet(action, background=False):
            if not args.wallet_provider:
                raise RuntimeError('wallet_provider_required')
            command = [sys.executable,str(wallet_tool),action,'--provider',args.wallet_provider,
                '--output',str(root/('wallet-'+action+'.json')), '--library-provider', PROVIDER]
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
                driver.wait(lambda: driver.button_enabled('Connect ' + PROVIDER_NAME),'denied_restore_not_connected')
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
                driver.wait(lambda: driver.button_enabled('Connect ' + PROVIDER_NAME),'denied_store_disconnect')
                driver.quit()
                driver = launch()
                driver.wait(lambda: driver.button_enabled('Connect ' + PROVIDER_NAME),'denied_store_restart')
                if first['name'] in driver.text() or second['name'] in driver.text():
                    raise RuntimeError('denied_store_old_account_restored')
                passed('denied_store_cleanup_and_restart')
            if args.scenario == 'wallet-unlock':
                driver.button('Disconnect')
                driver.wait(lambda: driver.button_enabled('Connect ' + PROVIDER_NAME),'unlocked_store_disconnect')
            driver.screenshot(root/'wallet-result.png')
            # Continue through the normal explicit session-only restart check.
        if args.scenario == 'wallet-unavailable':
            connect(driver,first,True)
            driver.wait(lambda: 'Secure credential storage is unavailable' in driver.text(),'missing_bus_session_only')
            passed('unavailable_store_session_only')
            driver.quit()
            driver = launch()
            driver.wait(lambda: driver.button_enabled('Connect ' + PROVIDER_NAME),'missing_bus_not_restored')
            if first['name'] in driver.text():
                raise RuntimeError('unavailable_store_restored')
            passed('unavailable_store_not_restored')
        if args.scenario == 'lifecycle':
            connect(driver, first, True)
            driver.screenshot(root/'account-a.png')
            passed('connect_and_listing_a')
            driver.quit()
            driver = launch(first)
            driver.wait(lambda: first['name'] in driver.text(), 'remembered_account_a')
            passed('remembered_restart_a')
            if args.check_project_import:
                expected_geometry = project_geometry(root/'imported.lzrproj')
                if not expected_geometry or not expected_geometry['objects']:
                    raise RuntimeError('offline_source_project_invalid')
                driver.quit()
                names = ['HTTP_PROXY','HTTPS_PROXY','ALL_PROXY','NO_PROXY','http_proxy','https_proxy','all_proxy','no_proxy']
                previous = {name: os.environ.get(name) for name in names}
                # Reserve a non-listening loopback port, so cloud connections
                # are refused before any credential can reach the provider.
                with socket.socket() as blocker:
                    blocker.bind(('127.0.0.1',0))
                    proxy = 'http://127.0.0.1:' + str(blocker.getsockname()[1])
                    try:
                        for name in names:
                            os.environ[name] = '127.0.0.1,localhost' if name.lower() == 'no_proxy' else proxy
                        driver = launch(first, offline=True)
                        driver.button('Open')
                        driver.file_dialog('Open Project', root/'imported.lzrproj')
                        driver.wait(lambda: driver.project_loaded('imported.lzrproj'), 'offline_project_open')
                        driver.button('Save As')
                        driver.file_dialog('Save Project As', root/'offline-copy.lzrproj')
                        driver.wait(lambda: project_geometry(root/'offline-copy.lzrproj') == expected_geometry, 'offline_geometry_round_trip')
                        driver.screenshot(root/'offline-local-project.png')
                        passed('native_cloud_blocked_offline_project_reopen_and_save')
                        driver.quit()
                    finally:
                        for name,value in previous.items():
                            if value is None:
                                os.environ.pop(name,None)
                            else:
                                os.environ[name]=value
                driver = launch(first)
                passed('remembered_login_survives_offline_project_round_trip')
            driver.button('Disconnect')
            driver.wait(lambda: driver.button_enabled('Connect ' + PROVIDER_NAME), 'disconnect_a')
            if first['name'] in driver.text():
                raise RuntimeError('old_account_visible_after_disconnect')
            connect(driver, second, True)
            driver.screenshot(root/'account-b.png')
            passed('switch_to_b_without_a_files')
            driver.quit()
            driver = launch(second)
            driver.wait(lambda: second['name'] in driver.text(), 'remembered_account_b')
            if first['name'] in driver.text():
                raise RuntimeError('wrong_account_restored')
            passed('remembered_restart_b')
            driver.button('Disconnect')
            driver.wait(lambda: driver.button_enabled('Connect ' + PROVIDER_NAME), 'disconnect_b')
            driver.quit()
            driver = launch()
            driver.wait(lambda: driver.button_enabled('Connect ' + PROVIDER_NAME), 'disconnect_persisted')
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
            driver.wait(lambda: driver.button_enabled('Connect ' + PROVIDER_NAME),'deleted_account_disconnected')
            if second['name'] in driver.text() or second['fixture'] in driver.text():
                raise RuntimeError('deleted_account_visible')
            passed('deleted_account_revoked_native_library_and_credential')
            driver.quit()
            driver = launch()
            driver.wait(lambda: driver.button_enabled('Connect ' + PROVIDER_NAME),'deleted_account_not_restored')
            if second['name'] in driver.text():
                raise RuntimeError('deleted_account_restored')
            passed('deleted_account_not_restored_after_restart')
        else:
            connect(driver, second, False)
            passed('explicit_session_only_connection')
            driver.quit()
            driver = launch()
            driver.wait(lambda: driver.button_enabled('Connect ' + PROVIDER_NAME), 'session_only_not_restored')
            if second['name'] in driver.text():
                raise RuntimeError('session_only_restored')
            passed('session_only_not_restored')
        driver.screenshot(root/'disconnected.png')
        if hashlib.sha256(application.read_bytes()).hexdigest() != original_hash:
            raise RuntimeError('signed_application_changed')
        (root/'results.json').write_text(json.dumps({'scenario':args.scenario,
            'platform':sys.platform,'provider':PROVIDER,'applicationSha256':original_hash,
            'callbackStatus':EXPECTED_CALLBACK_STATUS,
            'startupAdChecked':args.check_startup_ad,
            'projectImportChecked':args.check_project_import,
            'passed':results,'ipcMocked':False,'tlsVerificationBypassed':False,
            'nativePasswordPromptsTested':args.scenario in ['wallet-unlock','wallet-deny']},indent=2)+'\n')
        print(json.dumps({'passed':results,'platform':sys.platform}))
    except Exception:
        projects=[]
        for path in root.glob('*.lzrproj'):
            geometry=project_geometry(path)
            projects.append({'name':path.name,'readable':geometry is not None,
                'objects':len(geometry['objects']) if geometry is not None else None})
        (root/'failure.json').write_text(json.dumps({'platform':sys.platform,
            'scenario':args.scenario,'completed':results,'savedProjects':projects},indent=2)+'\n')
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
