# November 1 feedback fixes

Source reports: [camera #80](https://github.com/schmnr/beambench-feedback/issues/80),
[connection #79](https://github.com/schmnr/beambench-feedback/issues/79),
[tool names #122](https://github.com/schmnr/beambench/issues/122), and the
[AppImage catalog test](https://github.com/AppImage/appimage.github.io/pull/11129#issuecomment-6561393055).

## Changes

- The SHA-verified published 0.2.37 AppImage stores root-owned
  `AppRun.wrapped` with mode `0750`. Normalize package readability and
  executability before signing, and reject inaccessible paths in the audit.
  Regression fixtures cover `0750` and a launcher without execute bits even
  when the runtime compatibility policy is already installed. Retain all
  existing camera, font and update-relaunch checks.
- Linux desktop and agent camera capture use the existing native device
  backend instead of WebKit's camera portal. Native capture runs off the UI
  thread. Linux frame reads have a timeout and warm-up frames, and validate
  the configured pixel format before decoding. Keep macOS and Windows on the
  browser capture path. Failures retain the last good overlay and report the
  device error in diagnostics. This removes the reported portal dependency;
  it does not establish the original host's PipeWire failure cause.
- Device Settings shows the selected local/remote destination, hides local
  connection controls in remote mode, and permits an explicit switch back.
  Its destination selector is shared with the Laser panel. Keep the command
  guard and remote emergency-stop routing.
- Project opening preserves custom tool-layer names and existing entry IDs.
  Regression coverage saves, opens twice and restores recovery for both tool
  colors, checking ordinary layers, a locked reference object's geometry and
  layer visibility. Genuine legacy migration and duplicate merging remain.
  Recovery intentionally marks the restored project unsaved.

## Validation

- Local frontend: 2,877 tests passed; production build, TypeScript and ESLint
  passed.
- Local Rust: camera, service and desktop tests passed. The new persistence
  regression passes; existing legacy migration and camera tests pass.
- The new audit rejects the SHA-verified published 0.2.37 package. Repairing
  a local copy changes the stored root-owned launcher from `0750` to `0755`
  and passes the audit. That repacked copy is unsigned and is not desktop
  startup evidence.
- Required repository CI is attached to [PR #123](https://github.com/schmnr/beambench/pull/123).
- The unpublished signed Linux candidate is `v0.2.38-rc.1`, source
  `616ab0227e28750d5fa2ea1e76f0f6d69b14443b`, built by
  [workflow 41333780820](https://github.com/schmnr/beambench/actions/runs/41333780820).
  Its application source matches the fix commit; only version metadata differs.
  The `BeamBench-Linux-desktop-v0.2.38-rc.1` workflow artifact records the
  exact signed AppImage hash and six actual-desktop startup cases. The workflow
  requires `Frontend ready` on 22.04, 24.04 and 26.04 under both display-variable
  scenarios before uploading the package. Its media check retains host plugin,
  synthetic-frame, font and update-relaunch coverage.
- Native CachyOS/KWin/Wayland, a physical camera and laser hardware are not
  available. Container display-variable checks cannot establish that coverage.
- No release publication, stable-manifest update, reporter correspondence or
  catalog `/retest` is part of this source fix.

## Additional reports: Material Test preview and Falcon A1

[Material Test #124](https://github.com/schmnr/beambench/issues/124) was reproduced
in a separate native macOS debug app with private settings and no connected
hardware. The original preview hid the burn footprint and retained overlapping
motion-envelope rectangles with Overscan off. Its row labels intruded into the
grid.

- Cardinal raster bitmap rendering now follows the planner's world-coordinate
  convention, including vertical transpose, instead of applying placement a
  second time. Non-cardinal local rasters retain their origin/rotation.
- Completed raster regions no longer gain a synthetic motion-envelope border.
  Actual generated vector borders remain; Overscan controls its own overlay.
- Transient timelines with no project layer colors use black instead of white.
- Generated test labels resolve glyph geometry before applying their requested
  right/center and middle/bottom anchors. This corrects emitted label placement
  as well as the display. Whole-test absolute centering includes those labels,
  so their correction can shift the grid's absolute position within the centered
  job. Sample sizes, pitch, powers and feeds remain unchanged.

Validation: 2,934 frontend tests passed; production/native debug build and
TypeScript passed; ESLint passed with its existing TransformSection hook warning.
The service suite passed 905 unit tests (one existing ignored test), two
persistence integration tests and six phase-four integration tests. Targeted
quality-test and preset suites passed. Native inspection covered the reported
3 x 2 grid at 1 mm interval/absolute center (100,100), then 0.11 mm interval with
centering off: six visible fill cells, separate gaps, resolved row/column labels,
Overscan off/on, progress off/on, backward scrub, playback and zoom/pan.
No Frame or Start was executed. Automated preview/export regressions cover
1 mm and 0.1 mm intervals with centering off/on and inspect emitted 8 mm burn
strokes, 11 mm pitch, 1200/1800 feeds and S100/S200/S300. Bitmap/run transform
agreement covers both axes, both viewport Y directions, multiple zooms and
cardinal/non-cardinal angles.

Feedback report `r-9a0c2662b10c42b99d998b9042094352` is a stock **Creality Falcon
A1 10W preset request**, not a connection-failure log. The new version-1 USB
preset uses the manufacturer's published device profile: X 381 x Y 305 mm,
115200 baud and S1000. Homing and automatic air commands remain disabled.
See the [evidence record](../machines/creality-falcon-a1-10w.md) for source hash,
control rationale and limits. Shared planner/emitter and connected-$30 tests
include the new preset; its description is localized in all 23 languages.
Physical A1 connection, motion and output tests remain pending.
