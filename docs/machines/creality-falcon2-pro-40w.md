# Creality Falcon2 Pro 40W preset evidence

- Preset: `creality_falcon2_pro_40w`, version 1.
- Scope: stock Falcon2 Pro 40W using the USB GRBL 1.1 connection.
- Evidence level: Documentation-based. Physical Beam Bench testing is pending.
- Sources checked: 2026-10-11. Requested in feedback issue #52.
- Excluded: Falcon2 without Pro, Pro S, 22W and 60W variants, replacement
  controllers, changed modules, rotary accessories and offline TF-card operation.

## Sources and defaults

The [manufacturer download page](https://www.crealityfalcon.com/pages/download)
lists a Falcon2 Pro manual. The [manufacturer's English manual](https://cdn.creality.com/ow/official/ab140496-6ba2-4e3d-95f2-174bdc184b50.pdf),
printed page 22, gives the engraving area and names LightBurn and LaserGRBL.
The USB online workflow appears on printed page 25. The manual does not establish
the exact controller settings on the requesting owner's machine.

| Setting | Default | Basis |
| --- | --- | --- |
| Workspace | X 400 mm, Y 415 mm | Exact Pro model manufacturer specification, printed page 22 |
| Connection | GRBL, USB serial, 115200 baud | Manufacturer USB/LaserGRBL workflow; baud is the standard [GRBL 1.1 starting setting](https://github.com/gnea/grbl/blob/master/grbl/config.h), pending device confirmation |
| Origin | Bottom left | Beam Bench GRBL starting convention; physical axis orientation remains untested |
| Power scale | S1000 represents 100% initially | [GRBL default spindle maximum](https://github.com/gnea/grbl/blob/master/grbl/defaults.h); not established by the Pro manual. The actual controller's `$30` takes precedence on connection and when applying a preset to its active profile. |
| Power mode | Dynamic M4 | Scope requires GRBL 1.1 [laser mode](https://github.com/gnea/grbl/wiki/Grbl-v1.1-Laser-Mode); controller configuration and physical behavior remain untested |
| Homing | Off initially | Beam Bench setup choice; do not infer an enabled homing cycle solely from supplied switches. Applying this preset does not write `$22` or run homing. |
| Automatic air commands | None | The manual describes integrated air equipment but does not establish its host G-code interface. No M8/M9 commands are guessed. |
| Maximum profile speed | 6000 mm/min | Beam Bench starting value, not a manufacturer-rated maximum. Controller motion limits still apply. |
| Maximum power percentage | 100 | Controller scale, not a material-processing recommendation |
| Transfer and custom commands | Buffered, no header or footer | Existing GRBL adapter; no model-specific startup commands established |

The preset supplies configuration for the existing adapter. It does not implement
enclosure controls, camera integration, vendor USB protocols, offline jobs or
automatic pump control. The first connection should establish the firmware and
actual power scale before generating machine output. No settings are written to
the controller by applying the preset.

Automatic suggestion requires both Creality and the explicit Falcon2 Pro 40W
model string. Generic GRBL and USB chip identities cannot select this model.

## Validation

Software tests apply the preset, synchronize its bound project workspace, and
generate a rectangle through the real planner and GRBL emitter. They check
bottom-left placement, S250 at 25% of the starting scale, M4/M5, no air commands,
and no homing command. Another regression previews and applies this preset with
a connected controller reporting `$30=255`, retaining 255 instead of replacing
it with 1000. Catalog and suggestion tests reject ambiguous and other Falcon
variants. Guidance is localized in all 23 supported languages.

See [release validation](../testing/feedback-fixes-2026-10-11.md) for results.
No physical machine was connected or operated. Connection, motion, framing,
vector/raster output, pause/resume and cancellation remain unverified on this
model. A future submitted machine report can expand the evidence level without
requiring correspondence with the original requester.
