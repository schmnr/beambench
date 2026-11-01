# Beam Bench 0.2.38 release validation

- Version / tag: 0.2.38 / v0.2.38.
- Source: the immutable release tag containing this record.
- Date: November 1, 2026.
- Authorization: the owner requested publication of all fixes in PRs 123 and 125.
- Hardware: no physical laser or camera is attached to these checks.

| Area | Check | Result at source freeze | Notes / evidence required before publication |
| --- | --- | --- | --- |
| Reported regressions | Launcher, Linux camera, remote destination, tool names, Material Test, Falcon A1 | Pass | Parent-source PRs 123 and 125 passed required CI. See feedback-fixes-2026-11-01.md. Repeat exact release-source checks. |
| Launch | Signed package starts and reaches the actual frontend twice | Not run | Final macOS app and Windows installed app; Linux final AppImage on all three Ubuntu versions. |
| Project safety | New/Open/Close, unsaved prompts, recent files and recovery | Pass | Parent-source native preview QA retained unsaved prompts. Persistence, recovery and project lifecycle suites passed. Final packaged startup remains a gate. |
| Canvas safety and undo | Create/delete/transform/clear, recovery and import | Pass | Parent-source workspace regressions. Repeat on release source; no physical output claim. |
| Raster | Image processing, normal/negative output and repeated/cancelled preview | Pass | Parent-source automated processing and preview suites. Repeat exact-source regressions. Physical output Not run. |
| Material Test preview | Coarse/fine interval, center on/off, progress, overscan, scrub and zoom | Pass | Native macOS QA on the fixed parent source. Six separate cells and resolved labels; emitted 8 mm strokes, 11 mm pitch and selected powers/feeds verified automatically. |
| Placement and Start From | Workspace origins and Absolute/Current/User Origin transforms | Pass | Parent-source planner/service tests compare actual emitted coordinates. Repeat exact release-source tests. |
| Material Test Frame/Start | Same bounds and anchor in each Start From mode | Pass | Automated planner/service regression paths. Physical Frame and Start Not run. |
| Machine state | Home/origin/jog/alarm/unlock/disconnect/reconnect | Pass | Protocol, simulated-controller and service tests on parent source. Physical machine actions Not run. |
| Job lifecycle | Start/pause/resume/cancel/complete and blocked double-start | Pass | Parent-source automated streamer/service coverage; repeat exact source. Physical jobs Not run. |
| Localization | Locale catalog parity, native script/layout/persistence smoke | Not run | 0.2.38 frontend tests pass; native pre-tag checks and final limits go in the verification artifact. |
| Dependency security | Full and production npm audits | Not run | Require zero findings after the minimatch 11.0.1 tooling override. |
| Release source | Required CI, critical source tests, clippy and formatting | Not run | All required checks on release preparation and the exact tagged main source must pass. |
| macOS package | Universal2, signing, notarization, entitlement, Gatekeeper, app and CLI | Not run | Final signed package plus isolated startup and Intel-under-Rosetta checks. |
| Windows package | Signed NSIS, updater signature and installed desktop startup | Not run | Valid Authenticode; both Frontend ready callbacks; native credentials and runtime isolation. MSI not requested. |
| Linux package | Ubuntu 22.04 builder, stored permissions and host-library audit | Not run | Audit final signed bytes; reject inaccessible launchers and conflicting bundled libraries. |
| Linux startup | Actual signed unmodified AppImage on 22.04/24.04/26.04 | Not run | Six Frontend ready and continued-execution cases under X11 and mixed display variables. |
| Linux media and updates | Host camera plugins, synthetic capture, fonts and inherited relaunch | Not run | Retain all final-package regressions. Physical camera and native updater UI Not run. |
| Falcon A1 preset | Documented defaults, workspace, S scale and optional commands | Pass | Real planner/emitter and connected-$30 regressions on parent source. Physical A1 validation Not run. |
| Updates and source | Exact-version source, hashes, signatures, URLs and upgrade from 0.2.37 | Not run | Verify all final artifacts; exercise production updater engine on an isolated macOS copy before stable promotion. |
| Downloads | Website version, platform packages and exact-version source link | Not run | Update only after release and stable updater publication; validate public URLs and enabled locales. |
| Native KWin/Wayland | Actual Plasma Wayland session | Not run | No native session available. Xvfb mixed-variable checks do not establish native Wayland coverage. |
| Physical hardware | Laser motion/output and camera capture | Not run | Hardware and reporting machines unavailable. Camera portal bypass and Falcon A1 defaults are software/documentation evidence only. |

Final workflow IDs, signatures, hashes, source correspondence, native smoke
results and public URL checks belong in the release's verification artifact.
This source-freeze record does not claim that final packages have passed.
Preserve previous packages and tags; do not promote stable before the gates pass.
