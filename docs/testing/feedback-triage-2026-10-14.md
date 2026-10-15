# Feedback triage, October 14, 2026

Reviewed all 32 open reports in `schmnr/beambench-feedback`, including comments
and submitted diagnostic bundles. Closed 21 with individual reasons and left
11 open with descriptive titles and explicit next steps. No email was sent.
Application fixes and release publication are outside this triage change.

The [published v0.2.28 release](https://github.com/schmnr/beambench/releases/tag/v0.2.28)
is dated October 12, 2026 UTC. This supersedes the pending-release status in the
October 11 planning documents. Source inspected: `ce46a06a`.

## Closures

| Disposition | Reports | Basis |
| --- | --- | --- |
| Addressed in released code | #54, #51 | AppImage font policy and display-backend fixes in 0.2.28; original desktop workflows were not physically retested |
| Preset request fulfilled | #53, #52, #42 | Exact presets shipped in 0.2.28 or 0.2.25; physical compatibility remains untested |
| Addressed capability/error handling | #49, #44, #29, #21 | Missing-file handling, PDF Do support, macOS serial initialization/classification, homed framing work-offset conversion |
| Consolidated into #43 | #45 | Likely repeat PDF rejection from the same session; identical files are not established |
| Expected prerequisite, limit, or workflow | #50, #47, #41, #7, #2 | No active profile, denied port, raster complexity, incompatible USB driver, layer workflow |
| Workspace protection/error wording | #24, #10, #12, #14, #18, #37 | Refusal of reported out-of-bed placement; #24 independently matches physical geometry, #37's old wording was fixed in 0.1.6 |

These closures are scoped to the evidence, not claims that every original
hardware/file interaction was retested. Older bounds reports lack complete
placement diagnostics. #18 has a protected project attachment that was not
retrieved. Their comments explicitly allow reopening if current diagnostics
demonstrate safe physical placement being rejected.

For #21, the recorded work-to-machine offset is (69.293, 90.354) mm. Converting
the user-origin anchor (80.707, 49.646) gives a physical center of (150, 140) mm.
The 117.62 mm square fits the bed. This supports the released 0.2.9 framing fix.
For #24 the physical center is (30, 30) mm and half the artwork height is
67.044 mm, giving the reported 37.044 mm lower-edge overrun. That is expected.

The [action record](feedback-triage-2026-10-14/issue-actions.json) preserves every
rationale and the actual GitHub comment URLs.

## Remaining work

| Report | Current conclusion | Next step |
| --- | --- | --- |
| #55 | Confirmed code path blocks unrelated preference saves after invalid API startup | Separate unrelated persistence from failed API synchronization while preserving authentication and explicit API-edit validation |
| #30 | AlgoLaser TCP handshake succeeds, then a job receives GRBL error 20 | Establish exact firmware command support; compare power/air/motion output; do not blame the handshake or M7 alone |
| #48 | Exact network bridge compatibility unverified after retry/line-ending fixes | Current endpoint/controller identity and TCP RX/TX retest |
| #46, #27 | Old intermittent/silent serial discovery cases | Current controller identity, port/baud and handshake evidence |
| #20, #11 | Stop delivery not established after USB/access failures | Controlled no-output current-device stop/recovery verification; release notes alone are insufficient |
| #23 | Cutting failure accompanied by generic alarms | Current alarm number, terminal-job evidence, exact controller |
| #43 | Old error conflates supported object streams and unsupported encryption | Inspect original source PDF before deciding fixed versus expected behavior |
| #38 | Unknown object rejected by vector-only command | Project, selected object type and originating command |
| #1 | File-read access failure with no reproduction | Current import method and source-file availability; distinguish OS permission from stale-handle defect |

Only #55 currently has a reproducible application failure mechanism established
by this investigation. #30 has concrete command-rejection evidence needing a
compatibility investigation. The other reports are unresolved evidence/retest
work, not nine additional confirmed code defects.

## New report #55

Subsequent work prepared an [unreleased fix and permanent regressions](feedback55-fix-2026-10-14.md).
The investigation below records the original 0.2.28 behavior. Its temporary
probe targets the original `ce46a06a` API and should be run on that source;
the permanent desktop regression verifies the corrected behavior.

The Windows 0.2.26 report warns that panel layout could not be saved. Its startup
log also records invalid network API authentication. The same path is present
in the 0.2.28 source:

1. `appService.persistLayout` submits `update_app_settings` after its debounce.
2. `ops::app::update_app_settings` applies settings side effects before replacing
   the in-memory settings and before persisting them.
3. The desktop settings applier calls `ApiRuntime::sync_from_settings` on every
   update, then applies native appearance only if that succeeds.
4. A saved enabled, non-localhost API configuration with no valid environment
   token is rejected by `ApiServer::validate_config`, including when the update
   changes only a layout or another unrelated preference.
5. The error returns before the new layout is committed. The frontend then
   displays the reported generic layout-save failure.

The bundle omits the backend exception for the actual layout command and the
saved API fields. This is a matching reproducible explanation, not proof that
every possible filesystem or serialization failure on the reporter's Windows
installation has been ruled out. Native appearance and other preferences can
also be blocked by this callback.

Workaround: disable the local API or set it to localhost-only, then save the
layout again. Authentication must remain enforced for a network listener.

API enabled/interface preferences persist, but `ApiConfig::from_settings`
reads the token only from the process environment. A valid network setup can
therefore become invalid on a later launch without that token, for example
after switching from a configured terminal to a desktop launcher. This is a
plausible trigger, not an established fact about the reporter's launch method.
Fresh settings default to API disabled and localhost-only.

The [isolated probe](feedback-triage-2026-10-14/report55-repro.rs) uses the actual
desktop API runtime and service update operation. It sets an empty token in the
test process, constructs invalid saved network settings, observes startup and
layout-save rejection, verifies the old layout remains and no listener starts,
then verifies disabling the API allows the layout update. Test persistence uses
the service's automatically isolated test-process directory.

Temporarily append this to `tauri-app/src-tauri/src/state.rs`, run the command,
then remove the include. The probe confirms existing broken behavior; it is not
a regression test for a shipped fix.

```rust
#[cfg(test)]
include!("../../../docs/testing/feedback-triage-2026-10-14/report55-repro.rs");
```

```sh
BEAMBENCH_API_TOKEN='' cargo test -p beambench-tauri feedback55_investigation -- --nocapture
cargo test -p beambench-service auto_detection_preserves_transport_errors_without_protocol_retries -- --nocapture
```

The probe passed: one test, zero failures. The [runtime result](feedback-triage-2026-10-14/report55-result.txt)
records the command and output. The temporary source include was removed after
execution. No application source changes remain.
The existing [serial transport-classification regression](feedback-triage-2026-10-14/serial-classification-result.txt)
also passed, supporting the #29 closure. It covers macOS `Invalid argument`,
access-denied and disconnected-device failures without misclassifying them as
unsupported protocols.

Future fix checks should cover layout persistence across restart, unrelated
theme/preferences updates, disable/localhost recovery, and atomic rejection of
explicit invalid API changes without exposing a network listener. No original
Windows install, physical laser or original PDF was exercised here.
