# Longer RAY5 10W preset evidence

- Preset: `longer_ray5_10w`, version 1.
- Scope: the RAY5 10W with GRBL 1.1 firmware over USB serial.
- Evidence level: Documentation-based. Physical Beam Bench testing is pending.
- Sources checked: October 20, 2026.
- Request: an owner identified GRBL 1.1h on an MKS DLC32 V2.1 board over USB.
  This is the requested configuration, not independent verification of that
  board or a claim that every RAY5 revision uses it.
- Excluded: other wattages, proprietary or changed firmware, Wi-Fi, offline SD
  and touchscreen workflows, extension frames, rotary and automatic air pumps.

## Sources and defaults

[LONGER's RAY5 10W guide](https://wiki.longer3d.com/en/ray5-10w-20w), sections
1.2.2 and 1.2.3, provides the serial and LightBurn setup. Section 4.1.1 gives the
power scale. The [manufacturer download page](https://www.longer.net/pages/download-ray5-10w)
lists distinct firmware and limit-switch upgrades, so those features cannot be
assumed from the model name alone. The guide's URL also mentions 20W, but its
page title and the setup instructions cited here identify the RAY5 10W.

| Setting | Default | Basis |
| --- | --- | --- |
| Workspace | X 400 mm, Y 400 mm | LONGER's LightBurn setup instructions |
| Connection | GRBL, USB serial, 115200 baud | LONGER's serial setup instructions |
| Origin | Bottom left, front left on the frame | LONGER's LightBurn setup instructions |
| Power scale | S1000 represents 100% initially | LONGER's power troubleshooting instructions. The connected controller's valid `$30` takes precedence. |
| Power mode | Dynamic M4 | The requested GRBL 1.1h configuration and standard [GRBL 1.1 laser mode](https://github.com/gnea/grbl/wiki/Grbl-v1.1-Laser-Mode). Verify `$32=1` on the controller. |
| Homing | Off initially, no home on connect | LONGER disables automatic homing in its setup instructions. Switch upgrades need their own verified configuration. |
| Automatic air commands | None | No pump command established for the requested hardware. Do not infer M8/M9 support solely from the board name. |
| Maximum profile speed | 6000 mm/min | Conservative Beam Bench starting limit, not a rated processing speed. Controller limits still apply. |
| Maximum power percentage | 100 | Controller scale, not a material recommendation |
| Transfer and custom commands | Buffered, no header or footer | Existing GRBL adapter; no model-specific startup commands established |

Applying this preset configures Beam Bench. It does not flash firmware, write
controller settings or run homing. It does not implement the vendor's offline
interface or establish complete accessory support. Select the preset manually;
a generic GRBL banner or MKS board identity does not identify this exact model.

## Validation

Shared software regressions apply the preset, synchronize the bound project's
workspace and generate a rectangle through the real planner and GRBL emitter.
They check bottom-left coordinates, S250 at 25%, M4/M5, and absence of air and
homing commands. A connected-controller regression retains `$30=255` when
previewing and applying the preset. Catalog checks validate the settings and
unique ID. The UI regression previews and applies the preset with confirmation
and displays its evidence and setup guidance. New copy is localized in all
23 supported languages.

No physical machine was connected or operated. Connect/reconnect, jog, frame,
vector/raster output, pause/resume and cancel still need an owner test report
before expanding the evidence level. Users can submit one through **Share
machine test results** after saving and activating this profile. See
[the preset validation process](../machine-presets.md#owner-validation).
