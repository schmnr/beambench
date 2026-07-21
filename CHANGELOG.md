# Changelog

## Unreleased

- Added finite manual lift-table jogging for the Experimental Ruida RDC6442S
  adapter. Ruida machine profiles can select the controller's Z or U channel,
  since cabinet machines use both conventions, and the Move panel exposes
  output-disabled positive and negative table steps at the configured feed.
  The choice defaults to disabled instead of guessing a machine's wiring.
  Exact Z/U command bytes, bounded runtime behavior, and simulated table
  positions are covered by the Ruida protocol and virtual-controller tests.
- Added direct import for modern Lbrn `.lbrn2` projects and legacy `.lbrn`
  files. Beam Bench recreates Lbrn color layers and their cut/image
  settings, preserves paths, rectangles, ellipses, editable text, groups,
  embedded bitmaps, project notes, material height, placement, rotation, scale,
  and shear, and supports older projects that reuse shared path or bitmap data.
  Lbrn projects work through the normal file picker, batch importer,
  drag-and-drop flow, Local API, and CLI import paths.
- Added a dedicated Experimental LaserPecker controller choice over serial and
  network connections. Built-in official-profile presets cover LX1, LX1 Max,
  LX2, LP2 Plus, LP4 / LP4 Safeguard, and LP5, including 460800-baud serial
  operation, LX2 TCP port 8888, top-left workspaces, per-model S-value scales, `START_PRINT`,
  regular-mode commands, and separate LP4/LP5 450 nm and 1064 nm selections.
  The LaserPecker path avoids DTR resets and controller-settings queries, enters
  Ready only after a fresh idle/output-off GRBL status response, and is covered
  by official-profile fixtures plus a loopback network replay. The LP1 family,
  LP2 / LP2 Safeguard, and LP3 are not claimed because LaserPecker does not
  publish a compatible desktop GRBL/Lbrn control path for them.

## 0.1.9

- Added explicit serial and network controller connections to the Local API and
  CLI. Headless clients can select or auto-detect GRBL-family, Marlin,
  Snapmaker 2.0, Smoothieware, and Ruida adapters, receive the same
  backend-owned controller-choice challenge as the desktop, and continue that
  attempt without silently falling back to GRBL. Transport-specific CLI choices
  prevent selecting a serial-only adapter for a Network connection.
- Added a public controller compatibility guide covering the Supported GRBL
  baseline; the Experimental FluidNC, grblHAL, Generic GRBL-compatible,
  Marlin, Snapmaker 2.0, Smoothieware, Ruida, and Lihuiyu choices; connection
  steps; exact first-row limitations; and the normal in-app bug-report path.
  Trocen, TopWisdom, and galvo controllers are explicitly listed as deferred.
- Hardened Experimental K40/Lihuiyu and Ruida transfers so Stop, cancel, and
  status remain responsive while a job is being prepared. K40 transfers now
  advance packet by packet, while Ruida aborts close the upload stream, remove
  the partial controller file, and verify cleanup before returning to Ready.
  Any shutdown or cleanup that cannot be confirmed now requires recovery
  instead of claiming the laser is safely idle.
- Corrected controller safety checks and output generation: K40 raster travel
  keeps laser power off across blank space, K40 packet termination is
  deterministic, and Ruida jobs fail closed when any motion—including rotated
  raster overscan—would leave the configured bed. Marlin and Snapmaker no
  longer report an unsupported preflight as a passing safety check.
- Improved Experimental Marlin, Snapmaker, and Smoothieware throughput by
  streaming commands as acknowledgements arrive. Machine controls are now
  shown from reported controller capabilities, and staged uploads use a
  distinct Preparing/Uploading state so Stop remains available without
  offering actions such as Pause that cannot work yet.
- Quality-test dialogs now require a connected, Ready, idle machine and active
  profile before enabling Frame or Start, while Preview and G-code export stay
  available offline. Machine-state text follows the selected appearance,
  connection failures preserve stop/recovery warnings, and large buffered TCP
  replies no longer tear down an otherwise healthy FluidNC or grblHAL session.
- Added the stock K40/Lihuiyu controller path with an exact M2/M3 Nano
  M2-compatible protocol foundation for CH341 USB `1a86:5512`. Manufacturer
  packet framing, CRC-8, controller status, compact motion distances, and M2
  vector/raster speed codes are frozen in pinned offline corpora. The native
  compiler now covers vector, perforation, raster, and zero-output frame work,
  preserves arbitrary line geometry on the one-mil motion grid, bounds motion
  expansion, packetizes complete jobs, and converts grayscale input to explicit
  binary spans. A transport-neutral CH341 runtime now requires positive
  read-only status identity before mutation, retries only explicit checksum
  rejection, preserves uncertainty after ambiguous I/O, and exercises job
  transfer, terminal completion, pause/resume, cancel, output-off, home, unlock,
  and finite XY jog against a virtual M2 controller. A native-platform USB
  backend now enumerates exact CH341 devices by physical port, validates bulk
  endpoints `0x02`/`0x82`, claims the interface, and performs the pinned EPP
  1.9 initialization through IOKit on macOS, usbfs on Linux, or WinUSB on
  Windows without bundling a proprietary driver or libusb. This first row leaves
  optical power on the machine's physical panel and does not send M3-only PWM
  or hardware-parameter commands. The controller is now registered through the
  shared service as an explicit Experimental USB choice with its exact identity,
  capabilities, and diagnostics. The same product dispatch used by native USB
  routes home, unlock, finite XY jog, zero-output frame, job start, pause/resume,
  cancel, and emergency output-off through virtual integration tests without an
  additional experimental confirmation prompt. Desktop connection screens now
  discover and select matching USB devices, API clients can list and connect to
  them through dedicated routes, and the CLI provides equivalent discovery and
  connection commands. Blocking USB job and frame work runs off the UI and API
  async executors.
- Added an explicit **Ruida (Experimental)** Network controller choice for the
  exact RDC6442S Ethernet/UDP target. After a read-only card-identity match, the
  product now compiles and uploads native vector/raster jobs, confirms native
  running and terminal states, cleans up its uniquely scoped controller file,
  and routes zero-output framing, XY home, finite output-disabled XY step jog,
  pause, resume, cancel, and verified emergency-stop behavior through the live
  service. Unsupported absolute positioning, Start From Current Position
  placement, continuous jog, work-origin, manual-fire, override, Z/U-axis,
  rotary, USB, and controller-parameter actions remain unavailable. UDP
  virtual-controller tests exercise the same public
  paths; hardware evidence is still pending, so the adapter remains
  Experimental / Emulated.
- Added Ruida XY homing and bounded-speed step jogging for the exact RDC6442S target. The observed native commands use the controller's explicit no-output rapid option, accept only finite non-zero distances and speeds representable on the wire, distinguish manual motion from laser-job execution, and require recovery after ambiguous or partially delivered multi-axis movement. Exact command bytes, output-inactive behavior, home/jog state transitions, and partial X/Y delivery run against the virtual controller. Work-origin mutation remains deferred because its controller contract is not yet sufficiently identified.
- Added the Ruida execution lifecycle for the exact RDC6442S Ethernet target. Observed native commands now select, start, pause, resume, and stop a verified uploaded file, while bounded machine-status polling distinguishes unowned activity, queued work, running, paused, requested stop, natural part-end, and confirmed completion. The runtime never equates a command acknowledgement with motion or completion, never retries an ambiguously delivered control command, and enters explicit recovery on unknown status bits, status loss, unexpected activity, unexpected idle, or confirmation timeout. The complete lifecycle and stop-versus-completion races run against the virtual controller.
- Completed the Ruida upload and scoped-storage layer for the exact RDC6442S Ethernet target. The bounded UDP client verifies card identity, packet checksums, ACK/NAK behavior, 1470-byte chunking, upload progress, randomized Beam Bench controller filenames, controller file listing, receipt inspection, and uniquely scoped deletion. A deterministic one-millimetre sentinel encodes zero power on both laser channels; multi-packet, explicit-NAK retry, duplicate-name, storage-reset, invalid-job, and ambiguous-timeout recovery cases run against the virtual controller. ACK timeouts are never blindly resent because the observed protocol has no sequence number, and any ambiguous partial upload requires an external controller reset before more I/O.
- Completed the Ruida native-job compiler for vector, perforated vector, frame, and binary or grayscale raster geometry. Native output preserves scan direction, horizontal or vertical work modes, arbitrary-angle coordinate rotation, overscan motion, grayscale power transitions, threshold ramps, perforation phase across vertices, layer order, and motion-inclusive bounds; independent MeerK40t golden files cover both vector and raster `.rd` streams. Unresolved offset-fill sentinels still fail closed because valid planner output resolves them to vector rings before compilation.
- Began the Experimental Ruida backend with a dedicated transport-neutral packet/value codec, deterministic RDC6442S Ethernet/UDP fixtures, read-only card identity and machine-status commands, checksum rejection, and a virtual controller. The implementation route and third-party license notice are recorded; the later bullets describe the mutation and execution layers built on this foundation.
- Added Experimental FluidNC and grblHAL network connections over bounded TCP/Telnet streams. Both connection surfaces accept a host and TCP port, default to port 23, and offer Auto-detect or an explicit named controller. Bannerless connections require a fresh read-only GRBL status response before exact `$I`/`$I+` identity probing; runtime state and diagnostics preserve the actual TCP endpoint, and loopback tests exercise both families through Ready state.
- Added Smoothieware as an Experimental serial controller choice. Exact `M115` identity plus the effective enabled laser configuration activate a dedicated runtime; Auto-detect tries it after GRBL and Marlin. Vector, raster, perforation, framing, and quality-test jobs use Smoothieware's native motion-block `S` semantics, the controller-reported maximum power scale, explicit speed-proportional or constant-power selection, and `S0` on every non-burning feed move. Jobs stream one command per acknowledgement and complete on terminal `M400`; cancellation sends `M112` and requires reconnect.
- Added Snapmaker 2.0 as an Experimental serial controller choice. Exact `SM2-*`/`SnapmakerMarlin` identities activate a dedicated runtime that emits the vendor-documented constant `M3` or dynamic `M4` commands with PWM `S0-255`, preserves custom G-code, runs jobs, frames, and quality tests through acknowledged progress, and completes on terminal `M400`. Cancellation sends best-effort `M112` and requires reconnect because Snapmaker reports its emergency parser disabled. Artisan is not claimed by this adapter.
- Added standard Marlin as an Experimental serial controller choice. Exact `M115` identities now activate a dedicated runtime, generate Marlin jobs and frames from the active profile power range, report acknowledged progress through the normal job ticker, and require reconnect after `M112` cancellation or emergency shutdown. Auto-detect tries Marlin after GRBL identity cannot be established.
- Added 921600-baud profile and connection options for VMS LX2b controllers, plus a bounded automatic retry from the standard 115200-baud GRBL connection path when the controller is silent.
- Added a live standard-Marlin serial-session core with exact `M115` activation, one-command-per-ack execution, a terminal `M400` completion acknowledgement, and `M112` cancellation that requires reconnect instead of claiming a confirmed physical stop. Live activation requires Marlin's reported emergency parser capability, and recognized Snapmaker firmware remains reserved for its dedicated adapter.
- Corrected generated Marlin jobs so the completion `M400` is the actual final command after any configured finish-position travel.
- Snapmaker 2.0 firmware signatures are recognized as a distinct Marlin-derived controller identity instead of being routed through generic Marlin assumptions; the dedicated adapter now consumes that exact identity.
- Added complete offline Marlin job generation from Beam Bench execution plans, including vector, raster, framing, air assist, Z offsets, custom Start/End G-code, finish positioning, configured Marlin power scales, and standard or inline laser modes backed by golden job fixtures.
- Added an offline Marlin laser-command contract with standard, continuous-inline, and dynamic-inline modes; PWM, percent, RPM, servo, and custom power scales; explicit laser-off commands; and an `M400` completion barrier backed by a versioned command corpus.
- Added offline Marlin `M115` identity and capability parsing with exact firmware matching, conflict detection, bounded input, and replay transcripts.
- Added a transport-neutral, one-command-per-ack G-code protocol foundation for the upcoming Marlin and Smoothieware adapters, with fail-closed timeout, error, and resend handling backed by offline response transcripts.
- Exact controller matches and explicit controller selections now connect without duplicate Experimental confirmations or technical probe notices. Exact FluidNC and grblHAL adapters also expose the normal shared GRBL-family actions, including homing when enabled by the machine profile.
- Beam Bench can now connect to exactly identified FluidNC and grblHAL serial controllers through clearly labeled Experimental options while preserving their controller identity in diagnostics and runtime state.
- Machine profiles now provide clearly labeled, multiline **Start G-code** and **End G-code** editors for commands that must run automatically around every generated job.
- The G-code console now shows raw controller replies live, avoids duplicate sent commands, and clears its retained history when **Clear** is used. Its busy-job message no longer incorrectly suggests that pausing permits manual commands.
- Toolbar macros now carry always-visible numbered badges, making multiple saved macros distinguishable without relying on hover or color alone.
- Machine profile controls now keep a safe click area beside macOS overlay scrollbars, and Dot Width Correction uses a full-row checkbox target in both profile editors.
- Individual machine profiles can now be exported and imported as portable `.bbprofile` files. Imported profiles receive a new identity, remain inactive for review, and exclude computer-specific camera calibration data.
- Art Libraries can now insert PDF, EPS, and both PDF-compatible and PostScript AI artwork from shared `.bbart` files, matching the formats accepted by the multi-file importer.
- PDF and PDF-compatible AI imports now preserve standard RGB, grayscale, and CMYK paint colors, reuse existing matching color layers, and create safe Line layers for new colors. Beam Bench PDF exports now include their source layer colors as well.
- Platform release workflows now produce versioned, architecture-checked standalone CLI archives with SHA-256 checksums. macOS and Windows CLI artifacts follow their existing production signing requirements.
- Platform release workflows now validate and build the exact requested tag and
  serialize same-tag platform runs, preventing concurrent upload races from
  creating duplicate draft releases.

## 0.1.8

- Beam Bench now offers System, Light, and Dark app appearances in Settings. The app chrome and native window follow this preference while the drawing workspace background remains independently configurable.

## 0.1.7

- Camera capture and overlay setup are more reliable across macOS, Windows, and Linux. Release builds list real cameras only, duplicate device names resolve consistently, permissions and busy-device failures are clearer, and the signed macOS app now carries the required camera entitlement.
- Camera mappings and alignments now require at least three non-collinear point pairs, reject invalid frames and transforms, stay in preview until explicitly saved, and are invalidated when the camera or capture resolution changes. Manual overlay adjustment also returns to the correct saved mapping when cancelled.
- Creating a machine profile and immediately making it active no longer fails with "Profile not found". Unsaved profiles are saved before activation and cannot be deleted before they exist.
- Exact raster burn geometry, including overscan, is no longer clipped to simplified fill outlines in the preview.
- Serial-port access failures now give platform-specific guidance instead of Windows-only COM-port wording on macOS or Linux.
- Trace Image point markers remain visible on light images, and Space-drag panning works even after a numeric field has focus.
- Adjust Image has an **Auto** action that suggests conservative brightness, contrast, gamma, and sharpen settings from the selected image.

## 0.1.6

- Open lines can now be offset. The offset dialog labels open-line choices as Side A, Side B, and Both sides, defaults open lines to Both sides, and shows a live canvas preview before applying the offset. Thanks to the user who reported this.
- Selected horizontal and vertical lines are much easier to grab and move. Resize and rotate handles no longer steal the drag target from the line's move handle.
- After a crash, the next successful launch now opens the feedback dialog with captured, scrubbed crash diagnostics ready for review and submission.
- The Linux app has additional startup hardening for Wayland/WebView graphics failures.
- Scan-angle bounds checks now report the actual lower-bound problem instead of describing negative coordinates as exceeding the far edge of the bed.

## 0.1.5

- Raster engraving now sweeps each scanline continuously instead of reversing between separated engraved areas on the same row. This prevents the forward-and-back motion that could distort images and add unnecessary mechanical movement. Thanks to Chris Munguia for reporting this.

## 0.1.4

- Quitting from the Beam Bench menu now works reliably on macOS while still prompting before discarding unsaved work. Thanks to the users who reported this.
- Fatal serial and controller failures now cleanly end the job and disconnect stale sessions instead of leaving Beam Bench stuck in a running state. The last failure details and command traffic are retained in diagnostic reports to make controller-specific problems actionable.
- Machine Zero now clearly requires homing during the current connection before it can be used, preventing an unsafe machine-coordinate move with an untrusted origin.
- Text can now fall back character by character to installed system fonts, so mixed-language text including Chinese renders into the same vector geometry used by the canvas, preview, export, and laser output. Missing characters are identified when no installed font supports them.

## 0.1.3

- Jobs that reach Idle while Beam Bench is still waiting for missing serial acknowledgements now fail with a clear error instead of hanging indefinitely. The job progress panel shows the failure reason so the controller/connection problem is visible.
- Serial writes now send complete command payloads before reporting success, preventing partial writes from silently corrupting streamed commands.
- Rapid Trace Image preview updates no longer fail because overlapping performance timing marks reuse the same name. Performance instrumentation is now best-effort and cannot break the traced operation.
- Moving or resizing floating dialogs no longer crashes if the pointer is released while React is still applying the frame update. Thanks to the user who reported this.
- Right-side docked panels show visible scrollbars again when their content is taller than the panel.
- The Linux app now runs on older distributions again, including Debian 12 and Ubuntu 22.04. Recent builds were produced on a very new system and refused to start unless the computer had a 2024-era system library. Thanks to the user who reported this.
- Start From Current Position and User Origin now run the job exactly where the laser head is parked. The job anchor never accounted for where the design sits on the canvas, so jobs ran off to an unexpected spot on the bed instead of starting at the head. Framing and the material, focus, and interval tests are anchored the same corrected way, and vertical or angled photo engravings now shift correctly too. Thanks to the user who reported this.
- On Mac, updating from the installer disk image, a quarantined download, or a different disk used to fail with a cryptic "Cross-device link" error. The app now explains the situation before downloading and tells you to move Beam Bench into the Applications folder. Thanks to the user who reported this.
- SVG files with parts in different colors now import as separate objects, one per color, so each part can get its own cut and engrave settings. Same-color artwork still imports as a single piece. Thanks to the user who reported this.
- Choosing the custom finish position in the laser panel no longer fails with an error message. Thanks to the user who reported this.

## 0.1.2

- The Mac app now declares that it needs macOS 10.15 or later. On older systems such as macOS 10.14, the app used to open a dark, unresponsive window; macOS now shows a clear message instead of letting it launch. Thanks to the user who reported this.
- If the interface ever fails to load, a native message now explains the likely cause and where to get help, instead of leaving a silent dark window, and Quit works in that state instead of requiring a force quit.
- Fixed the app failing to start on some Linux systems, especially with NVIDIA graphics or recent graphics drivers. Thanks to the user who reported this.
- Fixed jobs stalling after the first lines on some controllers: the app filled the controller's memory by one byte too many, which silently corrupted a command and froze the stream. Thanks to the user who reported this.
- Machine position, speed, and status now update live while a job is running instead of freezing at their pre-job values.
- Job completion now waits for a fresh report from the machine, so short jobs can no longer show Completed while the laser is still moving.

## 0.1.1

- Added production update notification support.
- Job progress now shows Completed only after the machine finishes all motion, not when the last command is sent.
- Framing now follows your machine profile settings, including power mode and maximum power scale.
- Console commands and macros are blocked while a job is running to protect the job stream.
- The remote control API is now off by default and limited to your own computer when enabled.
- Crash recovery files are saved where the recovery check can find them.
- Closing the app, starting a new project, or opening a project now asks before discarding unsaved changes.
- Holding tabs are cut correctly when they cross a corner of the shape.
- Image engraving preview now reflects your actual power settings instead of always showing full darkness.
- Layer flash highlighting on the canvas now works.
- Fixed imports: PDF files from common design apps, compact SVG number notation, SVG text sizes in points, closed DXF polylines, and photo rotation from phone cameras.
- Cut Shapes no longer leaves extra objects behind when a selected object is locked.
- Vertical engraving jobs are checked against the correct bed dimensions.
- Art Library sizes, framing messages, and several dialogs now follow your language and unit settings.
- Jobs are checked before starting so overscan and scanning offset can never push the laser head past the edge of the bed.
- Overscan lead-in distance stays correct when scanning offset is active, preventing edge banding on fast engravings.
- Perforation gaps now match the configured timing exactly, and the laser reliably turns off in every gap.
- Custom start and finish points, the close-paths tolerance, and machine speed limits now follow your inch or millimeter setting.
- Warning messages in several dialogs were invisible due to a missing style and now display properly.
- Typing in the position and size toolbar applies when you finish editing instead of jumping after every keystroke, and the scale fields work again.
- The progress bar now says it is finishing while the machine completes buffered moves, and cancelled jobs show a Cancelled state.
- A visible Stop button appears while the machine is jogging.
- Holding tabs, snapping guides, resizing past zero, and several other canvas interactions behave correctly.
- Keyboard focus stays inside dialogs, repeated error messages no longer stack up, and the console no longer forces you to the bottom while reading.
- Dropping files onto the window to import them now works. Thanks to the user who reported this.
- Controllers running rebranded firmware (boards that announce themselves with a custom name instead of GRBL) now connect correctly. Thanks to the user who reported this.
- Imported files always keep their original measurements. Designs larger than the workspace are no longer scaled down silently: they import at true size with a warning. Thanks to the user who reported this.
- Imported photos and images are sized using the resolution information saved in the file, so a scan or export made at a specific physical size arrives at that size.
- Machines that intentionally run in spindle mode with a constant power profile, such as needle cutters and servo-driven tools, can now start jobs without a laser mode warning blocking them.
- Choosing Don't Save when closing now closes the window the first time instead of needing a second click.
- Pasting an image from the system clipboard (such as a screenshot) now works. The Paste menu item stayed disabled unless something was copied inside the app first, which blocked the shortcut entirely. Thanks to the user who reported this.
- Pasting a copied image file now imports the actual image instead of the file's icon, and the right-click Paste on the canvas also reaches the system clipboard.
- Outline framing now follows the actual shape of your design instead of tracing the same rectangle as regular framing. Thanks to the user who reported this.
- After framing, the laser no longer makes a surprise trip to the corner of the bed.
- Shapes being drawn now stay the correct size while you zoom or pan mid-draw. Thanks to the user who reported this.
- Connecting to a controller that uses a different communication speed now works automatically: when the controller answers with unreadable data, the app retries the common alternative speeds and remembers the one that works. Failed connections now explain the likely cause instead of a generic timeout.

## 0.1.0

- Initial macOS direct-download release.
