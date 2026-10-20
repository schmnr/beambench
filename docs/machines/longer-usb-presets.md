# Additional LONGER USB preset evidence

- Evidence level: Documentation-based. Physical Beam Bench testing is pending.
- Sources checked: October 20, 2026.
- Scope: standard frames with GRBL 1.1-compatible firmware over USB serial.
- The [RAY5 10W record](longer-ray5-10w.md) covers the original request separately.

## Exact models and defaults

| Model | Preset ID, version 1 | Workspace X × Y | Power mode | Homing capability | Air commands |
| --- | --- | --- | --- | --- | --- |
| RAY5 5W | `longer_ray5_5w` | 400 × 400 mm | M4 | Off | None |
| RAY5 20W | `longer_ray5_20w` | 375 × 365 mm initially; see conflict below | M4 | Off | None |
| RAY5 40W | `longer_ray5_40w` | 385 × 385 mm | M3 constant power | Off | None |
| Laser B1 20W | `longer_b1_20w` | 450 × 440 mm | M4 | Available; verify switches and firmware | M8 on, M9 off |
| Laser B1 30W | `longer_b1_30w` | 450 × 440 mm | M4 | Available; verify switches and firmware | M8 on, M9 off |
| Laser B1 40W | `longer_b1_40w` | 450 × 440 mm | M4 | Available; verify switches and firmware | M8 on, M9 off |

All six use 115200 baud, front-left/bottom-left origin, initial S1000, buffered
transfer, no custom header/footer, no per-move S requirement and no G0 overscan.
The valid connected controller `$30` takes precedence over the preset scale.
Verify `$32=1`. The 6000 mm/min profile speed is a conservative Beam Bench
starting limit, not a manufacturer speed rating or a material recommendation.
Maximum power is 100% of the controller scale.

Automatic homing is off in a new profile. Applying a preset preserves an
existing owner's `home_on_connect` preference. Homing capability does not run
homing or change controller settings. B1 users should home manually as needed
for the controller's soft limits. Air commands only apply to cuts with air
assist enabled; confirm that a compatible automatic pump is connected.

## Manufacturer evidence and discrepancies

### RAY5 5W

[The RAY5 5W guide](https://wiki.longer3d.com/en/ray5-5w), sections 1.2.2,
1.2.3 and 4.1.1, documents GRBL/USB, 115200 baud, 400 × 400 mm, front-left
origin, disabled automatic homing and S1000. Limit switches and an automatic
pump are not assumed. M4 follows standard
[GRBL 1.1 laser mode](https://github.com/gnea/grbl/wiki/Grbl-v1.1-Laser-Mode).

### RAY5 20W

[The dedicated RAY5 20W guide](https://wiki.longer3d.com/en/RAY5-20W), sections
1.2.2, 1.2.3 and 4.1.1, documents the same connection, origin and power scale,
but specifies **375 × 375 mm** in its LightBurn setup. The
[manufacturer comparison](https://eu.longer.net/pages/machine-comparison) and
[current combined product specifications](https://www.longer.net/products/longer-ray5-laser-engraver-5w-10w-20w)
specify **400 × 365 mm** instead. Use the intersection, **375 × 365 mm**, as
the initial workspace. This is deliberately smaller than either claimed full
area. Owners can enter larger dimensions only after checking their exact
frame, laser head and measured usable travel. Do not use the 10W workspace.
The dedicated guide disables automatic homing. Leave homing and pump commands
off until the fitted accessories and controller configuration are established.
M4 follows GRBL 1.1 laser mode.

### RAY5 40W

[The RAY5 40W guide](https://wiki.longer3d.com/en/ray5-40w), sections 1.2.2 and
1.2.3, documents GRBL/USB, 115200 baud, 385 × 385 mm, front-left origin and
disabled automatic homing. Its linked
[manufacturer FAQ](https://drive.google.com/file/d/1UsOVG0ms-EU16BSR48r0WtLU-aLZDd6e/view),
section 8, specifies S1000 and **constant power**, so this preset uses M3.
The FAQ elsewhere mentions 390 × 385 mm; retain the smaller 385 × 385 mm
confirmed by the setup guide and product specification. No automatic pump
commands are inferred from related models or from a LightBurn radio button.

### Laser B1 20W, 30W and 40W

The separate [B1 20W](https://wiki.longer3d.com/en/b1-20w),
[B1 30W](https://wiki.longer3d.com/en/b1-30w) and
[B1 40W](https://wiki.longer3d.com/en/b1-40w) guides, sections 1.2.2 and 1.2.3,
document USB, 115200 baud and S1000. Their LightBurn screenshots establish
GRBL, front-left origin and buffered transfer. The 20W/30W guide includes a
[manufacturer manual](https://drive.google.com/file/d/1_9gbvIXqSkmWBG3Fwjc717OH40TB-Pv_/view),
printed pages 23–25, with the same setup. LONGER requests DTR; Beam Bench's
existing GRBL serial transport toggles DTR on opening and then asserts it.

All three guides' section 2.2 links the same
[controller configuration](https://drive.google.com/file/d/1yIJmB0o0EbtZdxNmcAtwTnEp-o_NzyDH/view):
`$30=1000`, `$32=1`, `$22=1`, `$23=3`, `$130=450`, `$131=440`. Extract these
facts only; do not copy or send the configuration to a user's controller.
Standard GRBL 1.1 laser mode supports M4. Section 4.1's air-pump troubleshooting
explicitly specifies M8/M9 for each model. Homing is available for the documented
limit-switch configuration, with no automatic home enabled by the preset.

[LONGER's comparison](https://eu.longer.net/pages/machine-comparison) specifies
450 × 440 mm for all three models. Some setup screenshots disagree: the
20W/30W settings image shows 460 × 440 mm, while the 40W import summary shows
450 × 450 mm. Use **450 × 440 mm**, consistent with the controller travel and
the product specifications and within both screenshot areas. The 40W import
summary also enables startup homing while the reused settings image disables
it; retain Beam Bench's default of no automatic homing.

## Scope and validation

Select these presets manually. Generic GRBL, MKS, CH340 or a wattage string
does not identify an exact machine. Applying a preset configures Beam Bench;
it does not flash firmware, rewrite controller settings or operate the laser.
Changed boards/firmware, extension frames, rotary, infrared replacement modules,
Wi-Fi upload, offline SD, touchscreens and cameras need separate verification.

Backend regressions apply every preset, synchronize the project's workspace,
and run a rectangle through the real planner and GRBL emitter. They check
coordinates, S250 at 25%, M3/M4 and M5, B1 homing capability without emitted
homing, B1 M8/M9 only when a cut requests air, and no RAY5 air commands.
Connected-controller tests preserve `$30=255` through preview and apply for
each entry. Catalog checks validate unique IDs and settings. UI tests preview
and apply every entry with confirmation. Guidance is localized in all 23
supported languages.

No physical machine was connected or operated. Connect/reconnect, jog, frame,
vector/raster output, pause/resume, cancel and the B1 accessories still need
[owner validation](../machine-presets.md#owner-validation).

## Other LONGER candidates

[The RAY5 mini guide](https://wiki.longer3d.com/en/RAY5-mini) uses an imported
vendor profile with console macros and contains differing constant-power
instructions. Mini and miniS wattage/firmware revisions need exact profile,
workspace and command review before adding entries.

[The Nano guide](https://wiki.longer3d.com/en/nano-6w) requires a switch to
carving mode and specific raster intervals, and gives an unusual serial baud
value. Nano, Nano Pro and Nano Duo need their vendor profiles, mode commands
and galvo behavior verified against Beam Bench's output and streaming. Their
LightBurn compatibility alone does not establish support here. No Nano preset
is added by this work.
