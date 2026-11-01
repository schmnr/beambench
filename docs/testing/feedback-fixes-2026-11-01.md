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
