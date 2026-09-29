# 0.2.25 release candidate verification

Date: 2026-09-29. Source: the commit containing this record on
`codex/raster-memory-budget`. Version is 0.2.25 in Cargo, npm, both lockfiles,
and the Tauri configuration. The release owner authorized pushing, deployment,
and publication after the remaining checks on 2026-09-29. The prepared app and
website branches are pushed. Publication remains conditional on the checks below.

## Completed checks

- Frontend: 2,694 tests across 205 files pass. This includes five release-uploader
  tests, not an additional five tests. The production build passes.
- Localization: all 22 non-English locales contain translated Frame speed and
  Network labels plus E-stop preparation and sleep-protection messages. The
  existing image-complexity error already maps to translated text. Tests exercise
  the backend-message mapping in all 23 locales and reject English fallback.
- Frontend lint: no errors; one existing exhaustive-deps warning in
  `TransformSection.tsx`. Rust formatting and diff whitespace checks pass.
- Linux service and serial libraries: 827 service tests and 26 serial tests
  pass. One native desktop power-manager smoke test is explicitly ignored.
  Tests run unprivileged in Debian with writable isolated configuration/data
  directories. These are software/controller-simulation tests, not physical
  USB or Wi-Fi validation.
- E-stop: added cancellation checks after command generation and before initial
  output. Cancellation is recorded before waiting for the fire/session locks.
  A regression verifies that a prepared frame emits nothing while E-stop is
  waiting for the controller lock.
- Third-party license report regenerated, including the new sleep dependencies.
- macOS release preflight: toolchain, signing prerequisites, and matching
  0.2.25 source versions pass. A clean source commit and matching tag remain
  prerequisites to running the signed build.
- Website: 30 feedback tests pass, including optional BRLTTY fields. Documentation
  links pass for 5,290 pages and 28,888 local links. i18n validation passes for
  all 23 supported/enabled locales. The draft release notes compile as MDX.

## Native and physical checks still required

| Check | Result | Reason |
| --- | --- | --- |
| Native macOS 0.2.25 launch | Not run | Newly built Rust helpers stall in `_dyld_start` before executing. Retrying the build, a direct helper launch, and a single-job build did not clear it. No OS security protections were changed. |
| Real-app E-stop immediately after Start | Not run | Native candidate could not launch; simulated cancellation tests pass. |
| Real-app Frame and Frame Continuously | Not run | Native candidate could not launch; service and UI regressions pass. |
| Real-app PDF containing an image | Not run | Native candidate could not launch; import/renderer fixtures are covered by automated tests. |
| Chinese, Japanese, Korean native menus and glyphs | Not run | Translation content is verified, but native rendering and menu layout could not be inspected. |
| Physical Wi-Fi/USB laser, Windows, Linux Mint desktop | Not run | No attached physical controller or those desktop environments was used. |
| Signed packages, updater upgrade, source archives | Not run | Requires the approved release tag and platform packaging workflows. |

## Publishing order and decision

Source preparation and the automated checks below are complete.
Do not treat this record as release approval or claim the native/hardware checks
passed. Resolve the macOS launch stall or use a CI-built candidate for native
smoke checks before approving publication.

1. Push the prepared app branch and run the full CI matrix, including the Tauri
   desktop package, on the final source commit.
2. Push and deploy the website feedback-schema fix `5059a81` before releasing
   the app; it accepts the new optional BRLTTY diagnostics.
3. Complete the applicable native/controller smoke checks and record results.
4. After approval, tag the approved source `v0.2.25`, build the corresponding-source
   archives and signed platform packages, and verify their signatures/checksums.
5. Publish only after the checks pass. The release owner has supplied go-live
   authorization; no additional approval is needed for the agreed release.
6. Publish the website release notes only after the app artifacts are verified
   live. Drafts and the publishing handoff are committed in the site repository
   on `codex/release-0-2-25`; they are outside the public docs collection.

## Final automated checks

- `cargo test --locked --workspace --exclude beambench-tauri --exclude pdf417`
  passes on Linux: 3,509 tests pass, zero fail, and four are explicitly ignored.
  This total includes the service/serial tests above and two passing doc tests.
  The ignored checks cover desktop power management, a manual planner gate,
  fixture regeneration, and one vendored nesting documentation example.
- Linux workspace Clippy passes with existing warnings, using the same package
  exclusions. The Tauri desktop package still requires the native/CI check;
  the vendored `pdf417` package is excluded from this workspace validation.
- Final Rust formatting, whitespace, and version-consistency checks pass. All
  24 BeamBench Cargo packages, npm manifests/lockfile, and Tauri configuration
  agree on 0.2.25.
- Claude's compatibility-table correction is preserved in commit `19b09320`.
  GRBL over Network remains marked Experimental pending physical validation.

## Authorized release execution

- Full CI was dispatched for app commit `a33e91f3`. The macOS serial regression
  passed. Final CI and packaging results must be checked before publication.
- Site commit `630b88a` is pushed to main and its production build is underway.
- Corrected the 0.2.25 changelog heading to the existing release-tool format and
  verified that the updater extracts only this version's notes.
- Native macOS compilation was retried. A cached helper runs, but subsequent
  compilation still stalls; native smoke checks remain outstanding.
