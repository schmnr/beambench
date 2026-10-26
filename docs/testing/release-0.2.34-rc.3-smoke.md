# Craftgineer and PrintCutCarve package validation

- Candidate version/tag: 0.2.34-rc.3 / v0.2.34-rc.3.
- Source: the immutable tag containing this record.
- Date: October 26, 2026.
- Authorization: integration work, approved reviewed wording, signed-package validation,
  and three approved disposable PrintCutCarve accounts. Public desktop release
  and stable manifest changes remain excluded.
- Accounts: all free and paid accounts may connect to their available library.
  Only a paid subscription verified through either connected provider hides the
  startup ad. An optional protected paid-end cache expires at the verified date.
- Hardware: no physical laser or camera will be operated by these checks.

| Check | Status at source freeze | Required evidence |
| --- | --- | --- |
| Required source CI | To run on this candidate | All four repository checks. The prior integration source passed all four. |
| macOS signing | To run | Developer ID, notarization, Universal2 and exact candidate version. |
| Windows signing | To run | Authenticode NSIS and CLI, updater signatures and checksums; actual installed Frontend ready launches. |
| Linux package | To run | Ubuntu 22.04 builder, audit, signature and unchanged AppImage hash. |
| Linux startup | To run | Actual Frontend ready on Ubuntu 22.04, 24.04 and 26.04, with X11 and mixed Plasma/Wayland variables. |
| Linux media and relaunch | To run | Camera plugin, synthetic frames, fonts and inherited update-environment checks. |
| Account pilot | To run | Real hosted consent, member, pack-buyer and free accounts, own library listing and foreign account rejection. No email, Stripe calls or personal account changes. |
| Startup ad | To run | Paid remembered account suppresses the ad; pack-buyer and free accounts retain it. Offline suppression cannot outlive the verified paid period. |
| Project imports | To run | Original SVG insertion, revision, undo, save and offline reopen through the existing project pipeline. |
| Native credentials | To run | Remember/restart/switch/disconnect and session-only fallback on Windows and Linux; locked/denied GNOME Keyring and KDE Wallet. |
| Native Plasma/Wayland and physical hardware | Not run | Xvfb and mixed environment-variable coverage do not establish native KWin/Wayland or physical camera/laser behavior. |

The Library tabs and Preferences account controls are included in this candidate.
The earlier rc.2 artifacts do not contain these changes.
The live catalog has no full free packs at source freeze. The free-account
pilot must connect successfully, display an empty library and retain the ad.
Do not change catalog products to manufacture a populated free account.

Keep final hashes, workflow IDs and results beside staged artifacts. Credentials,
sessions, authorization URLs and private fixture evidence stay outside Git.
Package validation does not authorize publication or an updater change.
