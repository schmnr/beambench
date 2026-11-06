# Beam Bench 0.2.41 release validation

- Version / tag: 0.2.41 / v0.2.41.
- Source: the immutable release tag containing this record.
- Date: November 6, 2026.
- Authorization: the owner requested publication of the framing fixes and the
  new Help link to lessons, videos and practice files.
- Hardware: no physical laser or camera is attached.

| Area | Check | Result at source freeze | Notes / final-artifact gate |
| --- | --- | --- | --- |
| Framing speed | Approach, perimeter and return respect the chosen feed | Pass | GRBL recording-transport and emitter tests; approaches and returns use laser-off controlled travel. GRBL framing restores units-per-minute mode. |
| Scaled artwork | Affine bounds and Current Position placement | Pass | A 400 x 500 mm image scaled to 200 x 250 mm at 100,100 frames at X100–300 and Y100–350. Selected/whole-project and rectangle/hull cases passed. |
| Frame loops | Corner and edge anchors close without retracing a side | Pass | All perimeter origins and both workspace orientations passed. Center/off-hull anchors retain necessary controlled return travel. |
| Stop and repeat | Immediate cancellation, emergency stop and frozen repeat plans | Pass | Existing service regressions passed. No finish-the-loop motion was added after cancellation. |
| Generated output | Duplicate rapids, normal engraving and rotary feed mapping | Pass | GRBL, planner, service, Marlin and Smoothieware suites passed. Ordinary engraving rapid travel is retained. |
| Help | Native and React learning menu, documentation destination and translations | Pass | 167 frontend checks and 15 native-menu tests. English/German menus fit the 900 px desktop minimum. An isolated macOS application opened the live learning page; native ampersands display correctly. All 23 label catalogs are present. |
| Project safety | Save/open, recovery, clear, undo and raster/preview workflows | Pass | Parent-source automated suites passed. Repeat exact-tag critical suites before packaging. Owner data stays outside isolated smoke profiles. |
| Source quality | Exact release-source build/test/lint/Clippy/format | Not run | Require release preparation CI and exact-tag critical suites. Parent PR 137 passed all six jobs; its macOS capacity cancellation passed on retry. |
| macOS packages | Universal2, signing, notarization, CLI and packaged startup | Not run | Require exact signed artifacts and two Apple Silicon plus Intel-under-Rosetta Frontend ready launches. |
| Windows packages | Signed NSIS, updater signature, static runtime and installed desktop | Not run | Require exact-tag workflow evidence, including two installed-desktop Frontend ready launches. MSI not requested. |
| Linux package | Ubuntu 22.04 builder, permissions and library audit | Not run | Require exact-tag release workflow and final signed-artifact audit. |
| Linux startup | Unmodified signed AppImage on Ubuntu 22.04/24.04/26.04 | Not run | Require six X11/mixed-display Frontend ready and continued-execution cases. |
| Linux media/relaunch | Camera plugins, synthetic frames, fonts and inherited update environment | Not run | Retain all final-package workflow gates. Physical camera and native updater UI Not run. |
| Updates and source | Corresponding source, signatures, hashes, URLs and upgrade from 0.2.40 | Not run | Verify immutable assets before stable promotion and exercise the production updater on an isolated macOS copy. |
| Website | Download versions, localized release notes and Learn preservation | Not run | Stage from current committed website source. Verify candidate/public lessons and resources; preserve unrelated training work. |
| Native Plasma/Wayland | Actual KWin/Wayland desktop | Not run | No native session available. Xvfb with mixed display variables does not establish native Wayland coverage. |
| Physical hardware | RAY5/DLC32 motion, engraving and camera | Not run | Confirmed application defects are fixed; the reporter's exact project, firmware behavior and physical travel still need hardware confirmation. |

Final workflow IDs, exact hashes, signatures, source correspondence, package
startup and updater results belong in the published release verification
artifact. This source-freeze record does not claim final packages have passed.
Keep stable unchanged until all required gates pass.
