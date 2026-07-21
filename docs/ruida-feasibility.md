# Ruida Experimental Feasibility Decision

Status: **Software-ready as Experimental / Emulated**

Candidate first row: **RDC6442S, Ethernet/UDP port 50200, swizzle key `0x88`**

Hardware evidence: **not yet collected**

## Implementation And Licensing Route

Beam Bench will use a clean Rust implementation of the publicly observed Ruida
wire contract. The primary interoperability reference is the MIT-licensed
[MeerK40t Ruida implementation at commit `76106c5`](https://github.com/meerk40t/meerk40t/tree/76106c5bc54e4a33c9248e9916a0e3009b5bbf5d/meerk40t/ruida).
Its license permits use, modification, and redistribution with preservation of
the copyright and permission notice; Beam Bench records that notice in
`THIRD_PARTY_NOTICES.md`.

The GPL-licensed `jnweiger/ruida-laser` research was consulted only to
corroborate public protocol facts such as UDP port and checksum framing. No
source code from that project is incorporated into Beam Bench.

The implementation does not include Ruida or RDWorks binaries, firmware,
vendor SDKs, cryptographic material, or confidential captures. The byte
swizzle is an interoperability transform rather than an access-control bypass.

## Evidence Available Now

- Ethernet uses UDP port 50200 for the program/control channel.
- Controller-bound datagrams contain a two-byte big-endian wrapping checksum
  followed by a swizzled payload and are bounded to 1472 bytes.
- Controller replies are swizzled without that checksum prefix.
- Clear replies use `0xCC` for acknowledgement, `0xCF` for negative
  acknowledgement, and `0xCD` for an error.
- `DA 00 05 7E` is a read-only card-identity query. The current candidate row
  reports card ID `0x65106510` for RDC6442S.
- `DA 00 04 00` is a read-only machine-status query.
- Open source reports successful hardware use on an RDC6442S over Ethernet,
  making it a better evidence-backed first target than a generic “all Ruida”
  claim.

The `beambench-ruida` crate freezes those facts into deterministic golden
vectors and a virtual RDC6442S controller. Its software-only layers compile
native vector/raster jobs; exercise bounded upload, list, inspect, and uniquely
scoped delete behavior; and verify selection, start, pause, resume, natural
completion, requested stop, and recovery against that virtual controller.

The desktop product now exposes an explicit **Ruida (Experimental)** Network
choice. It requires the exact RDC6442S card identity before any mutation, then
uses the native UDP path for compiled jobs, zero-output framing, XY homing,
finite output-disabled XY step jogging, configurable finite Z- or U-channel
lift-table jogging, pause, resume, cancel, verified completion, and
controller-file cleanup. It does not fall back to GRBL or a generic DSP
simulation.

## Decision

Proceed with the exact-row native adapter and expose it as **Ruida
(Experimental)**. The software distribution gate requires:

1. deterministic vector and raster job compilation;
2. bounded UDP acknowledgement, retry, and timeout handling;
3. an exact read-only RDC6442S identity match before mutation or execution;
4. truthful upload and execution progress against the virtual controller; and
5. diagnostics sufficient for beta testers to submit sanitized protocol
   transcripts when a controller variant diverges.

Hardware reports may broaden or change the candidate row. They do not justify
silently treating another Ruida card ID or swizzle key as RDC6442S.

All five software requirements now pass through the same service and desktop
paths used by the product. Hardware evidence remains open, so this is not a
claim of broad Ruida compatibility or hardware-validated support. Absolute
positioning, Start From Current Position placement, controller work-origin
mutation, continuous jog, automated job Z/U motion, rotary, dual-head, USB,
manual fire, and controller parameter writes remain disabled for this first
row. Manual lift-table steps are available after the profile explicitly maps
the table to the controller's Z or U channel.
