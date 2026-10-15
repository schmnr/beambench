# Creality Falcon2 Pro 60W preset evidence

- Preset: `creality_falcon2_pro_60w`, version 1.
- Scope: stock Falcon2 Pro 60W with its original module and USB GRBL 1.1 controller.
- Evidence level: Documentation-based. Physical Beam Bench testing is pending.
- Sources checked: 2026-10-14.
- Excluded: Falcon2 without Pro, Pro S, other wattage models, replacement
  controllers or modules, rotary accessories and offline TF-card operation.

## Sources and defaults

The [manufacturer product specification table](https://www.crealityfalcon.com/products/falcon-2-pro-40w-enclosed-laser-engraver-and-cutter)
and [manufacturer device profile](https://www.crealityfalcon.com/cdn/shop/files/Creality_Falcon2_Pro_60W_Lightburn_Device_File.lbdev?v=14457500633982009042)
establish this model's workspace and GRBL serial interface. The downloaded
profile's SHA-256 is `11986a42d050bade0cd12479dfbd913aae783a626984355390a77af774530352`.
Only factual settings are used; the third-party profile is not redistributed.

The manufacturer's 60W device profile explicitly gives `Width=400`,
`Height=400`, `Name=GRBL`, `Type=Serial` and `HomeOnStartup=false`. It agrees
with the model-specific product table. Do not use the 415 mm Y travel of the
22W/40W models. This file omits baud and power scale; the preset uses the GRBL
starting values below, and a valid detected controller `$30` takes precedence.

| Setting | Default | Basis |
| --- | --- | --- |
| Workspace | X 400 mm, Y 400 mm | Model-specific manufacturer product table and device profile |
| Connection | GRBL, USB serial, 115200 baud | [GRBL 1.1 starting setting](https://github.com/gnea/grbl/blob/master/grbl/config.h); confirm on the actual controller |
| Origin | Bottom left | Beam Bench GRBL starting convention; physical axis orientation remains untested |
| Power scale | S1000 represents 100% initially | [GRBL default spindle maximum](https://github.com/gnea/grbl/blob/master/grbl/defaults.h); the connected controller's valid `$30` takes precedence |
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
