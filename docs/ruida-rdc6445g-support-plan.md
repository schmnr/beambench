# Ruida RDC6445G support plan

Status: **In progress. Read-only diagnostic probe implemented; RDC6445G
fingerprint pending.**

Target: **RDC6445G over Ethernet/UDP**

Initial product tier: **Experimental**

## Goal

Extend Beam Bench's existing RDC6442S Ruida adapter to the RDC6445G without
assuming that the two controllers share every identity value, status bit,
storage behavior, or motion command. Keep the current RDC6442S path unchanged
until captured RDC6445G behavior proves which parts can safely be shared.

The first useful release should identify an RDC6445G, report what it found, and
fail safely when the controller differs from the known protocol. Later releases
can enable storage, job execution, and manual motion as each behavior passes its
own evidence gate.

## Boundaries

- Ethernet/UDP on port 50200 is the first transport. USB support is a separate
  project even though Ruida documents USB connectivity for the controller.
- Keep the existing `ruida` controller driver ID and shared compiler where the
  wire behavior is identical. Do not create a second copy of the Ruida stack.
- Do not treat an RDC6445G as an RDC6442S merely because it answers on the same
  port or accepts the same swizzle key.
- No controller mutation occurs until Beam Bench has matched an accepted,
  model-specific compatibility target using read-only evidence.
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

Record the observed RDC6445G identity and reply fixtures in the plan before
enabling any mutation. If the controller does not expose a unique model value,
define the compatibility target from the exact observable protocol fingerprint
and document that the tested hardware carried an RDC6445G label. A manual model
selection alone is not positive controller identity.

## Phase 2: make Ruida targets explicit

Represent the known controller variants as data instead of embedding RDC6442S
in transport and service logic.

- [x] Add a small `RuidaCompatibilityTarget` registry containing the verified
  card ID, display model, transport, port, swizzle key, and supported status
  mask for each target. The target-aware adapter supplies its capability set.
- [x] Preserve `RDC6442S_ETHERNET_TARGET` with its existing values and behavior.
- [ ] Add an RDC6445G target only after Phase 1 supplies a defensible fingerprint.
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

All existing RDC6442S tests must pass without fixture changes. An unknown card
ID must still refuse upload, execution, home, jog, and table motion.

## Phase 3: verify read-only and storage behavior

Build the RDC6445G corpus from captured responses rather than copying the
RDC6442S corpus and renaming it.

- [ ] Add golden fixtures for enquiry, identity, machine status, file count, and
  document names using sanitized RDC6445G observations.
- [ ] Exercise acknowledgement, negative acknowledgement, error, checksum,
  timeout, duplicate reply, and unexpected reply behavior against the new
  virtual target.
- [ ] Confirm the recognized machine-status bits and fail on unknown bits.
- [ ] Upload Beam Bench's zero-output storage sentinel under a unique `BB*`
  filename, verify it appears exactly once, then delete and verify removal.
- [ ] Confirm packet size, chunking, filename limits, reply port, timeout, and
  retry behavior rather than inheriting those values without evidence.
- [ ] Preserve recovery-required behavior after an ambiguous write or partial
  upload. Never blindly resend a command whose effect is unknown.

### Phase 3 gate

The target may gain controller-storage access only after upload receipt and
scoped deletion work on both the virtual controller and one real RDC6445G.
Failure must leave unrelated controller files untouched.

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

Enable `can_frame` and `can_run_job` for the RDC6445G target only after the
controller reports a complete lifecycle for real vector and raster jobs and
Beam Bench removes its temporary file. A failed lifecycle remains Experimental
and enters recovery rather than guessing that the job completed.

## Phase 5: enable controls one at a time

Do not inherit the RDC6442S capability set as a block. Promote each operation
after its command and status transition have been observed on the RDC6445G.

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

- [ ] Continue showing `Ruida (Experimental)` in the connection selector unless
  evidence requires an explicit model choice.
- [ ] Show the exact matched target, card ID, transport, endpoint, evidence
  state, and enabled capabilities in Controller Info and diagnostic reports.
- [x] Give unknown variants a useful read-only error that asks for a report
  without implying the controller is defective.
- [x] Update English source strings and every locale file touched by the new
  model or message. Give the French text a human review because the first
  prospective tester is communicating in French.
- [x] Update `docs/controller-compatibility.md`, this plan,
  `docs/ruida-feasibility.md`, and `CHANGELOG.md` for the diagnostic-probe
  stage. Release notes and website compatibility copy remain tied to the next
  release rather than claiming RDC6445G support now.
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
diagnostic-report preview, refusal of an unknown target, and the capability
controls appropriate to the matched target.

The release sequence is:

1. Ship or privately provide a diagnostic probe build that cannot mutate an
   unknown RDC6445G.
2. Record the returned fingerprint and add deterministic virtual-controller
   coverage.
3. Enable storage and zero-output framing for the exact target.
4. Enable job execution after the tester confirms the required real-controller
   lifecycle.
5. Add home and finite jog only after their separate tests pass.

## Completion criteria

RDC6445G support is ready to remain available as Experimental when all of the
following are true:

- Beam Bench recognizes an evidence-backed RDC6445G protocol fingerprint and
  rejects unknown Ruida targets before mutation.
- RDC6442S behavior and fixtures remain unchanged.
- Vector and raster jobs frame, start, pause, resume, stop, complete, and clean
  up their scoped controller files on the real test machine.
- Every exposed motion command has a captured success path, bounded failure
  path, output-inactive rule where required, and virtual-controller test.
- Diagnostics identify the target and preserve enough sanitized evidence to
  investigate a failure.
- UI strings, translations, compatibility documentation, release notes,
  website copy, GPL source archive, and third-party notices agree with the
  shipped behavior.
- No proprietary Ruida component or GPL-2.0-only implementation code enters the
  source tree or release artifacts.

Promotion from Experimental to Supported is a later decision. It should require
successful use across more than one RDC6445G machine or firmware revision, not
just one tester completing one job.
