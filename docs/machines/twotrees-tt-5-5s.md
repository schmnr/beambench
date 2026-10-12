# Two Trees TT-5.5S preset evidence

- Preset: `twotrees_tt_5_5s`, version 1.
- Scope: stock TT-5.5S / Totem S frame with the original USB GRBL controller.
- Evidence level: Documentation-based. Physical Beam Bench testing is pending.
- Sources checked: 2026-10-11. Requested in feedback issue #53.
- Excluded: TTS-55, TTS-25, TT-2.5, TT-50, TS models, replacement controllers,
  extension frames, added homing switches, rotary and wireless accessories.

## Sources and defaults

The [manufacturer comparison](https://twotrees3d.com/pages/compare-tts-55-tts-25-tt-2-5-5-5)
distinguishes the TT-5.5S from the newer TTS-55: the TT model has a 300 by 300 mm
area and an MKS LKS 8-bit USB controller. The
[manufacturer help center](https://twotrees3d.com/pages/help-center-1) lists the
exact TT model. Its old WordPress English PDF link returns 404, but its
[Totem assembly instructions download](https://www.mediafire.com/file_premium/iipg8y5v82iyl54/Totem_Assembly_Instructions.pdf/file)
is accessible. The retrieved PDF has 28 pages; printed pages 22 and 24 are PDF
pages 23 and 25. These establish serial baud, the maximum power value and M3.

| Setting | Default | Basis |
| --- | --- | --- |
| Workspace | X 300 mm, Y 300 mm | Exact TT-5.5S manufacturer comparison |
| Connection | GRBL, USB serial, 115200 baud | Original USB controller and manual, printed page 22, section 5.2 |
| Origin | Bottom left | Beam Bench GRBL starting convention; physical axis orientation remains untested |
| Power scale | S1000 represents 100% initially | Manual, printed page 24, section 8.2. The actual controller's `$30` takes precedence on connection and when applying a preset to its active profile. |
| Power mode | Constant M3 | M3 is documented in section 8.2. No M4 support is assumed for the original controller. |
| Homing | Off initially | No configured stock homing cycle established; applying the preset does not enable homing or emit `$H` |
| Automatic air commands | None | No automatic air interface established for the stock model |
| Maximum profile speed | 3000 mm/min | Beam Bench starting value, not a manufacturer-rated maximum. Controller motion limits still apply. |
| Maximum power percentage | 100 | Controller scale, not a material-processing recommendation |
| Transfer and custom commands | Buffered, no header or footer | Existing GRBL adapter; no required custom startup commands established |

The manual's power troubleshooting example is evidence for the scale and mode,
not a command that Beam Bench sends during preset selection or connection.
Automatic suggestion requires Two Trees/Twotrees and an explicit TT-5.5S name.
TTS-55 and generic GRBL or CH340 identities do not match this preset.

## Validation

Software tests apply the preset, synchronize its bound project workspace, and
generate a rectangle through the real planner and GRBL emitter. They check
bottom-left placement, S250 at 25% of the starting scale, M3/M5, no air commands,
and no homing command. Another regression retains a connected controller's
`$30=255` in both preset preview and application, leaves inactive profiles on the
documented default, and ignores invalid reported power limits. Catalog and
suggestion tests distinguish TT from TTS models. Guidance is localized in all
23 supported languages.

See [release validation](../testing/feedback-fixes-2026-10-11.md) for results.
No physical machine was connected or operated. Connection, motion, framing,
vector/raster output, pause/resume and cancellation remain unverified on this
model. A future submitted machine report can expand the evidence level without
requiring correspondence with the original requester.
