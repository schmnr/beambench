# Credential store dependency probe

This standalone crate checks the credential backends selected for the connected
library feature. It is outside the Beam Bench workspace and does not add native
dependencies to the desktop application in Phase 1.

| Platform | Explicit dependency selection |
| --- | --- |
| All | `keyring-core = 1.0.1`, default features disabled |
| Linux | `zbus-secret-service-keyring-store = 1.0.1`, only `rt-tokio-crypto-rust` |
| macOS | `apple-native-keyring-store = 1.0.2`, only `keychain` |
| Windows | `windows-native-keyring-store = 1.1.0`, default features disabled |

The selected stores require Rust 1.88 or newer. The probe locks its dependency
versions. Linux uses Rust D-Bus and cryptography rather than libsecret, libdbus,
or OpenSSL. The Docker build checks the final ELF dependency list for these and
other extra host libraries. GNOME Keyring and D-Bus in the test image provide a
test service; their libraries must not become dependencies of the probe binary.

Run the harmless mock operation on the host:

```sh
cargo run --locked --manifest-path scripts/credential-store-probe/Cargo.toml -- --mock
```

This compiles the platform's real backend but accesses only the in-memory mock
store. `--native` explicitly exercises the real credential store using a unique
disposable identity and attempts cleanup. It never prints secrets or OS error
messages. Use native mode in a disposable desktop/session when possible. A native
failure or five-second deadline reports a session-only result. The CLI process
then exits; this deadline is not a production implementation of cancellation.

Build the isolated Ubuntu 22.04 proof:

```sh
docker build --platform linux/amd64 -t beambench-credential-probe:phase1 \
  -f scripts/credential-store-probe/Dockerfile scripts/credential-store-probe
docker run --rm --platform linux/amd64 --network none beambench-credential-probe:phase1
docker run --rm --platform linux/amd64 --network none \
  -e DBUS_SESSION_BUS_ADDRESS=unix:path=/missing-beambench-probe-bus \
  beambench-credential-probe:phase1 \
  /probe/target/release/beambench-credential-store-probe --native
```

In Phase 2, instantiate the real backend lazily on a blocking worker after the
frontend is ready. Serialize operations per provider/account, including refresh,
credential replacement, and deletion. A timeout must release the UI, reject late
results for a changed connection generation, and arrange cleanup of late writes.
A timed-out blocking operation may still be running; do not treat timeout as
successful cancellation or start another operation against the same entry.

Missing, locked, denied, or unavailable secure storage offers an explicit
session-only connection. It never falls back to plaintext or a mock/file store.
Use a service name and account key bound to provider, subject, and connection.
Do not share credentials with the renderer, diagnostic logs, or project files.

Before release, the real desktop still needs native credential tests on all
platforms, GNOME Keyring and KDE Wallet lifecycle checks, and the required final
signed AppImage startup matrix on Ubuntu 22.04, 24.04, and 26.04. A dependency
probe cannot replace those release gates.
