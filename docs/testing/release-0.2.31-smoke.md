# Release 0.2.31 smoke record

- Version/tag: 0.2.31 / v0.2.31, resolving the commit containing this record.
- Date: October 19, 2026.
- Tester: Codex. The owner authorized fixing feedback #57 and publication.
- Scope: Linux AppImage GStreamer plugin discovery and packaging regression gates.
- Hardware: no physical laser or USB camera connected or operated.

## Results

| Area | Result | Evidence or required publication check |
| --- | --- | --- |
| Feedback #57 reproduction | Pass | Published 0.2.30 ELF launcher hides installed appsink, v4l2src and videoconvert on Ubuntu 24.04; restoring host discovery loads all three and captures synthetic frames |
| Launcher correction | Pass | Repaired AppImage runs the complete packaged launcher chain against host plugins and a three-frame synthetic capture on Ubuntu 24.04 |
| User configuration | Pass | Unset, explicitly empty and custom versioned/unversioned system plugin paths retain their semantics; additional plugin paths and arguments with spaces or empty values survive |
| Packaging policy | Pass | Repair removes incompatible libraries, retains the desktop binary, installs both wrappers, invalidates stale signatures and is idempotent; final audit checks wrapper contents |
| Linux display and fonts | Pass | Ubuntu 24.04 runtime suite covers X11, Wayland, explicit overrides, normal fonts and color-font exclusions |
| Frontend | Pass | Full suite, production build and lint; existing TransformSection hook warning remains |
| Release source CI | Pending | All four required jobs must pass before tagging |
| Final platform packages | Pending | Exact-tag signed macOS, Windows and Linux artifacts, CLI archives and corresponding source must pass verification before publication |
| Final Linux camera gate | Pending | Repeat the packaged-launcher media regression against the final signed 0.2.31 AppImage |
| Updater | Pending | Validate four-platform URLs and signatures; exercise the older-version macOS updater against the candidate before replacing stable |
| Companion docs | Pending | Update and validate installation, camera troubleshooting and release notes in all 23 supported languages; verify candidate and production routes |
| Physical laser or camera capture | Not run | Hardware unavailable. Synthetic capture validates plugin discovery, not physical device compatibility or permissions |
| Native Windows/Linux/Intel macOS installation | Not run locally | Machines unavailable; platform CI and artifact audits provide recorded coverage |
| Unchanged project/raster/machine lifecycle manual matrix | Not run | No application behavior or machine command changes; existing automated regressions run in source CI and packaging |

## Release decision

Source CI, final package verification, updater verification and website checks
are publication gates. Record their final results in `release-verification.json`
beside the release assets. Keep 0.2.30 stable until these gates pass. Close
feedback #57 only after 0.2.31 is published and verified.
