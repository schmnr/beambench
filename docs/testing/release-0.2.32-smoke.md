# Release 0.2.32 smoke record

- Version/tag: 0.2.32 / v0.2.32, resolving the commit containing this record.
- Date: October 21, 2026, America/New_York.
- Tester: Codex. The owner authorized final validation and publication.
- Scope: Linux AppImage update relaunch, machine presets, Experimental D1 native USB and Wi-Fi, and Vite development-server fixes.
- Hardware: no physical laser or camera connected or operated.

## Results

| Area | Result | Evidence or required publication check |
| --- | --- | --- |
| D1 native USB | Pass, software only | 18 emulator tests and six worker/service tests cover identification, module rejection, button start/resume, laser-off frame, bounded acknowledged streaming, stop, disconnect and completion confirmation. Windows initial-DTR and macOS serial regressions pass. |
| D1 native Wi-Fi | Pass, software only | Existing 11 protocol tests and service regressions cover verified upload, ordered status, framing, physical start, stop and completion. |
| D1 status | Pass | Both adapters and all eight presets remain Experimental. Physical validation and accessory modes remain pending; no hardware compatibility claim is added. |
| Preset settings | Pass, software only | Preset catalog and device-settings regressions validate documented workspaces, power scale, transport and explicit controller selection. Hardware testing is pending. |
| Linux relaunch policy | Publication gate | Source tests must pass, then repeat the complete launcher-chain media/font/relaunch regression against the final signed AppImage. |
| Release source | Publication gate | All four CI jobs must pass on the exact release candidate, including the Vite patch, version metadata, full Rust/frontend suites, builds, lint, clippy and formatting. |
| Native macOS | Publication gate | Launch the final signed candidate twice with isolated settings and no machine profiles. Record actual observed behavior and process exit. |
| Localization | Publication gate | Check all 23 controller labels and hints retain Experimental; validate companion documentation translations and native app rendering where available. |
| Updater | Publication gate | Verify four-platform URLs and updater signatures, exercise the production macOS updater engine from 0.2.31 into an isolated candidate, then atomically advance stable. |
| Packages and source | Publication gate | Verify exact-tag signed/notarized macOS, Authenticode Windows, hardened Linux, standalone CLI archives, checksums and corresponding dependency source before publication. |
| Companion site | Publication gate | Publish 0.2.32 downloads, source link, release notes, installation and compatibility guidance only after verified packages are public. Validate local build, candidate and production routes in all 23 locales. |
| Physical D1/D1 Pro and other lasers | Not run | Hardware unavailable. Complete HW-039 before promoting D1 support beyond Experimental. Emulator tests are not physical validation. |
| Physical camera | Not run | Hardware unavailable. Synthetic frames exercise runtime plugin discovery only. |
| Native Windows/Linux/Intel macOS installation | Not run locally | Target machines unavailable. Platform CI, signing and package audits provide recorded software coverage. |
| Full manual project/raster/origin/motion matrix | Not run | No physical machine. Automated source and packaging regressions run; unavailable interactive/manual cases are not treated as passed. |

## Release decision

Keep 0.2.31 stable until every publication gate passes. Record final results,
exact source commit, platform workflow IDs, package/signature checks, updater
exercise and production website checks in `release-verification.json` beside
the release assets. D1 USB and Wi-Fi remain Experimental in this release.
