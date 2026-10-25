# Craftgineer Phase 2 package validation

- Candidate version/tag: 0.2.34-rc.1 / v0.2.34-rc.1.
- Source: the immutable tag containing this record.
- Date: October 24, 2026.
- Tester: Codex, with the owner completing account consent when needed.
- Authorization: paid-account testing and signed-package validation. Public
  release, stable manifest changes and a new subscription purchase are excluded.
- Hardware: no physical laser or camera will be operated by these checks.

## Gates

| Check | Status at source freeze | Required evidence |
| --- | --- | --- |
| Required source CI | To run | All four repository checks on the candidate source. |
| macOS signing | To run | Developer ID, notarization, Universal2 and exact candidate version. |
| Windows signing | To run | Authenticode NSIS and CLI, updater signatures and checksums; install the signed NSIS and require two actual Frontend ready launches on a disposable runner. |
| Linux package | To run | Ubuntu 22.04 builder, audit, signature and unchanged AppImage hash. |
| Linux startup | To run | Actual Frontend ready on Ubuntu 22.04, 24.04 and 26.04, with X11 and mixed Plasma/Wayland variables. |
| Linux media and relaunch | To run | Camera plugin, synthetic frame, font and inherited update-environment checks. |
| Paid-account pilot | To run | Actual billing entitlement, consent/cancel, list/search/page/preview, SVG/raster insertion, save and offline reopen. Browser admin preview is insufficient. |
| Document and account boundaries | To run | Account switching, revocation, late download after document replacement, selection and undo preservation. |
| Native credentials | To run | Installed desktop remember/restart/disconnect, denied or locked stores and session-only fallback; GNOME Keyring and KDE Wallet. The Windows backend probe is separate evidence from app credential restoration. |
| Account lifecycle | To run | Free/lapsed/recovered access, scheduled cancellation and deletion on disposable accounts. Never delete the owner's account. |
| Native Plasma/Wayland and physical hardware | Not run | Unavailable coverage must remain explicit. Container coverage does not establish these checks. |

Record final artifact hashes, workflow IDs, completed results and remaining
limitations beside the staged test artifacts. Keep private account and consent
evidence out of Git. A passing source build alone does not complete Phase 2.
