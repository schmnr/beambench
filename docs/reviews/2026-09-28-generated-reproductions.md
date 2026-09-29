# Generated bug-report reproductions, 28 September 2026

This investigation uses generated files, native LightBurn 1.6.03, and injected
transport failures. It does not depend on obtaining the reporters' files.
Baseline: commit `27588069`, following the earlier feedback corrections.

## Bitmap orientation: reproduced and corrected

The earlier MGL review did not detect an inversion because it compared retained
pixels and object transforms without an asymmetric native output reference.
A generated 4 x 3 staircase exposes the difference. LightBurn displays and
exports the first embedded PNG row at local negative Y. Beam Bench applied the
same local Y correction used for text and semantic shapes, reversing the bitmap.

The bitmap importer now composes the source matrix with the world-to-canvas
matrix directly. Text and primitive shape conversion retain their existing
coordinate handling. Pixels and PowerScale values are unchanged.

Native source, saved mirror conversions, and exported G-code are preserved in
[the orientation fixtures](../../crates/beambench-core/tests/fixtures/lbrn/orientation/README.md).
The reference export burns 10 mm near Y65, 20 mm near Y80, and 30 mm near Y95.
This establishes orientation independently of Beam Bench's own implementation.
Native mirror conversions preserve the embedded PNG pixels and negate the
corresponding matrix axes. Absolute placement across different machine bed
sizes was not established by the native single-image check.

The generated service matrix tests both supported workspace origins, both
LightBurn formats, four root mirror combinations, eight object transforms
including rotations, reflections and shear, and a translated group. It checks
black and white cell centers in the actual planned scanlines, 128 imports and
1,536 pixel probes. This is a raster-output check, not just a parser check.

Already imported bitmaps need re-importing from the original LightBurn file.
Automatically flipping all saved Beam Bench images would also affect ordinary
image imports and users' manual corrections. The earlier MGL PowerScale fix
also requires re-importing files whose scales were previously discarded.

## Connection recovery: initial-batch cleanup gap

Later streaming errors already clear the failed job and disconnect the session.
The initial `JobController::start` call first changes the session to Running,
then sends its initial batch. If that send fails, the service returned early
before storing the job and left the session behind. Framing used the same path.

Both GRBL entry points now retain the failed attempt's diagnostics and invoke
the existing stop/disconnect cleanup. The regression injects failure on the
first or third write for both engraving and framing, checks successful-send
counts and handle release, then installs a fresh connection and starts a new
job. It does not resume the failed job automatically.

A separate test denies two opens with the reported Portuguese access-denied
wording, verifies that neither attempt sends commands, and then validates a
fresh bannerless status response after the port becomes available. Existing
mid-job read/write failure and connection-ownership tests remain relevant.
Portuguese port-unavailable guidance is now translated instead of appearing in
English. These simulations do not establish the Windows USB-driver cause.

## PDF compatibility: compressed objects, forms, images and clipping

The mixed-artwork importer now accepts compressed object storage and nested Form
XObjects. Forms inherit the caller's graphics state, compose their matrices,
respect their own resource dictionaries, and clip to their BBox and the active
page/path clip. Cycles, excessive nesting, and unsupported compositing fail the
whole import. Unreferenced forms do not become artwork.

Embedded images use the existing raster import and planning pipeline. Supported
encodings include 8-bit DeviceGray/RGB/CMYK samples, 1-bit DeviceGray, PNG-style
prediction for 8-bit samples, and ordinary RGB/grayscale JPEG. Image dimensions,
Decode values, physical placement, reflections, and shear are preserved. PDF
coordinates convert to Beam Bench's top-left canvas for both images and vectors.

Image clipping remains vector geometry. Baking the clip into the original pixels
would incorrectly crop large, low-resolution pixels at their centers. Clipped
images instead receive a mask on a hidden, non-output Tool layer and are grouped
with that mask. The existing preview and planner apply it at display/engraving
resolution, including holes. Original image samples remain available for image
adjustments. Fully clipped images are omitted.

File import, PDF-compatible AI import, and the art library share this path.
Vector color routing and image-layer routing retain their existing behavior.
The import stages vectors, image assets, masks and layers together, commits one
undo snapshot, and rolls everything back on a later failure. Unsupported PDFs
with an AI extension no longer fall through to the EPS parser.

The two generated mixed PDF fixtures use classic and compressed-object storage.
Their references were rendered independently with Poppler. The comparison covers
nested resource-name overrides, form matrices/BBoxes, even-odd clipping holes,
clipped curves/strokes, mirrored/sheared bitmaps, and an image clip that crosses
source pixels. Service tests also probe the actual planned scanlines under both
workspace origins, exercise all three import entry points, and verify undo/redo.
Geometry probes use threshold mode so error-diffusion dithering cannot turn a
single sampled burn dot into a false clipping failure.
See [PDF fixtures](../../crates/beambench-core/tests/fixtures/pdf/README.md).

### Resource limits and parser patch

The importer retains the 64 MiB source limit and 32 MiB aggregate expanded-content
budget. Object streams are deferred, checked against their authoritative xref,
and charged before parsing. Individual object and xref streams are capped at
1 MiB. The document also limits object counts, drawing operations, form depth,
graphics-state depth, clipping geometry, and aggregate image pixels. Repeated
forms and images consume the same document budgets.

The vendored lopdf 0.44.0 patch makes these controls effective before the relevant
allocations. Its restricted reader rejects encryption before decryption or
metadata lookup, object parsing respects each compressed object's byte slice,
PNG predictor rows respect the decode limit, and content decoding stops at the
operation limit and rejects malformed trailing content. The patch is documented
in [BEAMBENCH-PATCH.md](../../vendor/lopdf/BEAMBENCH-PATCH.md). These are decoding
and complexity limits, not a measurement of total process RSS.

Encrypted/multipage PDFs, page rotation/cropping/custom units, text operators,
transparency groups, image masks/soft masks, non-device color spaces and actual
dashed strokes remain explicit compatibility limits. This imports editable laser
geometry and rasters; it does not implement every PDF painting feature. Vector
stroke widths retain the existing centerline-import contract. The synthetic
fixtures establish support for these cases, not the contents of missing customer
files from reports #43-45.

## Native serial and Linux diagnostics coverage

A Debian 12 ARM64 container, running as an unprivileged user on Docker's Linux
kernel, passed the real serial-transport suite. The added PTY test opens a real
terminal device at 115200 baud, denies a competing exclusive opener, exchanges
bytes, clears an incomplete response across close/reopen, and surfaces read and
write errors after removal. A separate real filesystem permission denial checks
the actionable error mapping. This validates Linux OS behavior without a USB
controller.

The Linux service diagnostics also ran against real `/proc`: a temporary renamed
sleep process exercises BRLTTY-name detection without launching BRLTTY or opening
USB devices. Existing tests verify Mint release metadata, installed/running
states, unavailable probes, and bounded metadata reads. No service is disabled.

Windows-native tests now exercise real access denial, missing-device errors,
`ClearCommError` failure, and `WriteFile` failure through Win32 handles. They
compile for `x86_64-pc-windows-msvc`; the Windows CI job now includes the serial
crate so those tests run on its Windows host. No Windows host was connected in
this session, and the remote workflow has not been dispatched. These tests do
not impersonate CH340/FTDI driver behavior.

Physical CH340/FTDI unplug/replug and actual BRLTTY USB-interface claiming on Mint
remain unverified. Automated tests and read-only diagnostics improve recovery
and evidence collection; they cannot establish the original hardware failure.

## Validation after the PDF extensions

- All 959 core library tests passed, including 51 PDF tests for the supported
  encodings, clipping, transforms, malformed content and resource limits.
- The PDF integration test passed against both independent Poppler references,
  including image clipping across original pixel boundaries.
- All 712 service-operation tests passed, including mixed PDF/AI/art-library
  imports, planned image pixels and clip holes, both workspace origins,
  image-only layer routing, atomic rollback, undo/redo and connection recovery.
  The final service suite ran directly from the Cargo-built test binary after
  the focused clipped-image regression passed; subsequent source changes were
  Rust formatting only.
- All 24 serial tests passed on native Linux in the unprivileged Debian
  container. Three service-diagnostics tests passed in a Linux validation crate
  that includes the actual service source, including real process detection.
- Windows serial library and test code passed
  `cargo check -p beambench-serial --tests --target x86_64-pc-windows-msvc`.
  This is a compilation check, not execution on Windows.
- Clippy completed for core, service and serial libraries/tests with existing
  warnings. Workspace Rust formatting and `git diff --check` passed.

These checks cover the changed paths. They do not constitute a full workspace
test run, packaged-app smoke test, or physical USB validation. PDF decoding and
complexity guards remain enabled.

## Earlier validation, before the PDF extensions

- 709 service-operation tests passed, including the 128-case bitmap matrix,
  initial engraving/framing failures, reconnect, and existing failed-import
  rollback and mid-job failure coverage.
- 40 PDF unit tests passed, including split/compressed page content, classic
  versus compressed object storage, solid dash resets, and retained rejection
  of unsupported artwork and excessive decompression.
- All 200 GRBL tests and 23 serial tests passed. Serial coverage includes the
  localized Windows error mapping and a real macOS pseudo-terminal byte exchange.
- All 45 locale-parity tests passed, including interpolation consistency.
- Clippy completed for core and service libraries and tests. Existing warnings
  remain; none point to the added code. Rust formatting and `git diff --check`
  passed.

Total: 1,017 tests passed. Native LightBurn reference checks supplement the
suite. This is scoped validation, not a packaged release or physical Windows /
Linux Mint USB-controller test. The PDF library suite was run directly after
its Cargo invocation finished those tests and stalled launching unrelated,
fully filtered integration binaries; the direct unit run exited successfully.

No hardware was operated, no reports were closed, and no release was published.
