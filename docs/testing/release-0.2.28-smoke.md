# Release 0.2.28 smoke record

- Tag: v0.2.28, resolving the commit containing this record.
- Date: 2026-10-11.
- Tester: Codex. User approved the Layers design and authorized release.
- Source: the reviewed 0.2.27 candidate with corrected profile recovery.

The detailed [0.2.27 smoke record](release-0.2.27-smoke.md) covers the shared
source and native interface. Its 0.2.27 packages and source archive passed
validation but were not published. Those results do not substitute for final
0.2.28 signing and package verification.

## Correction

Missing-profile notifications now open the Profiles tab through MainToolbar,
which mounts on macOS, Windows and Linux. The listener was previously in the
web-only MenuBar. A regression renders the toolbar and notification without
MenuBar and verifies that the action opens the profile setup dialog.

## Release gates

- Source CI must pass all four jobs before tagging. Critical backend and
  frontend tests also run in the exact-tag platform packaging workflows.
- Final signed macOS candidate must show version 0.2.28 and the approved
  controls. Verify the profile recovery action in the native app.
- Source archive must match the exact tag. Check notarization, signing,
  checksums, updater signatures, public URLs and the isolated upgrade before
  publishing the stable manifest and companion site.
- Final package and upgrade results belong in the release verification record
  published beside the artifacts and in the site publication record.

## Checks not run

Physical laser operation, physical preset compatibility, native Windows and
Linux installation, native Intel macOS execution and camera hardware checks
are not run because the necessary hardware/platforms are unavailable. The new
presets remain documentation-based. Existing encrypted-PDF and raster
complexity limits remain explicit.

Decision: publish only after the gates above pass. The public release remains
0.2.26 until the complete corrected release is verified.
