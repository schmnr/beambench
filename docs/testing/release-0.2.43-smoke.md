# Beam Bench 0.2.43 release validation

- Version / tag: 0.2.43 / v0.2.43.
- Commit: the immutable release tag containing this record.
- Date: November 7, 2026.
- Tester: Beam Bench release automation and local macOS verification.
- Hardware: no physical laser or camera attached.

| Area | Result at source freeze | Notes / final-artifact gate |
| --- | --- | --- |
| Falcon2 Pro 40W buffered USB streaming | Pass | The virtual serial replay reproduced the lost acknowledgments with the old timing. The revised timing acknowledged all 10,005 commands without overlapping a routine status check with pending replies. Physical Falcon retest remains pending. |
| PDF import | Pass | Tests exercise the chosen page through both desktop import commands, single-page import, cancellation, invalid page numbers, zero readable pages, and document replacement. Tests use sample PDFs because the reported file was unavailable. |
| Project safety, undo, raster, and preview | Pass | Automated checks on the parent source passed. Repeat the critical suites on the release tag. Manual project and canvas checks were not run. |
| Placement, Start From, and Material Test | Pass | Covered by automated planner and service tests. Physical framing and engraving were not run. |
| Machine state and job lifecycle | Pass | Automated service and streaming tests passed. Physical homing, jogging, alarms, and job runs were not run. |
| Localization | Pass | Locale parity and ICU checks passed for all 23 languages. The PDF dialog was checked in both themes, including a long filename and the expanded test locale. |
| Exact release source | Not run | Require release preparation CI and the critical suites on the tag. |
| macOS package and launch | Not run | Require Developer ID signing, notarization, Universal2, CLI checks, and isolated packaged startup on Apple Silicon and under Rosetta. |
| Windows package and launch | Not run | Require signed NSIS, updater signature, static runtime checks, and two installed desktop launches reaching `Frontend ready`. |
| Linux package and launch | Not run | Require the Ubuntu 22.04 build, launcher and library audit, and the unmodified signed AppImage reaching `Frontend ready` on 22.04, 24.04, and 26.04 in both test environments. |
| Linux media, fonts, and update relaunch | Not run | Require the final package's synthetic camera, font, and inherited update environment checks. |
| Updates, downloads, and source | Not run | Verify signatures, checksums, corresponding source, public URLs, and an isolated macOS update from 0.2.42. |
| Native Plasma/Wayland and physical devices | Not run | Mixed display variables under Xvfb cover launcher behavior. Physical Falcon2 Pro 40W confirmation remains pending. |

Publish after the final package gates pass. Retain exact artifact hashes and
verification evidence with the release.
