# Controller Detection Matrix

This matrix records the transports, safe probes, expected replies, and reset
rules Beam Bench uses when identifying common laser controllers. It is the
design input for connection detection and bug-report diagnostics; it is not a
claim that every controller listed here is currently supported.

## Detection Matrix

| Family | Common transport | Documented or common rate | Safe first probe | Positive evidence | Reset and current Beam Bench policy |
| --- | --- | --- | --- | --- | --- |
| GRBL 1.1 and GRBL-derived OEM boards | Serial | 115200 baud by default; 9600 and 57600 occur on older or vendor builds, and firmware can be compiled for other rates | Send the realtime `?` status byte, then use `$I` only after the stream is responsive | A valid `<State\|...>` status report, `Grbl ...` banner, or GRBL build information | Do not toggle DTR during the first sweep. Beam Bench tries the configured rate, then 115200, 921600, 460800, 230400, 57600, 38400, 19200, and 9600 without duplicates. A reset-assisted retry is reserved for an explicit classic-GRBL or generic-GRBL selection. |
| grblHAL | Serial, native USB, or TCP | 115200 baud is the core default for serial; board builds may differ | `?`, followed by `$I` after a valid stream is established | GRBL-compatible status plus an exact grblHAL identity | Never infer grblHAL from GRBL-shaped traffic alone. Auto-detect does not reset the controller and requires exact identity evidence before activating the adapter. |
| FluidNC | Serial, TCP, or WebSocket | Common serial setups use 115200 baud; network connections have no baud rate | `?`, followed by `$I` after a valid stream is established | GRBL-compatible status plus an exact FluidNC identity | FluidNC explicitly discourages senders from rebooting the controller merely because a connection opened. Beam Bench therefore never uses DTR/reset as a FluidNC identification step. |
| Marlin | Serial | Current Marlin defaults to 250000; 115200 is also common. Marlin accepts a wider configured set from 2400 through 1000000 | Send `M115` and wait for the bounded identity response | `FIRMWARE_NAME`, protocol, machine type, and advertised capability fields | DTR can reset some Arduino-class boards, so detection does not depend on a startup banner. Beam Bench tries the selected rate, 250000, and 115200 without duplicates and activates Marlin only from a parsed `M115` identity. |
| Snapmaker 2.0 | Serial, using a Marlin-derived firmware | Controller firmware determines the serial rate; it is covered by the Marlin rate sweep | `M115` | Exact Snapmaker firmware signature rather than a generic Marlin response | Uses the Marlin probe but activates the Snapmaker dialect only for exact evidence. Artisan and unidentified Snapmaker variants are not silently grouped into this row. |
| Smoothieware | Serial | 115200 baud is the published build default | `M115`, followed by read-only configuration checks only after identity | Exact Smoothieware identity and an enabled laser configuration | Beam Bench tries the selected rate and 115200 without duplicates. A generic G-code reply is not enough to activate the adapter. |
| LaserPecker LX1/LX1 Max, LP2 Plus, LP4, and LP5 | Serial | 460800 baud in the supported vendor profiles | Fresh `?` status, with no controller-settings query | Responsive idle/output-off GRBL status plus the user's exact LaserPecker model preset | Never toggle DTR. These products use explicit model selection because their status traffic alone is insufficient to distinguish every model safely. |
| LaserPecker LX2 | TCP, normally port 8888 | Not applicable | Fresh `?` status | Responsive GRBL status plus the explicit LX2 profile | No reset or settings write. The exact profile supplies its workspace and required job header. |
| xTool M1 (original) | HTTP over Wi-Fi or the USB network interface, port 8080 | Not applicable | `GET /cnc/status`, followed by optional read-only name, machine-type, firmware-version, and laser-power queries | Valid M1 status plus an M1 model/name or known 40.18 firmware-family value | Never run serial or GRBL probes. Connection is allowed only for an explicit xTool M1 choice. Missing optional identity endpoints are tolerated, but inconclusive identity fails closed. Upload uses the native M1 ZIP endpoint and is never retried after an ambiguous result. |
| Ruida RDC6442S and RDC6445G-compatible Ethernet controllers | UDP, normally port 50200 | Not applicable | Native read-only card/version and machine-status queries | A valid Ruida identity packet and valid status reply | Binary UDP protocol: never subject it to serial or G-code probes. Beam Bench activates RDC6442S exactly; 6445G-compatible variants stay in the opt-in experimental path and expose the returned card/version identity. |
| Lihuiyu M2/M3 Nano | Direct USB through CH341 EPP, normally `1a86:5512` | Not applicable | Native USB status/identity exchange | Valid Lihuiyu response on the expected USB interface | This is not a COM-port controller. Baud-rate and DTR troubleshooting are incorrect for it. Beam Bench uses its direct USB backend and reports the active OS USB driver when opening fails. |
| Trocen and TopWisdom DSP controllers | Vendor USB and/or Ethernet | Not applicable | No sufficiently documented, safe generic probe has been verified | A future adapter must require protocol-specific identity/status evidence | Deferred. Do not send GRBL, Marlin, or Ruida probes merely because the device has a USB or Ethernet connection. |
| JCZ/EZCad/BSL galvo controllers | Direct USB binary protocols | Not applicable | No generic safe probe; some variants require firmware-loading or controller-specific initialization | A future adapter must identify the exact board and initialization contract | Deferred. These are not serial G-code controllers and must not enter the serial baud sweep. |

## Detection Rules

1. Classify the transport before classifying the firmware. Serial, TCP,
   WebSocket, UDP, and direct USB are not interchangeable.
2. Start with a non-resetting, read-only probe. A controller reset is a
   protocol-specific fallback, not a universal discovery technique.
3. Preserve the evidence from every attempt. Reopening a port must not erase
   earlier transmit, receive, timing, rate, or reset-mode diagnostics.
4. Separate these outcomes in reports:
   - the port could not be opened;
   - the port opened but no bytes arrived at any supported probe rate;
   - bytes arrived but were unreadable at that rate;
   - a controller replied but its identity was unsupported or inconclusive;
   - a known controller returned a protocol error or alarm.
5. Do not activate a named adapter from shape alone. GRBL-compatible status is
   enough to establish a responsive stream, but FluidNC, grblHAL, Snapmaker,
   Smoothieware, and vendor-specific profiles still require exact evidence or
   an explicit user choice.
6. A connection bug report must include the machine/controller model and a
   short description. Automated evidence cannot determine external wiring,
   board labels, switch settings, or the model the user intended to connect.

## Evidence And Licensing Boundaries

Protocol facts come first from controller manuals, firmware documentation, and
firmware source maintained by the controller project. Permissively licensed
interoperability projects may be used to corroborate wire behavior or implement
missing details when their notices and attribution are retained. GPL projects
may be used as behavioral research, but Beam Bench does not copy their code.
Vendor SDKs, firmware, binaries, or protocol dumps with unclear or restrictive
redistribution terms are stop conditions for copying or bundling; public facts
can still be independently implemented when the license permits it.

Documentation and permissive sources used for this matrix:

- [GRBL interface documentation](https://github.com/gnea/grbl/blob/master/doc/markdown/interface.md) and [serial configuration](https://github.com/gnea/grbl/blob/master/grbl/config.h)
- [FluidNC firmware](https://github.com/bdring/FluidNC) and [sender guidance](https://github.com/bdring/FluidNC/wiki/GCode_Senders)
- [Marlin serial configuration](https://github.com/MarlinFirmware/Marlin/blob/bugfix-2.1.x/Marlin/Configuration.h) and [`M115` documentation](https://marlinfw.org/docs/gcode/M115.html)
- [Smoothieware build defaults](https://github.com/Smoothieware/Smoothieware/blob/edge/Rakefile)
- [grblHAL stream defaults](https://github.com/grblHAL/core/blob/master/stream.h)
- [Snapmaker 2.0 Marlin firmware](https://github.com/whimsycwd/SnapmakerMarlin)
- [xTool M1 specifications](https://support.xtool.com/article/1912), [XCS operating guide](https://support.xtool.com/article/1650), and the MIT-licensed [xtm1_toolkit interoperability implementation](https://github.com/fritzw/xtm1_toolkit)
- [Ruida RDC6445G manual](https://www.ruidacontroller.com/wp-content/uploads/2021/10/RDC6445G-Control-System-V1.2-Manual.pdf)
- [MeerK40t](https://github.com/meerk40t/meerk40t), an MIT-licensed interoperability implementation used to corroborate Ruida, Lihuiyu, and galvo controller behavior
- [TopWisdom TL-A1 manual](https://www.manualslib.com/manual/3135251/Topwisdom-Tl-A1.html) and [Trocen AWC708C Lite manual](https://www.plexishop.it/pdf/Trocen%20AWC708C%20LITE%20user%20manual.pdf)

Existing controller-specific decisions remain in
[`ruida-feasibility.md`](ruida-feasibility.md),
[`ruida-rdc6445g-support-plan.md`](ruida-rdc6445g-support-plan.md), and
[`lihuiyu-feasibility.md`](lihuiyu-feasibility.md).
