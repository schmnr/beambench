# GPL release compliance

Beam Bench is distributed as a whole under GPL-3.0-or-later. Its embedded
Potrace-derived files retain GPL-2.0-or-later notices; GPL-3.0 is an available
later-version option for distributing the combined application.

## Files that must accompany binary distribution

Every binary download location must provide equally clear access to the
corresponding source for that exact version. The source distribution includes:

- all tracked Rust, TypeScript, configuration, interface, and build files;
- `LICENSE`, `COPYRIGHT`, `THIRD_PARTY_NOTICES.md`, and `LICENSES/`;
- the Potrace file notices and `crates/beambench-core/src/potrace/README.md`;
- package lockfiles and the release/build scripts.
- the resolved Rust dependency sources under `vendor/cargo` and JavaScript
  dependency sources under `tauri-app/node_modules` in remedial archives.

The application About dialog and binary download page must identify the GPL
license, the Potrace copyright, and the public source location.

## Existing binary releases

Versions `v0.1.0` through `v0.1.9` contain the Potrace-derived implementation.
Run `scripts/build-gpl-source-archives` to make remedial corresponding-source
archives for these tags. Attach each archive to its matching GitHub Release
and place a source link beside every direct binary download and updater entry.
Keep those archives available for as long as the binaries remain available.

Before announcing completion:

- make the source repository public, or use another source host accessible to
  every binary recipient without authentication;
- attach the generated source archives to releases `v0.1.0` through `v0.1.9`;
- update `beambench.com` and `updates.beambench.com` download pages/manifests
  with the license and exact-version source links;
- ensure future release notes and download pages carry the same information;
- send the release and archive URLs to Peter Selinger and request written
  confirmation that the remediation is accepted.
