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
- Repository CI and final signed Linux candidate validation are pending.
- Native CachyOS/KWin/Wayland, a physical camera and laser hardware are not
  available. Container display-variable checks cannot establish that coverage.
- No release publication, stable-manifest update, reporter correspondence or
  catalog `/retest` is part of this source fix.
