# 0.2.25 release verification

Date: 2026-09-29. The release owner authorized pushing, deploying, and publishing
after the checks pass. Source version is 0.2.25 throughout Cargo, npm, and Tauri.

## Automated checks

- Frontend: 2,699 tests across 205 files pass after the UI cancellation fix.
  Production build passes. Lint has no errors and one existing exhaustive-deps
  warning in `TransformSection.tsx`.
- Targeted cancellation regressions: 125 tests pass across the machine store,
  Laser panel, Job Control panel, and menu tests. Five new cases cover pending
  preview/preflight, variable text, and a deliberate new Start after E-stop.
- All 22 non-English locales have translated network and Frame speed labels,
  preparation-stop and sleep-protection messages, and raster-complexity guidance.
  Error mapping is exercised in all 23 locales.
- Linux workspace excluding `beambench-tauri` and vendored `pdf417`: 3,509 tests
  pass, zero fail, four ignored. This includes 827 service and 26 serial tests.
  Workspace Clippy passes with existing warnings. Rust formatting passes.
- Full CI on `a33e91f3` passed all four jobs: desktop Rust build/tests/Clippy,
  Windows source regression, frontend build/tests/lint/performance budgets, and
  macOS serial regression. The final UI cancellation commit must pass PR CI too.
- Website: 30 feedback tests, lint, production build, all 23 locale catalogs,
  and 28,888 documentation links pass. Site commit `630b88a` is deployed;
  container health and local/public HTTP 200 responses are verified.
- Third-party notices include the sleep-protection dependencies. The updater's
  changelog extraction is verified with the existing version-heading format.

## Native macOS smoke checks

The development binary built successfully after slow/stalled startup attempts.
It was wrapped in a temporary app bundle for native accessibility automation,
with isolated configuration/data and a synthetic GRBL server listening only on
`127.0.0.1:23025`. No physical laser was connected or operated.

| Check | Result | Evidence |
| --- | --- | --- |
| Native 0.2.25 launch | Pass | Native Tauri window and version label verified. |
| PDF with embedded images | Pass | `mixed-classic.pdf` imports 10 objects; vector outlines, clipped artwork, and grayscale images are visible. |
| GRBL over TCP | Pass, simulated | Explicit GRBL selection connects and reports Ready. |
| Frame | Pass, simulated | One laser-off frame completes with the expected rectangle commands. |
| Frame Continuously | Pass, simulated | Repeated passes observed, then Stop returns to idle without restarting. |
| E-stop immediately after Start | Pass after fix | A small prepared job stops; no subsequent motion is sent. |
| E-stop during preview preparation | Pass after fix | A 1600 x 1600 synthetic raster at 350 mm produces 3,258,529 preview burn runs. Stop during generation returns to idle without starting; only the settings query follows reset, with no motion. |
| Japanese, Korean, Simplified Chinese | Glyph/menu check passed | Native menu labels update and glyphs/frame-speed labels render. Existing English headings remain in Move/Laser panels; this is not a claim of complete localization. |
| Physical Windows/Mint USB or Wi-Fi | Not run | No physical controller or those desktop environments available. Network GRBL remains Experimental. |
| Signed packages and updater install | Pending | Verify final artifacts before publication. |

## Release-blocking issue found and corrected

The native raster test found that E-stop during UI preview generation could be
followed by a later Start request. Rust's generation check begins after that UI
step. The UI now records a stop generation before awaiting the backend stop,
checks it after preview and preflight in all three Start entry points, and checks
it again after variable-text preparation. E-stop also closes pending preflight
confirmation. The native reproduction and new regression tests pass.

The first macOS packaging workflow was cancelled and its unpublished candidate
tag removed. Its source archive is superseded. Build and verify fresh packages
and corresponding source from the corrected, approved PR commit.

## Publication order

1. Merge the corrected release PR after required CI checks pass.
2. Tag that exact source `v0.2.25` and regenerate corresponding-source archives.
3. Run platform packaging workflows sequentially and verify final signatures,
   checksums, native launch, and updater metadata.
4. Upload immutable versioned files, verify their public URLs, then atomically
   replace the stable manifest. The website compatibility fix is already live.
5. Publish the release and website notes/download version. Record unavailable
   hardware explicitly and keep GRBL over Network marked Experimental.
