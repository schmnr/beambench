# Beam Bench 0.2.34 release validation

- Version / tag: 0.2.34 / v0.2.34.
- Source: the immutable release tag containing this record.
- Date: October 26, 2026.
- Authorization: owner requested the next release, documentation and companion
  site updates, followed by the remaining Craftgineer and PrintCutCarve tests.
- Baseline: signed 0.2.34-rc.3 completed integration and platform validation.
  Final binaries must be validated independently after the PDF and ACMER changes.
- Hardware: no physical laser or camera available to these automated checks.

| Area | Check | Result at source freeze | Evidence required |
| --- | --- | --- | --- |
| Source | Required CI, planner, machine-service and frontend tests | Not run | Exact release-source checks must pass before publication. |
| macOS | Universal2, Developer ID, notarization, actual startup | Not run | Final signed app and CLI, Gatekeeper and Frontend ready. |
| Windows | Signed installer, installed startup and credential probe | Not run | Valid Authenticode and updater signatures; installed Frontend ready. |
| Linux | Ubuntu 22.04 build, audit and unchanged signed artifact | Not run | AppImage audit, signatures and checksums. |
| Linux startup | Actual desktop on 22.04, 24.04 and 26.04 | Not run | Frontend ready and continued execution for X11 and mixed environments. |
| Linux media | Camera plugins, synthetic frames, fonts and update relaunch | Not run | Existing final-package regression checks. |
| Libraries | Connect, refresh, insert, undo and save/reopen | Not run | Final signed app with website-owned files, including a saved PCC sample. |
| Accounts | Remember, restart, disconnect and paid/free ad behavior | Not run | Final-package smoke, with rc.3's broader matrix retained as baseline evidence. |
| Changes | Vector PDF spacing setup and ACMER preset availability | Not run | Regression tests and final app inspection. |
| Updates | Source archives, public URLs and update from an older build | Not run | Exact-version source, matching signatures and successful update/relaunch. |
| Native KDE Wayland | Real KWin/Wayland session | Not run | Session unavailable; mixed variables under Xvfb do not establish coverage. |
| Physical hardware | Laser output and physical camera | Not run | Hardware unavailable; simulated tests do not establish physical compatibility. |

Record completed checks, artifact hashes, workflow IDs and unavailable coverage
in the final release evidence. Do not interpret this source-freeze plan as a
completed result. Preserve the immutable package and tag after signing.
