# Creality Falcon2 Pro 22W preset evidence

- Preset: `creality_falcon2_pro_22w`, version 1.
- Scope: stock Falcon2 Pro 22W with its original module and USB GRBL 1.1 controller.
- Evidence level: Documentation-based. Physical Beam Bench testing is pending.
- Sources checked: 2026-10-14.
- Excluded: Falcon2 without Pro, Pro S, other wattage models, replacement
  controllers or modules, rotary accessories and offline TF-card operation.

## Sources and defaults

The [manufacturer product specification table](https://www.crealityfalcon.com/products/falcon-2-pro-40w-enclosed-laser-engraver-and-cutter)
and [manufacturer device profile](https://www.crealityfalcon.com/cdn/shop/files/Creality_Falcon2_Pro_22W_40W_Lightburn_Device_File.lbdev?v=18063900233994644338)
establish this model's workspace and GRBL serial interface. The downloaded
profile's SHA-256 is `8324169e3396e1311482f9ee5596cfee76098cc84fddf95c07c3988953240f34`.
Only factual settings are used; the third-party profile is not redistributed.

The manufacturer's shared 22W/40W device profile explicitly gives `Width=400`,
`Height=415`, `Name=GRBL`, `Type=Serial`, `BaudRate=115200` and `S_Scale=1000`.
Its automatic homing preference is not copied: Beam Bench leaves homing off
until the owner checks the machine and firmware configuration.

| Setting | Default | Basis |
| --- | --- | --- |
| Workspace | X 400 mm, Y 415 mm | Model-specific manufacturer product table and device profile |
| Connection | GRBL, USB serial, 115200 baud | Manufacturer device profile |
| Origin | Bottom left | Beam Bench GRBL starting convention; physical axis orientation remains untested |
| Power scale | S1000 represents 100% initially | Manufacturer device profile; the connected controller's valid `$30` takes precedence |
| Power mode | Dynamic M4 | [GRBL 1.1 laser mode](https://github.com/gnea/grbl/wiki/Grbl-v1.1-Laser-Mode); firmware configuration and physical output remain untested |
| Homing | Off initially | Explicit Beam Bench setup choice; applying the preset does not write `$22`, home the machine or enable automatic homing |
| Automatic air commands | None | No verified host G-code interface for the integrated pump; no M8/M9 commands are guessed |
| Maximum profile speed | 6000 mm/min | Beam Bench starting value, not a manufacturer-rated maximum; controller limits still apply |
| Maximum power percentage | 100 | Controller scale, not a material-processing recommendation |
| Transfer and custom commands | Buffered, no header or footer | Existing GRBL adapter; no additional model-specific commands established |

Optical wattage does not set the controller's S-value scale. Connect and check
the detected scale before generating output. Applying the preset writes no
controller firmware settings. It does not configure vendor camera calibration,
enclosure controls, beam-mode switching, automatic pump control or offline jobs.

## Validation

Software regressions apply this preset to a bound project and generate a
rectangle through the real planner and GRBL emitter. They check the exact
workspace, bottom-left placement, S250 at 25% of the starting scale, M4/M5,
and absence of air and homing commands. A connected-controller regression
checks that preset preview and application retain a detected `$30=255`.
Exact-model suggestions reject Pro S, non-Pro, unknown wattage, brandless and
ambiguous identities. The preset description is localized in all 23 languages.
The profile dialog tests cover preview, explicit application and resulting size.

No physical machine was connected or operated. Connection, jogging, framing,
vector/raster output, pause/resume and cancellation remain unverified on this
model. This preset can ship at the documentation-based evidence level described
in [the catalog policy](../machine-presets.md).
