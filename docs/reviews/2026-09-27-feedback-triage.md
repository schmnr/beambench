# Feedback review — 27 September 2026

Reviewed the five reports filed after 19 September, including their full
diagnostic bundles and issue comments. None has an attached project or any
follow-up comments. The reported PDF source files are unavailable.

The public repository HEAD is still `e065f29d` (0.2.24). This checkout is on
`codex/raster-memory-budget` with the earlier memory, import PowerScale,
reference-point, preset, and BRLTTY work still uncommitted. This investigation
does not publish those changes or establish that users have received them.

## Report findings

| Report | Evidence | Assessment / next step |
| --- | --- | --- |
| [#43](https://github.com/schmnr/beambench-feedback/issues/43), `r-4cec41c805fb4d088adc3a6049cf37b4` | 0.2.24, Windows; PDF rejected with the combined encrypted/object-stream error | Importer compatibility restriction. The error does not distinguish encryption from compressed objects, and the scanner can also produce false positives. Obtain the original PDF before attributing an exact cause. |
| [#44](https://github.com/schmnr/beambench-feedback/issues/44), `r-a89f557b676144cfbefb58c050385b65` | Same profile/session context; PDF drawing operator `Do` rejected | The importer explicitly rejects this drawing operation. Supporting it needs resource-aware import, including transforms and appearance constraints. Skipping the operation would omit artwork. |
| [#45](https://github.com/schmnr/beambench-feedback/issues/45), `r-58f93e9207c84cfc8dfe1fccfcf07bc8` | Same profile; repeats #43's error | Group with #43 for investigation, without assuming it is the same file. |
| [#46](https://github.com/schmnr/beambench-feedback/issues/46), `r-3eae13215a62405daae4685a15f3b70a` | **0.1.4**, Windows COM4; repeated reset/banner timeouts, one successful connection, then an OS write failure during a job | Two symptoms, not simply “no GRBL response.” Reproduce on 0.2.24 or a newer test build. The current handshake first accepts fresh status responses without resetting, unlike the recorded 0.1.4 sequence. The underlying USB write failure is not diagnosed by this bundle. |
| [#47](https://github.com/schmnr/beambench-feedback/issues/47), `r-9cf22b1af12f407e8142ff3273eeb056` | 0.2.24, Windows COM3/CH340; both opens fail in about 30 ms with `Acesso negado` (access denied), no RX/TX | Windows denies opening the port before protocol detection. Check which process owns it and repeat after reconnecting with other serial apps closed. The bundle does not identify the owner or prove a Beam Bench defect. This is unrelated to Linux BRLTTY. |

The PDF reports are evidence of an import limitation, not evidence that the
user configured the laser incorrectly. Their font warnings predate the failed
imports and do not explain these explicit parser rejection messages.

## Confirmed failed-import state defect

`tauri-app/src/stores/projectStore.ts:importArtworkBatch` resolves and creates
a destination layer through a separate backend call before invoking the file
parser. When parsing rejects, the function exits without removing the new
layer or refreshing the frontend project. Repeating the attempt can accumulate
empty backend layers while the frontend remains stale.

A temporary Vitest reproduction ran the real store import action with mocked
backend services: start with no layers, reject each PDF import, repeat three
times. It observed three calls that created backend layers, zero frontend
layers, and no cleanup or project refresh. This reproduces the frontend state
transition, not a native end-to-end import with a customer PDF.

The three customer reports show zero objects and layer counts rising from two
to three to five. That is consistent with this defect, but other user actions
between reports are unknown. The source file matches the released 0.2.24 file
byte for byte.

Implemented: the frontend passes an optional destination-layer specification
with the import command instead of creating a layer first. The service stages
the complete batch on a project copy and commits only after every file succeeds.
Existing asset bytes remain shared through `Arc`. Layer creation, imported
artwork, assets, and routing changes become one undoable edit. Failures do not
change the live project, undo/redo history, or cached plan. Empty artwork imports
do not leave an empty destination layer. Existing API clients can continue to
provide just a layer ID.

Regression coverage includes path and content imports, three repeated failures,
early PDF rejection, a late image failure after SVG artwork and embedded image
assets have been staged, preservation of existing redo history, successful
mixed imports, inherited layer settings, and one-step undo/redo.

## PDF compatibility and scanner review

`crates/beambench-core/src/import_pdf.rs` rejects encrypted documents and object
streams before loading, to keep eagerly expanded data within resource limits.
It also rejects `Do`, clipping, unsupported graphics state, and other drawing
operations that it cannot preserve. These are explicit restrictions rather
than crashes. Removing the checks without implementing the corresponding
semantics and resource limits is not a complete fix.

The early `has_eager_pdf_encoding` scanner searches raw bytes for PDF names
without distinguishing syntax from literal strings, comments, or stream data.
An ordinary metadata string containing `/Encrypt` or `/ObjStm` triggers the
same rejection as a real unsupported encoding. A synthetic PDF 1.4 reproduction
confirmed that an ordinary title imports one vector path, whereas changing
only the title to `Guide to /Encrypt settings` or `Guide to /ObjStm settings`
causes rejection. The PDF library independently confirms both rejected files
are unencrypted and have no compressed cross-reference entries. This is a
confirmed false positive, but is not established as the cause of the customer
reports. This parser file also matches released 0.2.24 byte for byte.

Implemented: replace the raw-name scan with the PDF library's parsed encryption
metadata, using the same 1 MiB eager-stream decompression limit. The full loader
filters object streams before expansion and rejects compressed cross-reference
entries. Encryption and compressed objects now have distinct messages. The
64 MiB source and 32 MiB expanded page-content limits remain in place.
Regression fixtures include names inside metadata, comments and binary stream
bytes; real encrypted documents with empty and nonempty passwords; and actual
compressed objects produced by the PDF writer.

## Further investigation

### PDF compatibility

A writer-generated PDF with real compressed objects remains unsupported even
when the same vector artwork imports from a conventional PDF. This confirms a
format compatibility gap independently of the metadata false positive.

Additional fixtures use real page resource dictionaries referring to a vector
Form XObject (with its own transform and bounds) and a raster Image XObject.
Both reach the explicit `Do` rejection. Thus #44's operator message cannot tell
us whether its source contains vector forms, embedded images, or both. Ignoring
`Do` would import incomplete artwork.

A complete extension needs bounded object-stream expansion (including an
aggregate budget, not only the library's per-stream limit), resource resolution,
nested form transforms and state, recursion/cycle limits, form bounding-box
clipping, and an explicit route for images. Clipping, transparency, masks and
unsupported compositing must still be handled or rejected. Original PDFs from
#43–45 are needed to identify which subset will resolve those reports. These
features have not been silently enabled by this correction.

Library review used the pinned `lopdf 0.44.0` source: the normal loader applies
its filter before object-stream expansion; its encrypted loader ignores that
filter. This is why encryption is inspected before full loading. The metadata
reader also resolves metadata/page-tree objects, with the same per-stream limit;
this change is not a claim of a new aggregate PDF memory budget.

### Windows connection reports

For #47, the bundle already contains the actionable `serial_port_unavailable`
error, including advice to close competing serial applications and reconnect.
The low-level Windows library maps several open failures to `NoDevice`, with
localized OS detail. Beam Bench deliberately avoids guessing from English
error text. The recorded Portuguese `Acesso negado` is an OS open denial, not
a baud-rate, homing, or GRBL parser failure.

Reviewed current connection ownership: public connect/disconnect entry points
share `controller_connection_gate`, reject an already active session, and clear
pending connections before opening another. The serial transport rejects a
second open on the same transport; failed attempts and explicit disconnects
release their owned handles. This does not rule out another Beam Bench process,
another application, or a driver/device problem. The report cannot identify the
owner. No automatic retry/reset or privileged service change is justified by
this evidence.

For #46, current GRBL connection first asks for a fresh status without DTR reset,
then validates the response; it does not require an unsolicited startup banner.
The report's 0.1.4 reset/banner sequence does not exercise this path. A separate
mid-job OS write failure is real evidence of a transport failure, but does not
establish a firmware or USB-driver cause. Added a simulated write-failure test:
accept one command, acknowledge it, fail the next write, and verify that the job
and session are released while successful sent/acknowledged counts and the error
remain available in diagnostics. This verifies application cleanup, not the
customer's physical USB connection.

Next evidence needed: original PDFs for #43–45; a current-version diagnostic
bundle plus controller/firmware details for #46; and COM3 ownership/reconnect
results for #47. Facebook reports mentioned by the user have not yet been
provided in this round.

## Validation

- 211 frontend tests passed across import drop handling, project state, import
  service payloads, and the properties panel.
- TypeScript (`tsc --noEmit`) and ESLint on the changed frontend files passed.
- All 37 core PDF tests passed, including real encryption, escaped names, actual
  object streams, referenced Form/Image XObjects, and decompression-limit fixtures.
- All 36 service import tests passed, including batch rollback, no-op imports,
  inherited settings, mixed artwork, and single-step undo/redo.
- All 23 serial tests passed, including the exact Portuguese OS error and a
  macOS virtual serial-port reopen/byte-exchange test.
- All 121 machine-service tests passed, including the injected mid-job write
  failure, existing fatal-error cleanup, and connection ownership tests. These
  ran from the service unit-test binary built by the import test command, to
  avoid waiting again for the shared Cargo build lock.
- The additional GRBL session test run could not start because a separate
  `cargo test --workspace` held the build lock. The queued extra run was stopped;
  no pass is claimed for it.
- `cargo check -p beambench-api -p beambench-service --examples` passed for the
  selected example targets. The full `cargo check -p beambench-tauri` reached
  desktop compilation but was interrupted with exit 137, so a completed desktop
  check is **not** claimed. Its retry was queued behind the separate workspace
  build and was stopped with the blocked extra test run. Complete that native
  check before treating this as release validation.
- `git diff --check` passed. No full-workspace test pass is claimed here.
- Earlier triage ran 33 core PDF tests and 15 frontend import tests successfully;
  temporary reproductions asserting the broken behavior were removed afterward.
  The permanent regressions now assert the corrected behavior.

No original customer PDF, Windows runtime, or physical controller was tested.
No issue comments, customer messages, status changes, releases, or hardware
operations were performed during this review.
