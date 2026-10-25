#!/bin/bash
set -euo pipefail
export DISPLAY=:99
export XDG_RUNTIME_DIR=/home/beamtest/runtime
export DBUS_SESSION_BUS_ADDRESS=unix:path=/home/beamtest/bus
export APPIMAGE_EXTRACT_AND_RUN=1
export LIBGL_ALWAYS_SOFTWARE=1
# Only the disposable, unprivileged container needs this nested-sandbox override.
export WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS=1
export QT_LINUX_ACCESSIBILITY_ALWAYS_ON=1
export QT_ACCESSIBILITY=1
mkdir -p "$XDG_RUNTIME_DIR"
chmod 700 "$XDG_RUNTIME_DIR"
Xvfb :99 -screen 0 1440x1000x24 -nolisten tcp > /evidence/xvfb.log 2>&1 &
dbus-daemon --session --address="$DBUS_SESSION_BUS_ADDRESS" --fork
dbus-update-activation-environment DISPLAY XDG_RUNTIME_DIR QT_LINUX_ACCESSIBILITY_ALWAYS_ON QT_ACCESSIBILITY
openbox > /evidence/openbox.log 2>&1 &
if [[ "$1" == gnome ]]; then
  printf '%s' 'Disposable-wallet-only-20261025' | gnome-keyring-daemon --unlock --components=secrets > /evidence/keyring.log 2>&1
elif [[ "$1" == kde ]]; then
  mkdir -p /home/beamtest/.config
  printf '[Wallet]\nEnabled=true\nFirst Use=false\nDefault Wallet=kdewallet\nLocal Wallet=kdewallet\n[org.freedesktop.secrets]\napiEnabled=true\n' > /home/beamtest/.config/kwalletrc
  kwalletd5 > /evidence/kwallet.log 2>&1 &
  python3 /scripts/connected-library-smoke/wallet.py prepare-kde --provider kde --output /evidence/wallet-created.json
else
  echo 'Unsupported wallet scenario' >&2
  exit 1
fi
python3 /scripts/connected-library-smoke/wallet.py metadata --provider "$1" --output /evidence/provider.json
for scenario in lifecycle wallet-unlock wallet-deny wallet-unavailable; do
  python3 /scripts/test-connected-library-desktop.py \
    --application /test.AppImage --driver /usr/bin/WebKitWebDriver \
    --scenario "$scenario" --wallet-provider "$1" --output "/evidence/$scenario"
  python3 /scripts/connected-library-smoke/wallet.py metadata --provider "$1" \
    --expect-empty --output "/evidence/$scenario/wallet-cleanup.json"
done
