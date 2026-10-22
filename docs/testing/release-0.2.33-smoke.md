# Release 0.2.33 smoke record

- Version/tag: 0.2.33 / v0.2.33, resolving the commit containing this record.
- Date: October 22, 2026.
- Tester: Codex. The owner authorized the fix, release and companion deployment.
- Scope: Linux AppImage host-library compatibility and release startup gates.
- Hardware: no physical laser or camera connected or operated.

## Results

| Area | Result | Evidence or required publication check |
| --- | --- | --- |
| Reproduction | Pass | SHA-verified published 0.2.30 starts on Ubuntu 22.04, 24.04 and 26.04. Published 0.2.31 and 0.2.32 start on the two older bases but fail on 26.04 with LIBSYSTEMD_254 missing. |
| Minimal repair | Pass | Removing only bundled libsystemd restores affected packages. A fully repacked 0.2.32 candidate reaches actual frontend readiness on all three bases with X11 and mixed Plasma/Wayland variables. |
| Probe failure policy | Pass | Automated tests reject an exited process even with ready text, a live process without frontend readiness, and a crash immediately after readiness. |
| Final Linux package | Publication gate | Audit and sign the exact 0.2.33 AppImage, retain media/font/inherited-relaunch tests, then run all six actual-desktop cases against the unchanged signed package. Save its SHA-256 and readiness logs. |
| Release source | Publication gate | All four CI jobs must pass, including Rust build/tests/clippy/fmt, frontend tests/performance/build/lint, macOS serial and Windows source regressions. |
| macOS package | Publication gate | Verify Developer ID signing, notarization, Universal2 architecture and isolated candidate startup. |
| Windows package | Publication gate | Verify Authenticode-signed NSIS and CLI archives with exact version and required license files. |
| Updater and source | Publication gate | Verify all updater signatures and URLs, all archive checksums and corresponding source. Exercise the macOS updater engine on an isolated copy before advancing stable. |
| Companion site | Publication gate | Update release notes and Linux startup/install guidance in all 23 locales. Validate translations, links, lint, build, candidate and production routes before completion. |
| D1 USB and Wi-Fi | Unchanged | Keep Experimental. No new physical compatibility claim. |
| Native KWin/Wayland, Windows/Intel macOS install | Not run | Target desktop sessions unavailable. Linux container checks use software-rendered Xvfb, including mixed Wayland variables. These are not native Wayland tests. |
| Physical cameras, lasers and full manual project/motion matrix | Not run | Hardware unavailable. Source regressions and synthetic media tests do not replace physical validation. |

## Release decision

Keep 0.2.32 stable until the publication gates pass. Record the final source
commit, CI/workflow IDs, signed-package checks, Linux desktop logs, updater
exercise and deployed companion checks in release-verification.json beside the
release assets. See linux-packaging-incidents.md for both Kubuntu incidents and
mandatory future release checks.
