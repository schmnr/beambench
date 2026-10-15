# Release 0.2.29 smoke record

- Version/tag: 0.2.29 / v0.2.29, resolving the commit containing this record.
- Date: October 14, 2026.
- Tester: Codex. User authorized committing and publishing the #55 fix.
- Scope: preference saving after local API startup failure.
- Hardware: no physical laser connected or operated.

## Results

| Area | Result | Evidence or required publication check |
| --- | --- | --- |
| Settings persistence | Pass | The new regression failed before the fix, then passed with layout/theme/autosave settings surviving reload after missing API authentication or an occupied port |
| API validation | Pass | Actual invalid API edits leave memory, settings file, theme callback and events unchanged; disable and localhost-only recovery work |
| Preference rollback | Pass | Forced save failure reverses the previous/candidate transition |
| Native desktop regressions | Pass | All 50 desktop tests on ARM64 macOS, including listener rollback and worker responsiveness |
| Service settings regressions | Pass | Six preferences and six app-settings tests |
| Formatting | Pass | Workspace format and diff whitespace checks |
| Source CI | Pending | All four exact-source CI jobs must pass before tagging |
| Package launch and updates | Pending | Verify final signed candidate version, isolated launch/preferences, signatures, checksums, URLs and older-version updater smoke before publishing stable |
| Windows/Linux packages | Pending | Exact-tag platform workflows and final artifact verification |
| Corresponding source | Pending | Exact-tag archive, dependency sources, notices and source digest verification |
| Unchanged canvas/project/laser workflows | Not run manually | No geometry, placement, job generation or machine-operation code changed; CI and packaging workflows run existing regression suites |
| Physical laser/camera operation | Not run | Hardware unavailable |
| Native Windows/Linux/Intel macOS installation | Not run locally | Required machines unavailable; CI platform checks and final artifact audits provide the recorded coverage |

## Release decision

The [fix validation](feedback55-fix-2026-10-14.md) records the regression evidence.
Build and distribution checks are publication gates. Record their final results
in `release-verification.json` alongside the release assets. Version 0.2.28
remains stable until all required 0.2.29 artifacts and metadata are verified.
