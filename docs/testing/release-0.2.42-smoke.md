# Beam Bench 0.2.42 release validation

- Version / tag: 0.2.42 / v0.2.42.
- Commit: the immutable release tag containing this record.
- Date: November 7, 2026.
- Tester: Beam Bench release automation and local macOS verification.
- Hardware: no physical laser or camera attached.

| Area | Result at source freeze | Notes / final-artifact gate |
| --- | --- | --- |
| GRBL job replies | Pass | The regression on the parent source proves that an idle poll with a stale job snapshot leaves the new job's acknowledgement for the streamer. The test failed before the fix. |
| Long buffered job | Pass | A macOS virtual serial replay acknowledged all 192,892 synthetic lines. With six replies withheld, the existing safety stop reported 112 bytes outstanding. |
| Project safety, undo, raster and preview | Pass | Parent-source automated checks passed. Repeat critical suites on the release tag. Manual project and canvas smoke checks were not run. |
| Placement, Start From and Material Test | Pass | Covered by parent-source planner, emitter and service tests. Physical framing and engraving were not run. |
| Machine state and job lifecycle | Pass | Parent-source service and GRBL suites passed. Physical homing, jogging, alarms and job runs were not run. |
| Localization | Pass | Parent-source frontend checks passed. Manual locale review was not run. |
| Exact release source | Not run | Require release preparation CI and the critical suites on the tag. |
| macOS package and launch | Not run | Require Developer ID signing, notarization, Universal2, CLI checks and isolated packaged startup on Apple Silicon and under Rosetta. |
| Windows package and launch | Not run | Require signed NSIS, updater signature, static runtime checks and two installed desktop launches reaching `Frontend ready`. |
| Linux package and launch | Not run | Require the Ubuntu 22.04 build, launcher and library audit, and the unmodified signed AppImage reaching `Frontend ready` on 22.04, 24.04 and 26.04 in both test environments. |
| Linux media, fonts and update relaunch | Not run | Require the final package's synthetic camera, font and inherited update environment checks. |
| Updates, downloads and source | Not run | Verify signatures, checksums, corresponding source, public URLs and an isolated macOS update from 0.2.41. |
| Native Plasma/Wayland and physical devices | Not run | Mixed display variables in Xvfb cover launcher behavior. Physical Falcon2 Pro 40W confirmation remains pending. |

Publish only after the final package gates pass. Retain final checks and
artifact hashes with the release. The Falcon report captures the end of the
run, so the job-start race is a verified app defect and the cause of that
particular interruption is still open.
