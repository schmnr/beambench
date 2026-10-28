# Beam Bench 0.2.35 release validation

- Version / tag: 0.2.35 / v0.2.35.
- Source: the immutable release tag containing this record.
- Date: October 28, 2026.
- Authorization: owner requested dependency fixes, repository and documentation
  polish, the companion-site update, and publication of the latest version.
- Hardware: no physical laser or camera is attached to these automated checks.

| Area | Check | Result at source freeze | Evidence required |
| --- | --- | --- | --- |
| Dependencies | Rustls 0.23.46 and GLib upstream security backport | Pass locally | Optimized original GLib probe exits 139; backported probe returns `variant_str_iter=ok`. CI repeats the fixed probe on Linux. |
| Source | Required CI, geometry, machine-service and frontend tests | Not run | Exact release-source checks must pass before publication. |
| Windows runtime | CLI and actual desktop without an external VC runtime | Not run | PE import audit, blocked-runtime startup, both Frontend ready callbacks and static-runtime geometry/host tests. |
| macOS | Universal2, Developer ID, notarization and actual startup | Not run | Final signed app and CLI, Gatekeeper and Frontend ready. |
| Windows package | Signed installer, installed startup and credential probe | Not run | Valid Authenticode and updater signatures; installed Frontend ready. |
| Linux package | Ubuntu 22.04 build and signed final-artifact audit | Not run | Retain the glibc floor and reject conflicting bundled host libraries. |
| Linux startup | Actual signed desktop on 22.04, 24.04 and 26.04 | Not run | Frontend ready and continued execution under X11 and mixed display variables. |
| Linux media | Camera plugins, synthetic frames, fonts and update relaunch | Not run | Existing final-package regression checks must pass. |
| Remote jobs | Independent execution, reconnect, ownership and Start replay guards | Not run | Exact-source API and executable host regressions; simulated controllers only. |
| Updates | Corresponding source, public URLs and upgrade from an older build | Not run | Matching signatures and successful isolated update/relaunch before stable publication. |
| Companion site | Release notes, host guide, downloads and all enabled locales | Not run | Build/link/locale validation and final public version/URL checks after app publication. |
| Native KDE Wayland | Actual KWin/Wayland session | Not run | No native session available; Xvfb fallback tests do not establish this coverage. |
| Physical hardware | Laser motion/output and physical camera capture | Not run | Hardware unavailable; simulated tests do not establish physical compatibility. |

Final hashes, workflow IDs and completed test results belong in the release's
verification artifact. This source-freeze record is not a claim that the final
packages passed. Keep previous packages and tags immutable.
