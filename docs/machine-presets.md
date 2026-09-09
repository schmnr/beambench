# Expanding the machine preset catalog

Beam Bench can add presets without the maintainer owning every laser. Validate
controller behavior through shared protocol tests, check model defaults against
published specifications, and use owner reports to establish physical-machine
coverage. A preset supplies settings; it does not establish that every feature
or hardware revision has been tested.

## Evidence levels

| Level | Evidence needed | Can ship? |
| --- | --- | --- |
| Documentation-based | Exact model and scope, sources for control settings, explicit choices for missing or conflicting specifications, and passing relevant software tests | Yes, using an existing controller adapter and stating that physical testing is pending |
| Community-tested | The above plus a reviewed owner report with Beam Bench version, preset version, firmware, modifications, and results for the checks below | Yes, with coverage limited to the reported configuration and operations |
| Needs review | Important control settings remain unexplained, or a reproducible failure invalidates the current defaults | Keep new entries out of the catalog; investigate, correct, or withdraw affected existing entries |

Maintainer ownership is not an admission requirement. A reviewed community
report counts as physical-device evidence. Do not use an unqualified
"certified", "fully compatible", or "all features tested" claim. Multiple
independent reports increase confidence without changing the tested scope.

The controller adapter's status in [controller compatibility](controller-compatibility.md)
is separate. A community-tested preset does not automatically remove an
adapter's Experimental status. A new protocol still needs adapter development
and protocol tests before a preset can make it usable.

## Adding a preset

1. In **Machine Profiles** or **Device Settings**, use **Request a machine
   preset** beside the preset picker. No account is required. Requests can
   start with a model name and manual link; owning the machine is optional.
   [Beam Bench Support](https://beambench.com/support) also offers email links.
   Contributors who already use GitHub can use the **Machine preset request
   or test report** issue form.
   Prioritize requested machines with an existing controller adapter and clear
   specifications. Group work by controller family to reuse test coverage.
2. Create a short evidence record in `docs/machines/`. Record exact model,
   firmware scope, source URLs and access date, setting rationale, unknowns,
   and test status. The [S9 record](machines/sculpfun-s9.md) is an example.
   Separate stock hardware from extension kits, changed boards, added limit
   switches, automatic air pumps, and rotary attachments.
3. Verify transport, baud, workspace, origin, power scale, power mode, homing,
   air commands, and job headers/footers. Manufacturer manuals, firmware source,
   and manufacturer-published profiles are useful evidence. Extract factual
   settings and cite them; check licensing before copying files or code.
   Marketing wattage and material cutting speeds do not establish the
   controller's power scale or motion limits.
4. Add the entry to `profile_presets()` in
   `crates/beambench-service/src/ops/profiles.rs`, with a stable ID, version,
   and a comment linking its evidence record. The app, CLI, and API already
   read this list. Use the description for the evidence level and the existing
   advisory for setup decisions a user needs to make. Localize new copy in all
   supported locales. Do not infer an exact model from a generic GRBL banner
   or a USB chip shared by many machines.
5. Add tests for the settings that affect output. Exercise profile application,
   workspace placement, power mapping, and optional commands through the real
   planner/emitter. Reuse existing controller and streaming tests. Add virtual
   controller fixtures when a model introduces protocol behavior. Run the
   catalog checks, relevant backend/frontend tests, formatting, and lint checks.
6. Ship a documentation-based preset once that review passes. Invite owner
   validation through **Share machine test results** in the app. Update its evidence record and
   description after reviewing a completed report. A connection-only report
   remains partial evidence, not proof that jobs complete correctly.

Unknown optional features stay disabled. Never guess air-assist commands from
a related model or enable homing because a controller supports the command.
When published travel dimensions conflict, document the conflict and choose a
defensible smaller starting workspace if the axes are known. Uncertainty about
power mapping or required control commands needs resolution before inclusion.

Presets ship with the app through normal reviewed releases. Existing profiles
are not silently rewritten. Increment the preset version when defaults change;
users can preview the diff and explicitly reapply it. Evidence-only updates can
keep the settings version. Explain urgent corrections in release notes.

## Owner validation

Save and activate the profile used for testing, then choose **Share machine
test results** beside the preset picker in **Machine Profiles** or **Device
Settings**. Confirm the exact model and describe the results below. No GitHub
or Beam Bench account is needed. A reply email is optional. Users can inspect
**What gets sent**, submit for a report ID, or save a report file if sending
fails. [Beam Bench Support](https://beambench.com/support) offers email links
for users who cannot open the app or have an older version without these buttons.

Test reports include app/system details, saved active-profile settings and
available firmware details. They exclude project data, device identifiers,
logs and the previous job. Preset requests include app/system details and the
user's text, without unrelated active-machine diagnostics. Reports remain in
the existing private feedback intake. A report requires review before it can
change a preset's evidence level.

If more settings are needed, owners can export a `.bbprofile` file separately.
Export omits local connection and camera details, but names, notes and custom
G-code remain; review those before sharing it by email or in an optional GitHub
issue. The in-app catalog form does not attach projects or profile files.

Record Pass, Fail, Not run, or Not applicable for each check. Follow normal
machine precautions and do not enable accessories or capabilities just to
complete this list.

| Check | Useful evidence |
| --- | --- |
| Connect and reconnect | OS, connection type, firmware banner, and stable status replies |
| Jog with laser off | X/Y directions and measured distance; use short moves within known travel |
| Frame with laser off | Position and dimensions agree with preview in the tested Start From mode |
| Small vector job | Correct size, position, power behavior, and reported completion |
| Small raster job | Correct orientation, spacing, power behavior, and reported completion |
| Pause and resume | Motion/output stop as expected, then the job continues correctly |
| Cancel | Job stops, output turns off, and subsequent recovery works |
| Homing or air assist | Only if fitted and configured; record which commands and hardware were tested |

A complete base-machine report covers the first seven rows. Unsupported
operations on other controller families can be Not applicable with a reason.
Failures stay visible in the evidence record until resolved. Recheck the
affected operation after changing defaults or controller behavior; routine
unrelated releases do not require every owner to repeat every test.

## Keeping the workload manageable

- Start with requested models that use GRBL and other existing adapters.
  Review small batches within one family, with separate defaults where hardware
  differs. Brand-wide compatibility claims are too broad.
- Let owners contribute profiles and test reports without writing Rust. A
  maintainer or coding assistant can turn reviewed settings into an entry.
- Ask manufacturers for their public setup profiles and firmware references.
  Those help establish defaults even before a community tester is available.
- Preserve regression cases from reports in shared automated suites so one
  controller fix benefits every preset using that adapter.
- Keep existing presets in service while their evidence records are backfilled.
  Their presence alone does not grant a new Community-tested label. Prioritize
  records for entries involved in support requests or settings changes.

The initial expansion candidate is the S9. Next candidates should come from
requests and usable source material, including related SculpFun models if their
own specifications support an entry. Do not copy the S9 defaults across a brand.

## Evidence records

- [Sculpfun S9, preset version 1](machines/sculpfun-s9.md): documentation-based,
  physical testing pending.

Existing S30 Pro Max, ACMER S2, xTool M1, and LaserPecker entries predate this
record format. Their current scope remains in the controller compatibility
document and existing tests; backfill records without inventing test history.
Generic GRBL is a configurable fallback and does not represent a specific model.
