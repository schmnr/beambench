# Connected library protocol

Beam Bench's Phase 1 consumer types live in
`crates/beambench-common/src/connected_library.rs`. The canonical provider
contract is [Craftgineer's desktop library API version 1](https://github.com/schmnr/craftgineer/blob/main/docs/integrations/desktop-library-v1.md).
The desktop consumer supports independent Craftgineer and PrintCutCarve connections, with a Library tab for each connected account.
The provider API is disabled by default until rollout validation is complete.

The server and Rust consumer share the synthetic response snapshot at
`crates/beambench-common/tests/fixtures/desktop_library_v1.json`. Keep it
byte-identical to Craftgineer's
`src/lib/integrations/desktop-library/fixtures/v1.json` when changing the
protocol. The server tests exercise lifecycle access rules; the consumer tests
exercise serialization, account binding, revisions, supported media, and limits.

Call `LibraryAccountResponse::validate_for_provider` on account responses and
`LibraryListingResponse::validate_for_provider` with the expected provider and
authorized account ID before displaying a listing. Validate collections with
`LibraryCollectionsResponse::validate_for`. The native HTTP client binds callbacks and requests
to a provider, connection generation, destination window, and document generation.
Build file and thumbnail URLs from the fixed provider origin and encoded opaque
IDs, never from display names or URLs embedded in library data.

PrintCutCarve's contract and rollout instructions live in its
`docs/integrations/desktop-library-v1.md`. Its separate canonical synthetic
snapshot is `crates/beambench-common/tests/fixtures/printcutcarve_library_v1.json`,
which matches PrintCutCarve's `lib/integrations/desktop-library/fixtures/v1.json`.
Both fixtures include the separate paid-period metadata. PrintCutCarve browses accessible
packs before files, serves original SVG bytes, and reports missing, unsupported,
unsafe or oversized files explicitly in `unavailableFiles`. SVG covers the
current original-file catalog. Future original formats need a separate validated
contract and importer change.

Each provider has its own connection generation, refresh lock, remembered
preference and password-store entry. Craftgineer's established service, user key
and saved JSON format remain compatible. Browser consent and ordinary downloads
do not hold the connection lock; refresh rotation and final import commit remain
serialized. Metadata-only native events clear or update other windows after
connection or entitlement changes. No token is sent to a renderer.

The credential backend selection has an isolated reproducible probe in
`scripts/credential-store-probe`. Its own Cargo lockfile does not affect the app's
dependency graph. See that directory's README for platform features, operational
failure handling, and the separate native release gates.


The native manager uses the external browser, one-use PKCE S256 codes, a bounded
loopback listener, and rotating refresh tokens. Access tokens stay in memory;
remembered refresh credentials use the operating system password store. A lazy,
serialized worker bounds wallet waits and removes late writes after cancellation.
The renderer stores only a non-secret preference indicating whether to attempt
credential restoration at startup or when Manage connections is opened.

Remembered credentials survive connection failures that occur before a refresh
request is sent, and HTTP 429/503 responses. Craftgineer rejects those statuses
before rotation or rolls back its token transaction. The app reports an
unavailable or rate-limited connection and permits an explicit refresh. Rejected
credentials, malformed successful responses, and transport failures after sending
remain fail-closed: the old refresh token must not be replayed if rotation may
have committed. This policy applies both when restoring and during a session.

Downloads enforce the MIME type, size and SHA-256 revision. Insertion uses the
existing importer, local assets and undo transaction. A native document lease,
window and connection checks at commit, and request identifiers protect pending
imports. A user action creates one request identifier before import setup and
reuses it on retries. Native in-flight reservations and a bounded completed-ID
cache reject duplicate work within a connection. Parsing runs on a blocking worker rather than an async network worker.

Source checks and a dependency audit do not replace release validation. Installed
macOS and Windows credential lifecycle tests and the final signed, unmodified
AppImage startup tests on Ubuntu 22.04, 24.04 and 26.04 remain mandatory. The Linux
tests must reach the real Frontend ready callback and retain camera, font and
update-relaunch coverage. Follow `docs/releasing.md` before publishing.


All free and paid accounts may connect to their own available library. An active
paid subscription verified through either connected service hides the startup
promotion; free files and individual pack purchases do not. The optional account
`paidThrough` field is separate from library access and remains backward
compatible with earlier responses. The native credential record may include this
verified end date when remembering an account. It suppresses the startup ad
offline only until that date and is cleared on revocation or disconnect. No paid
flag is persisted in frontend settings. Startup waits for both provider checks
before deciding whether to show the promotion.
