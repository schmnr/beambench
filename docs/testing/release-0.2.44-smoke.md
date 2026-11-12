# Beam Bench 0.2.44 release validation

- Version / tag: 0.2.44 / v0.2.44.
- Commit: the immutable release tag containing this record.
- Date: November 11, 2026.
- Tester: Beam Bench release automation and local macOS verification.
- Hardware: no physical laser or camera attached.

| Area | Result at source freeze | Notes / final-artifact gate |
| --- | --- | --- |
| AtomStack USB presets | Pass | All 36 configurations pass real profile, workspace, vector and raster output tests. Tests check dimensions, power scale, M3/M4 and exact air-command gating. The 33 new entries are based on manufacturer documentation; physical confirmation remains pending. |
| Remembered USB speed | Pass | A regression reproduced the old saved baud after applying Swift. Tests cover preview, persistence, 460800 discovery, desktop reopen/connect and unchanged port, driver and network preferences. |
| SVG color import and export | Pass | Automated cases cover color routing, existing cut settings, multicolor/reused text, labels, tool layers and color-preserving export/import. |
| Group positioning | Pass | Transformation X/Y edits move grouped artwork and descendants with their selection bounds in automated tests. |
| Continuous framing | Pass | Automated service and desktop tests cover Finish Framing, completing the current lap and return, Start gating, and immediate red Stop. Physical framing was not run. |
| Resized outlines and Tabs | Pass | Automated geometry, planner, service and desktop tests cover rounded/concave rectangle resizing, older projects, cloning, current click hits, stale selection and low-zoom removal. |
| Creality original Falcon presets | Pass | Automated profile and output checks cover the original 5W, 7.5W and 10W entries. Physical confirmation remains pending. |
| Project safety, canvas, undo, raster and preview | Pass | Parent-source checks passed all 3,872 Rust and 3,056 frontend tests. Repeat release preparation CI and critical suites on the tag. Manual canvas and project workflows were not run. |
| Placement, Start From and Material Test | Pass | Covered by automated planner and service tests. Physical framing and engraving were not run. |
| Machine state and job lifecycle | Pass | Automated service and streaming tests passed. Physical homing, jogging, alarms and jobs were not run. |
| Localization | Pass | Locale parity, ICU validation and AtomStack guidance checks cover all 23 languages. Native visual checks were not run. |
| Exact release source | Not run | Require release preparation CI and the critical suites on the tag. |
| macOS package and launch | Not run | Require Developer ID signing, notarization, Universal2, CLI verification and isolated packaged startup on Apple Silicon and under Rosetta. |
| Windows package and launch | Not run | Require signed NSIS, updater signature, static-runtime audits and two installed desktop launches reaching Frontend ready. |
| Linux package and launch | Not run | Require the Ubuntu 22.04 builder, launcher/library audit and the unmodified signed AppImage reaching Frontend ready on Ubuntu 22.04, 24.04 and 26.04 in both test environments. |
| Linux media, fonts and update relaunch | Not run | Require final-package synthetic camera, font and inherited update-environment checks. |
| Updates, downloads and corresponding source | Not run | Verify final signatures, hashes, source archives, public URLs and an isolated macOS update from 0.2.43. |
| Native Plasma/Wayland and physical machines | Not run | Xvfb tests exercise display fallback policy. No physical AtomStack or Creality laser, camera or native KWin/Wayland desktop is available. |

Publish after the final package gates pass. Preserve exact artifact hashes and
verification evidence with the release. The source-freeze results above do not
claim that the final packages or physical machines have already been tested.
