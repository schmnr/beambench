# Contributing to Beam Bench

Beam Bench combines a Rust workspace with a Tauri 2 and React desktop
application. Contributions should preserve compatibility with existing project
files and fail safely when a controller or machine state is uncertain.

## Before you start

- Use the repository's issue forms for reproducible bugs and feature proposals.
- Use [Beam Bench Support](https://beambench.com/support) for setup help,
  connection troubleshooting, and questions that are not source-code issues.
- Open an issue before investing in a large feature or architectural change so
  the direction can be agreed on first.
- Report vulnerabilities privately according to [SECURITY.md](SECURITY.md).
  Never include credentials, private user data, or unredacted diagnostics in a
  public issue or pull request.

## Contribution workflow

1. Fork the repository and branch from the latest `main`.
2. Keep each branch and pull request focused on one change.
3. Add or update tests for behavior changes.
4. Run the relevant quality checks below.
5. Open a draft pull request early if you want feedback.
6. Describe the user impact, safety impact, and validation in the pull request.

Maintainers normally squash a pull request when merging it. A pull request may
be closed when it is out of scope, duplicates existing work, creates an
unacceptable safety risk, or cannot be maintained.

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
