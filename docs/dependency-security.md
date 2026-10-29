# Dependency security

## 0.2.37

The project, feedback-export and xTool archive dependency is updated from ZIP
2.4.2 to 9.1.3, above the fixed version for
[GHSA-m665-v7pm-7grj](https://github.com/zip-rs/zip2/security/advisories/GHSA-m665-v7pm-7grj).
Project loading explicitly validates local and central file headers before
reading entries. Updating the dependency alone does not enable this opt-in
validation. Imported project assets remain in memory and archive-supplied paths
are not extracted to disk.

The regression changes a local filename while leaving its central-directory
name intact. Without the explicit validation, the project is accepted and the
test fails. With validation, it is rejected. A fixture created by the signed
0.2.36 CLI verifies compatibility with the previous ZIP 2 writer.

The updater's optional legacy archive feature is disabled, removing its separate
ZIP 4.6.1 dependency. The shipped update formats remain raw Windows NSIS EXE,
Linux AppImage and macOS tar.gz. The updater's macOS tar support does not depend
on that optional feature. Keep the existing HTTPS endpoint and signing key, and
verify the production updater engine and final signed packages before publishing.

The resolved workspace lockfile contains only ZIP 9.1.3. Check all target graphs
when auditing this dependency; a host-only macOS tree would miss the updater's
former Windows-only ZIP dependency.

## 0.2.35

Rustls is updated from 0.23.45 to 0.23.46, resolving the four October 2026
Dependabot findings in the previous lockfile. The dependency update originally
proposed in PR #111 is included in this release preparation.

The Linux desktop still uses Tauri's GTK3 stack and its GLib 0.18 API.
GLib 0.20 is not a compatible replacement for that stack. The root Cargo
manifest therefore patches `glib` to [the vendored 0.18.5 source](../vendor/glib/README.beambench.md)
with upstream's two-line fix for
[RUSTSEC-2024-0429](https://rustsec.org/advisories/RUSTSEC-2024-0429.html).
Its version and MIT license are preserved. This does not update or bundle a
different system GLib library, or change the Ubuntu 22.04 build baseline.

An optimized probe calls the affected safe iterator APIs with forward, reverse,
mixed, `nth`, `nth_back`, `last`, `count`, and UTF-8 cases:

```sh
cargo run --locked --release --manifest-path scripts/glib-variant-probe/Cargo.toml
```

On macOS with GLib 2.84.3, the same probe compiled against the unchanged
crates.io 0.18.5 package exited with SIGSEGV, status 139. The backported source
returns `variant_str_iter=ok`. Linux CI runs the optimized probe separately
from the app tests. Release AppImages must still pass the final-package checks
on Ubuntu 22.04, 24.04 and 26.04.

Keep dependency alerts enabled. Do not suppress a finding merely to make the
repository look clean. A scanner that reports the upstream version against
this locally patched source needs the backport and regression evidence attached
to its review. Remove the patch when the GTK/Tauri stack can use a fixed upstream
release directly.

## Frontend tooling

The 0.2.35 lockfile updates the build tooling's Zod dependency to 4.6.7,
addressing [GHSA-mrhx-c7xc-m3w5](https://github.com/advisories/GHSA-mrhx-c7xc-m3w5).
Both the full npm audit and the production-only audit must report zero known
vulnerabilities before publication. The earlier Vitest 5 migration and the
`postcss-selector-parser` 7.1.6 override remain in place. Historical audit
results are retained in Git history; recheck the current lockfile rather than
treating those earlier counts as current.

```sh
cd tauri-app
npm audit
npm audit --omit=dev
```
