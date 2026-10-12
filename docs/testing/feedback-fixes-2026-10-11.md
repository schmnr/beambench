# Next-release fixes from October 11 feedback

Changes are prepared in the working tree for the next release. All 16 reports
from September 11 through October 11 were investigated. Work uses submitted
evidence and public documentation; no reporter correspondence is required.
No release has been published and no feedback issue status has been changed.

## Implemented changes

- AppImage font policy excludes host color fonts that trigger the shipped
  WebKit renderer's COLRv1 assertion. Normal system text fonts remain available;
  emoji use monochrome fallbacks or missing-glyph placeholders. The policy is
  limited to the app. An explicit `FONTCONFIG_FILE` is preserved.
- Launcher respects an explicit `GDK_BACKEND`, selects native Wayland when
  X11 is absent, and tries Wayland after X11 when both displays are advertised.
  The existing removal of incompatible bundled host libraries remains in place.
- Package repair audits the installed font/display policy and is idempotent.
  Modified artifacts lose their old updater signature before release signing;
  an already-repaired artifact keeps its current signature. CI and Linux release
  builds run actual SquashFS repair and Fontconfig regression checks.
- The preceding Linux startup log survives restart. Feedback includes bounded
  head/tail excerpts from both launches through the existing scrubber and report
  preview. Display availability and font-policy activation are recorded. A
  second app instance cannot rotate the running instance's log.
- Failed backend commands retain their action names without command arguments.
  Matching error notifications carry that context into reports. Missing-profile
  errors offer a direct **Machine Profiles** recovery action.
- Added [Creality Falcon2 Pro 40W](../machines/creality-falcon2-pro-40w.md) and
  [Two Trees TT-5.5S](../machines/twotrees-tt-5-5s.md) USB presets, with localized
  documentation-based guidance. TT-5.5S uses documented M3 constant power.
  Exact model suggestions reject ambiguous and similarly named variants.
- Applying a GRBL preset to the active, connected machine retains its known
  `$30` power scale in both preview and application. Inactive profiles keep
  their model defaults. Invalid or missing controller limits are ignored.
- Pre-existing Unreleased TCP changes are retained and validated: CR, LF,
  fragmented CRLF, and distinct silent/unrecognized-response failure messages.

## Disposition of every report

| Reports | Next-release disposition | Remaining uncertainty or limit |
| --- | --- | --- |
| [54](https://github.com/schmnr/beambench-feedback/issues/54) | Confirmed renderer assertion avoided by the app font policy; before/after reproduction preserved | Original JPG interaction and Fedora desktop not reproduced; the assertion is reproduced independently |
| [51](https://github.com/schmnr/beambench-feedback/issues/51) | Remove forced X11; native Wayland and invalid-X11 fallback start successfully; preserve startup evidence | Original Mint display/session configuration is unknown; a process with no usable display still cannot start GTK |
| [50](https://github.com/schmnr/beambench-feedback/issues/50) | Add profile-setup recovery and failed-action context | Original triggering action was not captured; valid missing-profile preconditions remain |
| [49](https://github.com/schmnr/beambench-feedback/issues/49) | Keep the 0.2.26 localized missing-file error and stale Recent Projects cleanup; regression passes | A deleted/moved project cannot be recovered from its old path |
| [48](https://github.com/schmnr/beambench-feedback/issues/48) | Keep 0.2.26 delayed TCP status retries and traffic capture; include pending line-ending fixes | Exact controller/bridge unknown and old report has no received bytes; refused ports are distinct from a failed protocol handshake |
| [47](https://github.com/schmnr/beambench-feedback/issues/47) | Retain transport-specific access-denied classification and guidance | OS denies COM3 before protocol probing; software cannot claim another application's exclusively opened port |
| [46](https://github.com/schmnr/beambench-feedback/issues/46) | Keep current status-first serial handshake and selected-classic-GRBL reset fallback | Report is from 0.1.4; device-specific operation cannot be established from that old log |
| [45](https://github.com/schmnr/beambench-feedback/issues/45), [43](https://github.com/schmnr/beambench-feedback/issues/43) | Keep 0.2.25 compressed PDF object-stream support; equivalent-encoding regressions pass | Actual files absent; encryption remains explicitly unsupported. The encrypted loader bypasses bounded filtering and cannot simply replace the guarded import path. |
| [44](https://github.com/schmnr/beambench-feedback/issues/44) | Keep 0.2.25 PDF Form XObject and supported image handling; regressions pass | Original PDF absent; other unsupported effects may still be rejected |
| [42](https://github.com/schmnr/beambench-feedback/issues/42) | Requested Sculpfun S30 Pro 10W preset already shipped in 0.2.25; real-output regression passes | Physical testing pending |
| [41](https://github.com/schmnr/beambench-feedback/issues/41) | Keep raster memory protections and current DPI/size guidance; exact boundary regression passes | Four million burn runs per image and eight million per project remain intentional limits |
| [40](https://github.com/schmnr/beambench-feedback/issues/40) | Keep released opaque PDF graphics-state fix and regression | Already closed as fixed |
| [39](https://github.com/schmnr/beambench-feedback/issues/39) | Keep released GRBL-compatible homing capability and classification fix; regression passes | Already closed as fixed |
| [52](https://github.com/schmnr/beambench-feedback/issues/52) | Add exact Creality Falcon2 Pro 40W USB preset | Documentation-based; no physical test or accessory integration established |
| [53](https://github.com/schmnr/beambench-feedback/issues/53) | Add exact Two Trees TT-5.5S USB preset, separate from TTS-55 | Documentation-based; no physical test established |

## Validation

All recorded checks pass. [Evidence and reproduction instructions](feedback-fixes-2026-10-11/README.md)
and [validation.json](feedback-fixes-2026-10-11/validation.json) preserve commands,
counts, hashes and limitations.

- Full frontend suite: 210 files, 2,770 tests. The subsequently added profile
  recovery tests pass with the invoke/store tests: 16 tests across three files.
- Full core, planner, GRBL, serial and service suites: 2,542 passes, three
  pre-existing ignored tests. Final profile regressions: 27 passes, including
  the later controller-scale fix. Counts overlap between filtered/full runs.
- Desktop Rust suite: 49 passes. TypeScript, targeted ESLint, Rust formatting,
  documentation links and diff whitespace pass. Service/desktop Clippy passes
  with existing warnings; no diagnostics point to the added implementation.
  The production frontend build passes with its existing large-chunk warning.
- Linux runtime tests: four passing tests, including six display cases,
  explicit overrides, actual Fontconfig queries, and actual SquashFS repair,
  audit, idempotence and signature handling.
- Released WebKit 2.50.4 with a host COLRv1 font: matching assertion and renderer
  termination before policy; the same probe survives ten seconds after policy.
- Repaired release payload: frontend ready on X11, native Wayland without X11,
  and Wayland with an invalid advertised X11 display. Headless graphics/input
  warnings occur but no GTK initialization failure or font-policy load failure.
- Linux-only opener helper compiled and ran with a fixture desktop opener,
  verifying that child processes lose the app font environment and retain their
  display environment. It opened no external application or network target.

Linux checks use Ubuntu 24.04 amd64 under Docker emulation on an ARM64 Mac.
The static AppImage runtime cannot execute in that environment; the repaired
payload is extracted and launched through its own `AppRun`. This validates the
packaging policy and shipped renderer, not every supported distribution or the
new desktop binary on Linux. Full Linux compilation remains a release/CI check.
No physical laser was connected or operated. The catalog labels reflect that.

The next step is the normal release build and distribution verification. Future
submitted reports can identify residual device-specific failures; the original
reporters do not need to be contacted to ship these changes.
