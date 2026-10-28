# Beam Bench 0.2.36 release validation

- Version / tag: 0.2.36 / v0.2.36.
- Source: the immutable release tag containing this record.
- Date: October 28, 2026.
- Authorization: owner requested publication of both connection fixes and an
  update to the companion site.
- Hardware: no physical laser or camera is attached to these automated checks.

| Area | Check | Result at source freeze | Evidence required |
| --- | --- | --- | --- |
| Connection selection | Missing saved port, returning port and usable Disconnect | Pass on parent source | Connection-dialog regressions from PR 116. Exact release-source CI must repeat them. |
| Windows discovery | Merge registry-only COM ports independently of discovery counts | Pass on parent source | Native Windows serialport tests from PR 116. Preserve USB metadata and avoid duplicates. |
| GRBL detection | Rejected Windows settings, later baud success, reset recovery and fatal errors | Pass on parent source | Six native Windows regressions from PR 117, including non-resetting auto-detection and original-error retention. |
| Source | Required CI, project safety, undo, raster, previews, placement, material tests and job lifecycle | Not run | Exact release-source checks must pass before publication. These are automated tests, not physical output validation. |
| Windows runtime | CLI and actual desktop without an external VC runtime | Not run | PE import audit, blocked-runtime startup, both Frontend ready callbacks and static-runtime geometry/host tests. |
| macOS | Universal2, Developer ID, notarization and actual startup | Not run | Final signed app and CLI, Gatekeeper, Frontend ready and retained layout across two launches. |
| Windows package | Signed installer, installed startup and credential probe | Not run | Valid Authenticode and updater signatures; installed Frontend ready. |
| Linux package | Ubuntu 22.04 build and signed final-artifact audit | Not run | Retain the glibc floor and reject conflicting bundled host libraries. |
| Linux startup | Actual signed desktop on 22.04, 24.04 and 26.04 | Not run | Frontend ready and continued execution under X11 and mixed display variables. |
| Linux media | Camera plugins, synthetic frames, fonts and update relaunch | Not run | Existing final-package regression checks must pass. |
| Remote jobs | Independent execution, reconnect, ownership and Start replay guards | Not run | Exact-source API and executable host regressions; simulated controllers only. |
| Localization | Catalog parity and enabled companion-site documentation | Not run | App locale checks and site build/link/locale validation. |
| Updates | Corresponding source, public URLs and upgrade from 0.2.35 | Not run | Matching signatures and successful isolated update/startup before stable publication. |
| Companion site | 0.2.36 release notes, downloads and all enabled locales | Not run | Candidate and public version/URL checks after app publication. |
| Native KDE Wayland | Actual KWin/Wayland session | Not run | No native session available; Xvfb fallback tests do not establish this coverage. |
| Physical hardware | Laser motion/output and physical camera capture | Not run | Hardware unavailable; these fixes do not establish that the reporting machines have reconnected. |

Final hashes, workflow IDs and completed test results belong in the release's
verification artifact. This source-freeze record is not a claim that the final
packages passed. Keep previous packages and tags immutable.
