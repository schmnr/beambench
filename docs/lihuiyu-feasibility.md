# Lihuiyu M2/M3 Nano Experimental Feasibility

Status: **Protocol, compiler, virtual runtime, native USB backend, service,
desktop, API, and CLI product integration complete; hardware evidence pending**

Candidate first row: **Lihuiyu M2/M3 Nano in M2-compatible mode, CH341 USB
`1a86:5512`, binary beam output**

Hardware evidence: **not yet collected**

## Evidence And Implementation Route

The authoritative wire reference is the five-page `LHYMicro-GL2`
specification published by Lihuiyu Studio Labs on 2024-06-24 and archived in
the pinned MeerK40t repository. It defines the fixed 30-byte payload, CRC-8,
motion vocabulary, M2 timer formula, run controls, and controller status
signals. Beam Bench also uses the MIT-licensed MeerK40t Lihuiyu and CH341
modules at commit `76106c5bc54e4a33c9248e9916a0e3009b5bbf5d` as an independent
interoperability reference and source of golden output.

Beam Bench is implementing the protocol independently in Rust. It does not
include LaserDRW, CorelLaser, vendor firmware, dongle code, or proprietary
CH341 driver DLLs. The live backend uses the permissively licensed `nusb`
0.2.5 crate, which talks to IOKit on macOS, usbfs on Linux, and WinUSB on
Windows without bundling libusb.

## First-Row Boundary

The USB device must match CH341 vendor/product `1a86:5512`, accept EPP 1.9
initialization, and return a recognized Lihuiyu controller status before Beam
Bench may enter Ready. Passive USB naming or vendor/product IDs alone are not
treated as positive controller identity.

Device selection prefers the operating system's stable bus and physical port
chain instead of a reconnect-sensitive USB address. The claimed interface must
expose bulk OUT endpoint `0x02` and bulk IN endpoint `0x82` together. The
backend sends the pinned vendor/device `B1` control request with value `0x0102`
only when the runtime begins its positive identity sequence; it does not issue
the unused CH341 `SetParaMode` request or reset the device when the handle is
dropped.

M2-compatible mode provides binary laser on/off only. Optical power is set on
the machine's physical panel. Beam Bench will not emit M3-only `W` or `AT`
power commands, claim that per-layer software percentages alter tube current,
or write controller hardware parameters in this first row.

The offline compiler quantizes absolute geometry once onto the documented
one-mil grid, preserves arbitrary vector slopes with bounded Bresenham motion,
maintains perforation phase across vertices, converts grayscale pixels to
deterministic binary spans, forces frame paths to zero output, and packetizes
the resulting command stream. A pinned independent rectangle transcript and
unit coverage for raster, perforation, bounds, and packet reconstruction lock
that behavior before USB execution is enabled. Packetization removes EGV line
delimiters and its host-only completion-wait marker before forming controller
payloads.

## Software Gate

Before the row is exposed as **Lihuiyu M2/M3 Nano (Experimental)**, the same
product path must prove:

1. deterministic vector, raster, perforation, and zero-output frame
   compilation (**complete offline**);
2. exact packet padding, CRC, CH341 EPP wrapping, status handling, and bounded
   retry/timeout behavior (**complete against the virtual USB path**);
3. positive read-only controller status before mutation (**complete against
   the virtual USB path**);
4. home, finite XY jog, run, pause/resume, cancel, output-off, and completion
   semantics against a virtual controller (**complete**); and
5. native USB enumeration, endpoint validation, deterministic device selection,
   interface claim, EPP initialization, and actionable transfer errors
   (**complete in the controller crate**); and
6. explicit USB connection, identity/capability registration, product
   diagnostics, and action/job dispatch through the same service path used by
   the application (**complete against the virtual controller**); and
7. desktop USB discovery and selection, capability-gated controls, API USB
   discovery/connection routes, and CLI USB discovery/connection commands
   (**complete; native hardware evidence pending**).

The runtime never retries an ambiguously delivered packet. It retries only
after the controller explicitly reports checksum rejection, retains possible
output activity after lost acknowledgement or completion timeout, and requires
reconnection from that recovery state. A job becomes complete only after the
controller reports a documented terminal status; packet acceptance alone is
not treated as completion.

Hardware reports may later split M2 and M3 into narrower rows or enable an
M3-specific PWM adapter. They do not justify silently sending M3-only commands
to an unverified board.
