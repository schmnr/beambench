# Repository workflow

This project is maintained by its owner. Finish completed work with a clean
checkout on `main`, committed and pushed to `origin/main`.

- Temporary branches and pull requests are useful while work is underway.
  Merge validated work and remove its branches before handing completed work
  back to the owner.
- Respect required checks and repository rules. Do not bypass a failing check
  or merge incomplete changes just to remove a branch. Explain any blocker.
- Review untracked files as well as tracked changes. Preserve unrelated work;
  do not discard it or publish private planning material without authorization.
- Remove obsolete worktrees after preserving any needed files and verifying
  their work is on `main`.
- Merging source changes does not authorize publishing a release. Follow the
  owner's release instructions and `docs/releasing.md` when a release is requested.

## Linux packaging regressions

- Read `docs/testing/linux-packaging-incidents.md` before changing Linux
  packaging or publishing a release. Kubuntu startup has broken repeatedly when
  build-host libraries were bundled alongside newer host libraries.
- Keep the Ubuntu 22.04 builder and validate the final signed, unmodified
  AppImage on Ubuntu 22.04, 24.04 and 26.04. Require the actual desktop's
  `Frontend ready` callback. A library audit, dependency resolution or substituted
  media probe alone does not prove that the app starts.
- Retain camera, font and update-relaunch tests when fixing startup. Report
  unavailable native Plasma/Wayland and physical hardware checks honestly.
