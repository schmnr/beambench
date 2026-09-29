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
- Final source PR CI and merged-source CI passed all four jobs: desktop Rust
  build/tests/Clippy, Windows source regression, frontend build/tests/lint/
  performance budgets, and macOS serial regression. The release tag pins
  `46bce7b428ddb283bb183df105d2738468b76c5e`.
  Runs: [PR](https://github.com/schmnr/beambench/actions/runs/36598134553),
  [merged source](https://github.com/schmnr/beambench/actions/runs/36599985343).
- Website: 30 feedback tests, lint, production build, all 23 locale catalogs,
  and 28,934 documentation links pass. All 23 release-note locales build. Site commit `630b88a` is deployed;
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
| Signed macOS package | Pass | Universal Intel/Apple Silicon app, CLI and DMG signed and notarized. Gatekeeper, stapling, DMG integrity and production updater signature verified. |
| Windows package | Pass | Required NSIS installer and CLI built; Authenticode validation and independent updater signature checks passed. |
| Linux package | Pass | AppImage runtime-library audit passed after hardening; CLI version, checksums and updater signature verified. |
| macOS update from 0.2.24 | Pass | Isolated app copy downloaded the public update and restarted at 0.2.25. Installed code signature and executable hash match the verified release; the user-installed 0.2.24 app is unchanged. |
| Native Windows/Linux install and update UI | Not run | Those desktop environments were unavailable. CI package/signature checks passed. |

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

## Final signed macOS package

The signed and notarized package was rebuilt locally with
`scripts/release-macos-local` from the exact tagged source. The GitHub macOS job
passed its source checks but was cancelled after a packaging retry selected an
unfinished `rw.*.dmg` file. [PR #66](https://github.com/schmnr/beambench/pull/66)
fixes artifact selection and passed all four CI jobs. It changes only the build
workflow; the tagged application source and packages remain unchanged.

The final signed package was extracted and launched with isolated configuration
and data. No installed user application or physical controller was changed.

- Fresh `mixed-classic.pdf` import: 10 objects, including images, vector paths
  and clipping masks, rendered successfully.
- GRBL TCP, Frame and Frame Continuously: passed against a loopback simulator.
  At least 27 continuous passes were observed before Stop returned to idle.
- Immediate E-stop during fresh raster preparation: passed with a 1600 x 1600
  synthetic raster sized to 351 mm. Preview represented 3,278,361 burn runs over
  3,510 scanlines. After the reset at Unix timestamp `1790707584.131836`, the only
  command received was `$$`; no motion or laser-on command followed.
- Mac updater signature: verified with the production public key using the same
  `minisign-verify` version as Tauri, independent of the signing script.

## Published application artifacts

The GitHub release and four-target stable updater were published on 2026-09-29.
All 15 package, signature and source artifacts match their GitHub SHA-256
digests. The archive checksum files match, and all three distinct updater
signatures verify against the production key. The versioned download URLs were
checked before the stable manifest was replaced atomically. Prior installers
and the previous manifest were retained.

- [Release and corresponding source](https://github.com/schmnr/beambench/releases/tag/v0.2.25)
- [Windows workflow](https://github.com/schmnr/beambench/actions/runs/36612555395): passed.
- [Linux workflow](https://github.com/schmnr/beambench/actions/runs/36618354885): passed.
- [Stable updater manifest](https://updates.beambench.com/stable/latest.json)
- [Package checksums](https://updates.beambench.com/stable/0.2.25/SHA256SUMS)

The actual updater test used an isolated, signed 0.2.24 macOS app copy. Its
startup notification found 0.2.25, installation completed, and the app restarted
successfully. The visible version, installed code signature and executable
checksum all match the release. The original installed app remains 0.2.24.
