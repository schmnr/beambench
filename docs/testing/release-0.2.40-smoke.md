# Beam Bench 0.2.40 release validation

- Version / tag: 0.2.40 / v0.2.40.
- Source: the immutable release tag containing this record.
- Date: November 5, 2026.
- Authorization: the owner requested publication of the bug fixes and desktop
  serial number, date/time and CSV personalization features.
- Hardware: no physical laser or camera is attached.

| Area | Check | Result at source freeze | Notes / final-artifact gate |
| --- | --- | --- | --- |
| Reported regressions | Copy Along Path native menu and Art Library vector save/thumbnail | Pass | PR 132 passed required source CI. Existing controls and layout retained. |
| Personalization | Serial/date/CSV previews, ranges, spacing and fixed editable copies | Pass | PR 133 passed all six checks. Frontend workflow exercised with a real isolated native backend. Native-window manual interaction unavailable. |
| Project safety | Save/open generated text, embedded CSV, reusable original, cancel and project changes | Pass | Native backend workflow and frontend/service tests. Settings and all 28 owner recoveries preserved. |
| Undo | Single-step batch undo/redo and counter restoration | Pass | Frontend and service regressions, real-backend workflow. |
| Text data | Quoted/multiline/Unicode CSV, bad headers/rows, finite spacing and date formatting | Pass | Core, service, frontend and native command tests. |
| Localization | All 23 catalogs, native menu contract and desktop controls | Pass | Catalog parity, translated labels and menu tests; visual review of new controls. Broader translated native-window manual checks Not run. |
| Canvas, raster and preview | Clear/recovery, image processing and repeated/cancelled previews | Pass | Parent-source automated suites. Repeat critical release-source gates. Physical output Not run. |
| Placement and machine state | Workspace origins, Start From modes, framing and homing guards | Pass | Parent-source planner/service tests; no machine-control changes in this release. Physical motion Not run. |
| Job lifecycle | Start, pause, resume, cancel, complete and blocked double-start | Pass | Parent-source automated suites. Physical jobs Not run. |
| Source quality | Exact release-source test/build/lint/Clippy/format | Not run | Require release preparation CI and exact-tag critical suites. Parent-source main CI 42143696462 passed all six jobs. |
| macOS packages | Universal2, signing, notarization, CLI and packaged Frontend ready | Not run | Require exact final signed artifacts, isolated Apple Silicon and Intel-under-Rosetta startup. |
| Windows package | Signed NSIS, updater signature, static runtime and installed desktop | Not run | Require exact-tag workflow evidence including two actual Frontend ready launches. MSI not requested. |
| Linux package | Ubuntu 22.04 builder, final signed AppImage and permissions/library audit | Not run | Require exact-tag release workflow and final signed-artifact audit. |
| Linux startup | Actual unmodified signed AppImage on Ubuntu 22.04/24.04/26.04 | Not run | Require six X11/mixed-display Frontend ready and continued-execution cases. |
| Linux media/relaunch | Camera plugins, synthetic capture, fonts and prior update environment | Not run | Retain final-package workflow gates. Physical camera and native updater UI Not run. |
| Updates and source | Source correspondence, hashes, signatures, public URLs and 0.2.39 upgrade | Not run | Verify immutable assets before stable promotion; exercise production updater engine on an isolated macOS copy. |
| Website | Download versions, release notes, source and Learn preservation | Not run | Stage from current website source without unrelated private training changes; verify candidate and public Learn pages/resources. |
| Native Plasma/Wayland | Actual KWin/Wayland desktop | Not run | No native session available. Container X11/mixed-variable tests do not establish native Wayland coverage. |
| Physical hardware | Laser motion, engraving and camera | Not run | No hardware available. This release does not resolve or claim to explain the RAY5 unexpected-movement report. |

Final workflow IDs, exact artifact hashes, signatures, source correspondence,
packaged startup and update results belong in the published release verification
artifact. This source-freeze record does not claim final packages have passed.
Keep stable unchanged until all required gates pass.
