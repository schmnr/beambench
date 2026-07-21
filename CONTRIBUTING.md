# Contributing to Beam Bench

Beam Bench combines a Rust workspace with a Tauri 2 and React desktop
application. Contributions should preserve compatibility with existing project
files and fail safely when a controller or machine state is uncertain.

## Development setup

Install a current stable Rust toolchain, Node.js, and the platform prerequisites
listed by the Tauri 2 project. Then install the frontend dependencies:

```sh
cd tauri-app
npm ci
```

Run the desktop application in development mode:

```sh
cd tauri-app
npm run tauri dev
```

## Quality checks

Run the checks relevant to your change before submitting it:

```sh
cargo fmt --all -- --check
cargo test --workspace --exclude pdf417
cargo clippy --workspace --exclude pdf417

cd tauri-app
npm test
npm run build
npm run lint
```

The vendored `pdf417` crate is third-party source and is excluded from workspace
linting. Avoid running multiple Cargo commands against the same target directory
at once because they contend for build artifacts.

## Safety-sensitive behavior

Changes involving machine control must preserve these invariants:

- Treat controller acknowledgement as buffered receipt, not proof that motion
  has finished.
- Reject raw G-code while a managed job is active.
- Keep the local API disabled and localhost-only by default.
- Require explicit confirmation for motion, laser output, and raw G-code through
  the CLI and API.
- Build framing and output settings from the active machine profile.
- Keep preview power mapping consistent with emitted machine output.
- Route actions that can discard an edited project through the unsaved-changes
  guard.

Controller changes should include protocol or virtual-controller tests. Claims
of hardware support should be backed by real-device validation and reflected in
[`docs/controller-compatibility.md`](docs/controller-compatibility.md).

## Licensing

Contributions are accepted under GPL-3.0-or-later. Do not add code with an
incompatible license. Preserve copyright and license notices for third-party or
derived source files.
