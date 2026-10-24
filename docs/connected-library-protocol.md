# Connected library protocol

Beam Bench's Phase 1 consumer types live in
`crates/beambench-common/src/connected_library.rs`. The canonical provider
contract is [Craftgineer's desktop library API version 1](https://github.com/schmnr/craftgineer/blob/main/docs/integrations/desktop-library-v1.md).
Authentication, HTTP routes, account settings, and library tabs are Phase 2.

The server and Rust consumer share the synthetic response snapshot at
`crates/beambench-common/tests/fixtures/desktop_library_v1.json`. Keep it
byte-identical to Craftgineer's
`src/lib/integrations/desktop-library/fixtures/v1.json` when changing the
protocol. The server tests exercise lifecycle access rules; the consumer tests
exercise serialization, account binding, revisions, supported media, and limits.

Call `LibraryAccountResponse::validate` on account responses and
`LibraryListingResponse::validate_for` with the authorized account ID before
displaying a listing. The later HTTP client must also bind callbacks and requests
to a provider, connection generation, destination window, and document generation.
Build file and thumbnail URLs from the fixed provider origin and encoded opaque
IDs, never from display names or URLs embedded in library data.

The credential backend selection has an isolated reproducible probe in
`scripts/credential-store-probe`. Its own Cargo lockfile does not affect the app's
dependency graph. See that directory's README for platform features, operational
failure handling, and the separate native release gates.
