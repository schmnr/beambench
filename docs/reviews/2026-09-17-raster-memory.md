# Raster memory and feedback #41

Feedback [#41](https://github.com/schmnr/beambench-feedback/issues/41), report `r-1e12fbdb1a464517937c1c563e2dd4b4`, reached the one-million-run image limit during preview in 0.2.24. Its project and image settings were not attached. The measurements below use synthetic checkerboards, not the missing customer image.

## Change

Image scanlines now share processed pixel data. Consumers expand one row's burn runs at a time. Coordinate translation, reflection, reversal, and dot-width correction retain their original order and arithmetic. Existing serialized scanline arrays remain compatible. Explicit vector-fill runs share immutable storage and copy only when edited.

Travel insertion moves segments instead of copying them twice. Preview and statistics share the cached execution plan, and unchanged display coordinates borrow that plan. Unmasked images borrow processed pixels. Each raster cache has both an entry limit and a 64 MiB pixel allocation budget. Eviction leaves images currently in use valid.

GRBL preparation writes and validates commands into an automatically removed temporary file. Streaming retains the next command, the bounded acknowledgement window, and the latest 1,000 console entries. Pause/resume and error attribution retain the same command indices. Preparation completes before starting motion. Export uses a temporary destination and replaces the requested file only after a successful copy.

The work limits are now:

- Four million burn runs per image angle pass, up from one million.
- Eight million image burn runs and 128 MiB of retained image pixel buffers per plan, counted across images and angle passes.
- Image dimensions of at most 65,536 pixels per axis before scan rotation, bounding row expansion.
- A 512 MiB GRBL spool file budget. Legacy adapters that request a complete command list have a 256 MiB allocation estimate limit.

These limits bound individual components, not total application RSS. Existing controller-specific limits remain in force.

## Measurements

Measured on this Mac with debug builds and `/usr/bin/time -l`. MB below means 1,000,000 bytes. The benchmark includes image creation, planning, three live logical plan copies, preview generation, and controller output. GRBL cases also read the spool to hash every command. Wall times include concurrent builds and macOS executable-loading delays, so they are not useful speed comparisons.

| Case | Burn runs | Output | Peak resident memory |
| --- | ---: | ---: | ---: |
| Before, base `e065f29d`, GRBL | 1,000,000 | 2,002,008 commands | 288.4 MB |
| After, GRBL | 1,000,000 | 2,002,008 commands | 26.8 MB |
| After, GRBL | 4,000,000 | 8,008,008 commands | 89.4 MB |
| After, grayscale GRBL | 4,000,000 | 8,008,008 commands | 120.9 MB |
| After, Ruida | 4,000,000 | 88,088,369 bytes each for clear and RD output | 527.9 MB |
| After, Lihuiyu | 4,000,000 | Rejected by existing motion-expansion ceiling | 187.2 MB |

The one-million-run GRBL output matches the original command hash exactly:

```text
526506b4762cb61c50d2e49cb0dd97c21c4f6644aba241a4cfa8ad8e397a89a7
```

The four-million-run GRBL command hash is:

```text
5aa4bc2bdf2d52db1793495b24727609e528454d76480fba578956ce7617405a
```

GRBL memory at one million runs falls by about 91%. The compact representation trades repeated row expansion and disk I/O for lower RAM use. Ruida still retains native motion and both encoded output buffers, so its peak is higher. Lihuiyu's separate motion ceiling is not raised.

Reproduce the current benchmark:

```sh
cargo build -p beambench-planner --example raster_memory
/usr/bin/time -l target/debug/examples/raster_memory 1000
/usr/bin/time -l target/debug/examples/raster_memory 4000
/usr/bin/time -l target/debug/examples/raster_memory 4000 gray
/usr/bin/time -l target/debug/examples/raster_memory 4000 ruida
/usr/bin/time -l target/debug/examples/raster_memory 4000 lihuiyu
```

## Verification scope

Regression coverage checks compact/expanded row equivalence, coordinate transforms, grayscale powers, serialization, exact guard boundaries, combined plan budgets, cache eviction, shared plan ownership, spool output equivalence, sink failures, invalid commands, bounded logs, and command order across buffered/synchronous pause and resume. A service regression previews and prepares export for a 1.1-million-run image through the application service path.

Local validation passed. Across the combined suite and targeted reruns, 3,049 distinct Rust unit/integration tests passed; two tests remained skipped. The four API preview/export tests also passed in their earlier targeted run and are included in the broader API coverage.

The combined core, planner, raster, preview, controller, streamer, service, API, and CLI suite first passed 2,361 tests before the large-checkerboard fixture failed, leaving 687 tests unrun. The service, streamer, Smoothieware, and xTool rerun passed 887 tests, including 200 already covered, and exposed a second mismatch in that same fixture. Its image bounds and project bed now both fit the default 200 mm machine bed. The final targeted checkerboard regression passed, retaining the same 1.1 million burn runs and checking both preview and prepared export.

The workspace check, including Tauri, passed. Workspace clippy completed with warnings; it was not a warning-free run. The service restart regression for transform-anchor persistence passed. macOS substantially delayed loading newly compiled test binaries and compiler dependencies; a compiler sample showed the dynamic loader waiting in code-signature file mapping. A separate service-test sample was still in `_dyld_start`, before test code. Formatting and whitespace checks passed.

Physical laser operation and a native Windows runtime are not covered by these local checks. The original customer image remains unavailable. No release or deployment is performed by this change.
