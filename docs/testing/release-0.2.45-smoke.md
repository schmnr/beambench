# Beam Bench 0.2.45 release validation

- Version / tag: 0.2.45 / v0.2.45.
- Commit: the immutable release tag containing this record.
- Date: November 14, 2026.
- Tester: Beam Bench release automation and local macOS verification.
- Hardware: no physical laser or camera attached.

| Area | Result at source freeze | Notes / final-artifact gate |
| --- | --- | --- |
| Artilume presets | Pass | All 18 T1, T1-P, U1 and A4 module configurations pass real profile application, persistence, workspace, preview, vector and raster output tests. Independent expectations cover model-specific X/Y travel, homing flags, M4, S1000 and absence of automatic air or homing commands in output. Manufacturer documentation and QR-linked setup videos establish the starting settings. Physical testing is pending. |
| Connected-controller settings | Pass | Applying every new preset retains the live controller's reported power scale. Automatic homing remains an owner preference. USB settings, existing machine profiles and accessories require their documented setup. |
| Test-settings isolation | Pass | The copied integration-executable regression reproduces the Cargo 1.100 layout failure before the correction and passes afterward. Tests cover both Cargo layouts, inherited settings/data preservation, and normal application paths. Native Windows source CI confirms the correction. |
| Project safety, canvas, undo, raster and preview | Pass | Parent-source CI passed 3,875 Rust and 3,098 frontend tests. Manual canvas and project workflows were not run. Repeat release preparation CI and critical suites on the immutable tag. |
| Placement, Start From and Material Test | Pass | Automated planner and service cases passed. Physical framing and engraving were not run. |
| Machine state and job lifecycle | Pass | Automated service, controller and streaming cases passed. Physical homing, jogging, alarms and jobs were not run. |
| Localization | Pass | Preset guidance, locale parity and ICU checks cover all 23 languages. Native visual checks were not run. |
| Exact release source | Not run | Require release preparation CI, exact-main CI and the critical suites on the tag. |
| macOS package and launch | Not run | Require Developer ID signing, notarization, Universal2, CLI verification and isolated packaged startup on Apple Silicon and under Rosetta. |
| Windows package and launch | Not run | Require signed NSIS, updater signature, static-runtime audits and two installed desktop launches reaching Frontend ready. |
| Linux package and launch | Not run | Require the Ubuntu 22.04 builder, launcher/library audit and the unmodified signed AppImage reaching Frontend ready on Ubuntu 22.04, 24.04 and 26.04 in both test environments. |
| Linux media, fonts and update relaunch | Not run | Require final-package synthetic camera, font and inherited update-environment checks. |
| Updates, downloads and corresponding source | Not run | Verify final signatures, hashes, source archives, public URLs and an isolated macOS update from 0.2.44. |
| Native Plasma/Wayland and physical machines | Not run | Xvfb tests exercise display fallback policy. No physical Artilume laser, camera, native KWin/Wayland desktop or Intel Mac is available. |

Publish only after the final package gates pass. Retain exact artifact hashes
and final verification evidence with the release. The source-freeze results do
not claim that final packages or physical machines have already been tested.
