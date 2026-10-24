# Connected library protocol

Beam Bench's Phase 1 consumer types live in
`crates/beambench-common/src/connected_library.rs`. The canonical provider
contract is [Craftgineer's desktop library API version 1](https://github.com/schmnr/craftgineer/blob/main/docs/integrations/desktop-library-v1.md).
The Phase 2 candidate adds native account management and a Craftgineer tab.
The provider API is disabled by default until rollout validation is complete.

The server and Rust consumer share the synthetic response snapshot at
`crates/beambench-common/tests/fixtures/desktop_library_v1.json`. Keep it
byte-identical to Craftgineer's
`src/lib/integrations/desktop-library/fixtures/v1.json` when changing the
protocol. The server tests exercise lifecycle access rules; the consumer tests
exercise serialization, account binding, revisions, supported media, and limits.

Call `LibraryAccountResponse::validate` on account responses and
`LibraryListingResponse::validate_for` with the authorized account ID before
displaying a listing. The native HTTP client binds callbacks and requests
to a provider, connection generation, destination window, and document generation.
Build file and thumbnail URLs from the fixed provider origin and encoded opaque
IDs, never from display names or URLs embedded in library data.

The credential backend selection has an isolated reproducible probe in
`scripts/credential-store-probe`. Its own Cargo lockfile does not affect the app's
dependency graph. See that directory's README for platform features, operational
failure handling, and the separate native release gates.


The native manager uses the external browser, one-use PKCE S256 codes, a bounded
loopback listener, and rotating refresh tokens. Access tokens stay in memory;
remembered refresh credentials use the operating system password store. A lazy,
serialized worker bounds wallet waits and removes late writes after cancellation.
The renderer stores only a non-secret preference indicating whether to attempt
credential restoration when Manage connections is opened.

Downloads enforce the MIME type, size and SHA-256 revision. Insertion uses the
existing importer, local assets and undo transaction. A native document lease,
window and connection checks at commit, and request identifiers protect pending
imports. Parsing runs on a blocking worker rather than an async network worker.

Source checks and a dependency audit do not replace release validation. Installed
macOS and Windows credential lifecycle tests and the final signed, unmodified
AppImage startup tests on Ubuntu 22.04, 24.04 and 26.04 remain mandatory. The Linux
tests must reach the real Frontend ready callback and retain camera, font and
update-relaunch coverage. Follow `docs/releasing.md` before publishing.
