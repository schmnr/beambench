#!/usr/bin/env python3
"""Drive only the disposable test session's real password-store dialogs."""
import argparse
import json
import os
from pathlib import Path
import time
import dbus
import pyatspi

PASSWORD = 'Disposable-wallet-only-20261025'


def nodes(root):
    yield root
    for child in root:
        yield from nodes(child)


def visible():
    result = []
    # A pending app connection also has a Cancel button. Respond only inside
    # the OS password-store applications, never inside Beam Bench's own UI.
    for application in pyatspi.Registry.getDesktop(0):
        if application.name not in ['gcr-prompter','kwalletd5']:
            continue
        for node in nodes(application):
            try:
                if node.getState().contains(pyatspi.STATE_SHOWING):
                    result.append(node)
            except Exception:
                pass
    return result


def wait(find, label, timeout=25):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        result = find()
        if result:
            return result
        time.sleep(0.1)
    raise RuntimeError('wallet_timeout_' + label)


def button(names):
    target = wait(lambda: next((n for n in visible() if n.getRoleName() == 'push button'
        and n.name in names), None), 'button')
    if not target.queryAction().doAction(0):
        raise RuntimeError('wallet_button_rejected')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=['prepare-kde', 'metadata', 'lock', 'unlock', 'deny'])
    parser.add_argument('--provider', choices=['gnome', 'kde'], required=True)
    parser.add_argument('--output', type=Path)
    parser.add_argument('--expect-empty', action='store_true')
    args = parser.parse_args()
    bus = dbus.SessionBus()
    service = dbus.Interface(bus.get_object('org.freedesktop.secrets', '/org/freedesktop/secrets'), 'org.freedesktop.Secret.Service')
    owner = bus.get_name_owner('org.freedesktop.secrets')
    pid = dbus.Interface(bus.get_object('org.freedesktop.DBus', '/org/freedesktop/DBus'), 'org.freedesktop.DBus').GetConnectionUnixProcessID(owner)
    command = Path('/proc/' + str(pid) + '/cmdline').read_bytes().split(b'\0')[0].decode()
    expected = 'kwalletd5' if args.provider == 'kde' else 'gnome-keyring-daemon'
    if Path(command).name != expected:
        raise RuntimeError('wrong_password_store_provider')
    result = {'provider':args.provider, 'process':expected, 'action':args.action}
    if args.action == 'prepare-kde':
        collection, prompt = service.CreateCollection({'org.freedesktop.Secret.Collection.Label':dbus.String('kdewallet',variant_level=1)}, 'default')
        if str(prompt) != '/':
            dbus.Interface(bus.get_object('org.freedesktop.secrets', prompt), 'org.freedesktop.Secret.Prompt').Prompt('')
            radio = wait(lambda: next((n for n in visible() if n.getRoleName() == 'radio button' and n.name == 'Classic, blowfish encrypted file'), None), 'wallet_type')
            radio.queryAction().doAction(0)
            if any(n.name == 'Next >' for n in visible() if n.getRoleName() == 'push button'):
                button(['Next >'])
            else:
                button(['Finish'])
            fields = wait(lambda: [n for n in visible() if n.getRoleName() == 'password text'], 'new_wallet_password')
            if len(fields) != 2:
                raise RuntimeError('unexpected_wallet_password_fields')
            for field in fields:
                field.queryEditableText().setTextContents(PASSWORD)
            button(['Finish', 'OK'])
        wait(lambda: str(service.ReadAlias('default')) != '/', 'default_collection')
        result['encryptedWalletCreated'] = True
    elif args.action in ['unlock', 'deny']:
        wait(lambda: [n for n in visible() if n.getRoleName() == 'password text'], 'native_password_prompt')
        if args.action == 'deny':
            button(['Cancel'])
        else:
            fields = [n for n in visible() if n.getRoleName() == 'password text']
            if len(fields) != 1:
                raise RuntimeError('unexpected_unlock_password_fields')
            fields[0].queryEditableText().setTextContents(PASSWORD)
            button(['Unlock', 'OK', 'Open'])
        result['nativePromptObserved'] = True
        result['promptResponse'] = args.action
    else:
        collection = service.ReadAlias('default')
        if str(collection) == '/':
            raise RuntimeError('default_collection_missing')
        if args.action == 'lock':
            service.Lock([collection])
        properties = dbus.Interface(bus.get_object('org.freedesktop.secrets', collection), 'org.freedesktop.DBus.Properties')
        result['locked'] = bool(properties.Get('org.freedesktop.Secret.Collection', 'Locked'))
        unlocked, locked = service.SearchItems({'service':'com.beambench.desktop-library','username':'craftgineer'})
        result['providerCredentialItems'] = len(unlocked)+len(locked)
        if args.expect_empty and result['providerCredentialItems']:
            raise RuntimeError('provider_credentials_remain')
        if args.action == 'lock' and not result['locked']:
            raise RuntimeError('wallet_lock_failed')
    if args.output:
        args.output.write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps(result), flush=True)


if __name__ == '__main__':
    main()
