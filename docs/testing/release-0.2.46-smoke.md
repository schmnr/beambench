# Beam Bench 0.2.46 release validation

- Version and tag are 0.2.46 and `v0.2.46`.
- The immutable tag identifies the release source.
- Source preparation began November 26, 2026.
- Physical machines and cameras were unavailable at source preparation.

| Area | Result at source preparation | Evidence and final checks |
| --- | --- | --- |
| Independent laser windows | Pass | macOS development build ran jobs simultaneously on three emulated USB GRBL controllers. It rejected a duplicate port claim. Cancelling and closing the middle window left the other two jobs streaming. Repeat package startup and keep physical USB controller testing pending. |
| Initial GRBL connection | Pass | Regression tests cover a delayed first write, a successful write with no reply, a persistent write failure and restoring the serial port’s normal timeout. Confirming the fix on the Ortur in feedback #97 still requires the owner’s device. |
| Machine presets and output | Pass | Automated tests cover preview and apply, workspace placement, saved settings, vector and raster output, air assist where supported, and retention of the connected controller’s power scale. New presets await physical-machine tests. |
| Project, canvas and Outliner | Pass | The frontend suite passed 3,174 tests, including incompatible-drop guidance and continued use of valid destinations. Manual project, canvas and artwork operations were not run. |
| Localization | Pass | The frontend tests cover all 23 locale catalogs, placeholders and revised dialogs. Native menu and rendering checks are tracked separately before tagging. |
| Exact release source | Not run | Require passing CI and critical suites on the release source. |
| macOS | Not run | Verify the signed, notarized Universal2 package, CLI, updater archive and isolated launches on Apple Silicon and under Rosetta. |
| Windows | Not run | Verify the final signed NSIS installer, updater signature, runtime imports and two installed launches reaching Frontend ready. |
| Linux | Not run | Start the unmodified signed AppImage on Ubuntu 22.04, 24.04 and 26.04. Keep the launcher and library audit, camera, font and inherited update-relaunch checks. Native Plasma/Wayland and physical-camera checks are pending. |
| Updates and source | Not run | Verify signatures, checksums, corresponding source and hosted URLs before advancing the stable updater. Exercise an isolated update from 0.2.45. |

Publish after the final artifact checks pass. Retain artifact hashes and the evidence from the platform and updater checks with the release.
