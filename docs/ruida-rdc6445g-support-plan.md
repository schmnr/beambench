# Ruida RDC6445G support plan

Status: **Experimental execution path implemented; physical RDC6445G testing
pending.**

Target: **RDC6445G over Ethernet/UDP**

Initial product tier: **Experimental**

## Goal

Extend Beam Bench's existing RDC6442S Ruida adapter to the RDC6445G as an
explicitly selected Experimental controller. Reuse the shared protocol when the
controller answers the established Ruida identity and status queries. Keep the
acknowledgement, scoped cleanup, status-transition, stop, and completion checks
active during community testing.

## Boundaries

- Ethernet/UDP on port 50200 is the first transport. USB support is a separate
  project even though Ruida documents USB connectivity for the controller.
- Keep the existing `ruida` controller driver ID and shared compiler where the
  wire behavior is identical. Do not create a second copy of the Ruida stack.
- Do not report an RDC6445G as hardware-validated merely because it answers on
  the same port or accepts the same swizzle key.
- A controller may enter the Experimental runtime after its Ruida identity and
  machine-status queries succeed. A dedicated card-ID registry row is not a
  prerequisite for community testing.
- Preserve the existing no-blind-retry, scoped-file cleanup, recovery-required,
  and completion-confirmation rules.
- Keep unsupported features disabled. This includes absolute-position
  reporting, Start From Current Position, work-origin mutation, continuous jog,
  manual fire, rotary, dual-head control, controller parameter writes, and USB.
- Do not use Playwright for this work. Test the controller path with Rust unit,
  corpus, service, and virtual-controller tests; use frontend component tests
  and a manual packaged-desktop smoke test for the UI.

## Licensing rules

Beam Bench remains GPL-3.0-or-later throughout this work.

- Write the implementation in Beam Bench's Rust codebase from documented
  behavior, our own observations, and user-contributed diagnostics or captures.
- The pinned MIT-licensed MeerK40t Ruida implementation may remain an
  interoperability reference. Preserve its notice and update the pinned commit
  and `THIRD_PARTY_NOTICES.md` if a newer revision is used.
- The GPL-2.0-only `jnweiger/ruida-laser` project may corroborate protocol facts,
  but no code, translated code, or expressive implementation from it may enter
  Beam Bench.
- Do not add Ruida or RDWorks binaries, firmware, DLLs, drivers, SDK code,
  decompiled code, manuals, or substantial copied manual text to the repository
  or release packages.
- Sanitize user captures and diagnostics. Record permission before retaining a
  contributed fixture in the public repository.

Stop and alert the maintainer before continuing if support appears to require a
proprietary SDK, restricted vendor component, decompilation, or code from a
GPL-2.0-only source. That is a licensing and legal review point, not a routine
implementation choice.

Reference material:

- [Official RDC6445G manual](https://www.ruidacontroller.com/wp-content/uploads/2021/10/RDC6445G-Control-System-V1.2-Manual.pdf)
- [Official Ruida download center](https://www.rdacs.hk/download)
- [Pinned MIT-licensed MeerK40t reference](https://github.com/meerk40t/meerk40t/tree/76106c5bc54e4a33c9248e9916a0e3009b5bbf5d/meerk40t/ruida)
- [GPL-2.0-only ruida-laser license](https://github.com/jnweiger/ruida-laser/blob/master/LICENSE)

## Phase 1: collect a read-only fingerprint

The original `RuidaStorageClient::connect` read the card ID, rejected anything
other than `0x65106510`, and then reported RDC6442S unconditionally. The new
probe retains safe identity evidence and selects a known target through a
registry before any mutation is allowed.

- [x] Add a read-only probe result that retains the received card ID, endpoint,
  acknowledgement behavior, and any safely queried firmware or model evidence.
- [x] Ensure an unknown Ruida response appears in the bug-report diagnostics
  even when the connection is rejected.
- [x] Keep the probe bounded and read-only. It may use enquiry, card identity,
  machine status, and other confirmed read-only queries only.
- [x] Report unknown status bits without interpreting them.
- [x] Add a diagnostic-only outcome such as `recognized`, `unknown_variant`, or
  `inconclusive`. An inconclusive result must never enter Ready state.
- [ ] Ask the RDC6445G tester for the controller label, firmware version shown on
  its panel, connection type, and a Beam Bench report from this probe build.
- [ ] If more evidence is needed, provide a small capture procedure using a
  private network and Wireshark. Do not require the user to publish artwork,
  filenames, addresses, or a complete production job.

### Phase 1 gate

The explicit **Ruida (Experimental)** selection and successful read-only
identity and status replies are enough to enter the community-test runtime. A
real report is still required before claiming hardware validation or adding a
dedicated card-ID row.

## Phase 2: make Ruida targets explicit

Represent the known controller variants as data instead of embedding RDC6442S
in transport and service logic.

- [x] Add a small `RuidaCompatibilityTarget` registry containing the verified
  card ID, display model, transport, port, swizzle key, and supported status
  mask for each target. The target-aware adapter supplies its capability set.
- [x] Preserve `RDC6442S_ETHERNET_TARGET` with its existing values and behavior.
- [x] Create an RDC6445G experimental target from a successful read-only probe;
  retain its actual card ID and show the model when the mainboard version
  identifies the 6445 family.
- [x] Make `RuidaStorageClient`, `RuidaRuntime`, and `RuidaRuntimeSession` carry
  the matched target rather than comparing against `RDC6442S_CARD_ID` globally.
- [x] Derive positive identity, controller information, diagnostics, and error
  messages from the matched target. Remove hardcoded RDC6442S wording from
  shared preflight and service messages.
- [x] Keep one `ControllerDriverId::Ruida`. Do not add another frontend protocol
  selection unless the controller cannot be safely distinguished through a
  read-only probe.
- [x] Add a generic evidence-backed virtual-target constructor while sharing the
  protocol implementation. Add a named RDC6445G constructor only after its
  fingerprint is captured.

### Phase 2 gate

All existing RDC6442S tests must pass without fixture changes. A new card ID may
use the experimental capabilities only after both identity and status replies
succeed. A failed status query still refuses the connection.

## Phase 3: exercise read-only and storage behavior

The shared virtual controller covers the expected protocol. Add real RDC6445G
responses to the corpus as testers provide them instead of blocking the test
path until those captures exist.

- [ ] Add golden fixtures for enquiry, identity, machine status, file count, and
  document names using sanitized RDC6445G observations.
- [ ] Exercise acknowledgement, negative acknowledgement, error, checksum,
  timeout, duplicate reply, and unexpected reply behavior against the new
  virtual target.
- [x] Preserve unknown status bits in diagnostics without failing solely
  because an experimental controller reports additional flags. Required job
  transitions still depend on the established running, moving, and part-end
  bits.
- [ ] Upload Beam Bench's zero-output storage sentinel under a unique `BB*`
  filename, verify it appears exactly once, then delete and verify removal.
- [ ] Confirm packet size, chunking, filename limits, reply port, timeout, and
  retry behavior from the first hardware report; correct any difference found.
- [ ] Preserve recovery-required behavior after an ambiguous write or partial
  upload. Never blindly resend a command whose effect is unknown.

### Phase 3 gate

The virtual RDC6445G path must prove upload receipt and scoped deletion before
release. The first hardware test uses the same checks. Failure must leave
unrelated controller files untouched and produce enough diagnostics to correct
the adapter.

## Phase 4: prove job compatibility

The existing compiler should remain shared if the RDC6445G accepts the same
native job stream. Add variant-specific compilation only when captured evidence
shows an actual difference.

- [ ] Compare minimal vector and raster files produced for the RDC6445G with the
  existing protocol corpus. Store only fixtures that we have permission to
  publish.
- [ ] Verify vector, perforation, binary raster, grayscale raster, scan
  direction, overscan, rotated geometry, layer order, min/max power, and
  motion-inclusive bounds.
- [ ] Confirm the zero-power sentinel keeps both laser channels inactive.
- [ ] Run a zero-output frame first, with the tester watching the physical stop.
- [ ] Run one small, low-risk vector job and one small raster job after framing
  behaves correctly.
- [ ] Confirm that upload acknowledgement is not treated as job start or job
  completion.

### Phase 4 gate

Expose `can_frame` and `can_run_job` in the opt-in Experimental path after the
virtual controller reports a complete lifecycle and Beam Bench removes its
temporary file. A failed hardware lifecycle remains Experimental and enters
recovery rather than guessing that the job completed.

## Phase 5: test the shared controls

Expose the implemented Ruida capability set in Experimental mode. Test each
operation separately on the first RDC6445G and disable or specialize only the
commands that the hardware report shows are different.

- [ ] Select, start, pause, resume, stop, natural completion, and requested-stop
  races.
- [ ] XY home with output confirmed inactive.
- [ ] Finite X and Y step jog with bounded speed and output confirmed inactive.
- [ ] Optional finite lift-table jog after the tester explicitly identifies the
  machine's Z or U wiring.
- [ ] Disconnect, application cancel, network interruption, controller NAK,
  status loss, and restart recovery during each applicable operation.

Continuous jog, origin mutation, automatic Z or U job movement, manual fire,
rotary, USB, dual-head output, and parameter writes need separate plans and are
not acceptance requirements for RDC6445G support.

## Product and diagnostic work

- [x] Continue showing `Ruida (Experimental)` in the connection selector unless
  evidence requires an explicit model choice.
- [x] Show the exact matched target, card ID, transport, endpoint, evidence
  state, and enabled capabilities in Controller Info and diagnostic reports.
- [x] Give unknown variants a useful read-only error that asks for a report
  without implying the controller is defective.
- [x] Update English source strings and every locale file touched by the new
  model or message. Give the French text a human review because the first
  prospective tester is communicating in French.
- [x] Update `docs/controller-compatibility.md`, this plan,
  `docs/ruida-feasibility.md`, and `CHANGELOG.md` for the experimental runtime.
  Release notes and website compatibility copy remain tied to the next release
  and must say physical RDC6445G testing is still pending.
- [ ] Do not claim general Ruida or all-RDC6445G compatibility. Name the exact
  tested transport, fingerprint, firmware if known, and capabilities.

## Test and release checks

Run focused checks while developing:

```sh
cargo test -p beambench-ruida
cargo test -p beambench-service ruida
cargo test -p beambench-api
cargo test -p beambench-cli
cd tauri-app
npm test -- --run
npm run build
```

Before a release, run the repository's full Rust and frontend checks from
`CONTRIBUTING.md`, then complete a packaged-app smoke test on each release
platform. The desktop smoke should cover connection, identity display,
diagnostic-report preview, refusal when the required status query fails, and
the capability controls appropriate to the matched target.

The release sequence is:

1. Complete the shared adapter, RDC6445G virtual-controller, service, and
   packaged-app checks.
2. Ship the opt-in Experimental path with card ID and mainboard version in
   Controller Info and bug reports.
3. Ask the tester to connect, preview, zero-output frame, and run a small job.
4. Use the existing status and cleanup diagnostics to correct any
   controller-specific difference found on hardware.
5. Add a dedicated card-ID row when a report supplies the real fingerprint.

## Completion criteria

RDC6445G support is ready to release as Experimental when all of the following
are true:

- Beam Bench identifies RDC6445G version replies and retains the actual card ID
  for every experimental target.
- RDC6442S behavior and fixtures remain unchanged.
- The RDC6445G virtual target frames, starts, pauses, resumes, stops, completes,
  and cleans up its scoped controller files through the product service path.
- Every exposed motion command has a captured success path, bounded failure
  path, output-inactive rule where required, and virtual-controller test.
- Diagnostics identify the target, card ID, and mainboard version and preserve
  enough sanitized evidence to investigate a hardware difference.
- UI strings, translations, compatibility documentation, release notes,
  website copy, GPL source archive, and third-party notices agree with the
  shipped behavior.
- No proprietary Ruida component or GPL-2.0-only implementation code enters the
  source tree or release artifacts.

Promotion from Experimental to Supported is a later decision. It should require
successful use across more than one RDC6445G machine or firmware revision, not
just one tester completing one job.
