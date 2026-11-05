# Beam Bench 0.2.39 release validation

- Version / tag: 0.2.39 / v0.2.39.
- Source: the immutable release tag containing this record.
- Date: November 5, 2026.
- Authorization: the owner requested publication of the merged fixes and
  framing diagnostics so further reports can help investigate the RAY5 case.
- Hardware: no physical laser, RAY5/DLC32, 3018 or camera is attached.

| Area | Check | Result at source freeze | Notes / final-artifact gate |
| --- | --- | --- | --- |
| Reported regressions | SVG encoding, disabled homing, homed XY return and framing evidence | Pass | PRs 127, 128, 129 and 130 passed required source CI. Repeat critical suites on the release tag. |
| Project safety | New/Open/Close, unsaved work, recovery and editable SVG/SVGZ import | Pass | Parent-source project/service coverage. Final packaged startup and import smoke pending. |
| Canvas and undo | Create/delete/transform/clear and undo/redo | Pass | Parent-source automated suites. No output hardware claim. |
| Raster and preview | Processing, inversion, repeat/cancel preview and emitted geometry | Pass | Parent-source automated suites. Repeat critical release-source checks. Physical output Not run. |
| Placement | Workspace origins and Absolute/Current/User Origin | Pass | Planner/service regressions compare emitted endpoints and frame/job placement. |
| Non-homed framing | All nine job origins, two frame shapes and three starting positions | Pass | 54 simulated cases. The supplied rectangle project also generated and completed expected framing in simulation. No physical-clearance claim. |
| Homing and return | Reject `$22=0`; capture valid homing endpoint; return XY without Z | Pass | Session/service regressions and isolated native desktop simulator workflow. Known negative endpoint and changed Z preserved. |
| Frame diagnostics | Pre-dispatch refusal, streaming failure, complete/cancel/Stop/Disconnect | Pass | Seven added service regressions; preserved report/ZIP trace, home-path scrubbing and bounded memory/body size. |
| Job lifecycle | Start/pause/resume/cancel/complete, stop and blocked double-start | Pass | Parent-source streamer/service suites. Physical jobs Not run. |
| Localization | Catalog parity, new return-control labels and native layout/persistence | Pass | Parent-source frontend coverage and English/German native control smoke. Applicable release-source native script/menu smoke pending. |
| Source quality | Exact release-source tests, lint, Clippy and format | Not run | Require release preparation CI and exact tagged-source critical suites. |
| Dependency security | npm full and production audit | Pass | Full app audit has zero findings at preparation. Repeat locked install checks in packaging. |
| macOS packages | Universal2, signing, notarization, Gatekeeper, CLI and app startup | Not run | Final signed artifact, two isolated Frontend ready launches and Intel-under-Rosetta startup required. |
| Windows package | Signed NSIS, updater signature, native credentials and installed desktop | Not run | Exact tag workflow must pass signatures, dependency audit and two actual Frontend ready launches. MSI not requested. |
| Linux package | Ubuntu 22.04 builder, signed artifact, permissions and host-library audit | Not run | Unpublished 0.2.39-rc.1 package validation precedes final-tag packaging. Require final unmodified signed artifact checks too. |
| Linux startup | Actual signed unmodified AppImage on Ubuntu 22.04/24.04/26.04 | Not run | Require all six X11/mixed-display Frontend ready and continued-execution cases. |
| Linux media/relaunch | Camera plugins, synthetic capture, fonts and inherited update environment | Not run | Retain final-package workflow gates. Physical camera and native updater UI Not run. |
| Updates and source | Source correspondence, hashes, signatures, public URLs and 0.2.38 upgrade | Not run | Verify all immutable versioned assets before stable promotion; test production updater engine on an isolated macOS copy. |
| Website | Versioned downloads, release notes and exact-version source link | Not run | Stage independently of unrelated website work; promote after packages and stable updater are public. |
| Native Plasma/Wayland | Actual KWin/Wayland desktop | Not run | No native session available. Container mixed-variable checks do not establish native Wayland coverage. |
| Physical hardware | RAY5/DLC32 unexpected movement, 3018 motion and camera | Not run | The reported unexpected movement remains unresolved. Diagnostics are not a confirmed firmware/motion fix. |

Final workflow IDs, artifact hashes, signatures, source correspondence, packaged
startup and update results belong in the published release verification artifact.
This source-freeze record does not claim that final packages have passed. Keep
the previous release available and leave stable unchanged until required gates pass.
