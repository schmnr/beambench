# Import a LightBurn material library

Open **Material Library → Import** and select a `.clb` file. Beam Bench shows
the converted presets and any skipped entries before saving. Expand a preset
to inspect its converted speed, maximum power, passes, and source notes.
Choose **Import** to append the reviewed presets, or **Cancel** to leave your
library unchanged. JSON import and export continue to work as before.

Imported presets receive new IDs and do not overwrite existing presets.
Saving uses the same persistence path as JSON import: a failed write leaves
the in-memory library unchanged and the review open for retry. The import
saves the presets shown in the review, without rereading the source file.

## Conversion

| LightBurn source | Beam Bench preset |
| --- | --- |
| Material name, thickness/title, description | Material, thickness, and preset name |
| `Thickness="-1"` / No Thickness | 0 mm; original title and thickness retained in notes |
| `Cut`, `Scan`, `Offset` / `OffsetFill`, `Image` | Line, Fill, Offset Fill, Image |
| `speed` in mm/s | Speed in mm/min, multiplied by 60 |
| `maxPower`, `numPasses` | Maximum power percentage, passes |
| Fill/Image `interval`, `angle`, `biDirectional`, `floodFill` | Line interval, scan angle, bidirectional scanning, flood fill |
| Fill/Image `crossHatch` | Two angle passes, 90 degrees apart |
| Fill/Image `overscan=0` | Overscan distance 0 mm |
| Image `ditherMode` | Supported Beam Bench raster mode |
| Image `negative`, `passThrough` | Inversion, pass-through |

Image modes supported are Threshold, Ordered, Atkinson, Dither (Floyd–Steinberg),
Stucki, Jarvis, Newsprint, Halftone, Sketch, and Grayscale. The algorithms in
the two applications can produce different results. Unknown modes, including
3D Slice, are skipped rather than substituted.

LightBurn omits settings equal to its built-in defaults. Missing values use
100 mm/s speed, 20% maximum power, one pass, 0.1 mm interval, 0-degree scan
angle, bidirectional scanning on, crosshatch/flood fill off, and Stucki image
mode. Invalid explicit values are reported as skipped, never clamped or
replaced with defaults.

## Settings to review manually

Beam Bench material presets do not represent all LightBurn layer settings.
The original entry attributes and cut-setting XML are kept in each imported
preset's notes, including settings that were converted. Notes remain available
in JSON exports. They are displayed as text and never executed.

Minimum power, air assist, Z offsets/steps, kerf, tabs, perforation, laser
selection, frequency, pulse width, timing, global repeats, fill grouping,
angle increments, and other advanced settings require manual transfer where
supported. Omitted source defaults also matter: minimum power is 20%, air
assist is on, and overscan is on at 2.5%. Percentage overscan cannot be
converted to a fixed distance without machine acceleration, so enabled
overscan leaves the target layer's distance unchanged. Offset Fill imports
the common speed/power/pass values; its remaining source settings stay in notes.

Sub-layers become separately numbered presets in source order. Applying one
does not create or run the other sub-layers. Disabled layers and sub-layers,
unsupported operations, and invalid entries appear in the skipped list.
Preset application continues to use the selected layer's operation; select
the matching operation and review your laser's settings before running a job.
The importer does not infer a laser model, rescale power for another laser,
or connect to a machine.

## Format evidence and validation

The conversion follows LightBurn's [material-library hierarchy](https://docs.lightburnsoftware.com/latest/Reference/MaterialLibrary/),
[built-in defaults](https://docs.lightburnsoftware.com/2.2/Reference/CutSettingsEditor/WorkingWithDefaults/),
and the [developer's settings descriptions and speed units](https://forum.lightburnsoftware.com/t/looking-for-a-definitive-list-of-parameters-stored-in-a-clb-file/191415/2).
The format has no published XML schema. Tests use small, self-authored XML
examples, with the structure checked against these public libraries:

- [TTL Nova 24 60W](https://code.ttlmakerspace.com/TTL/DesignForLasers/src/commit/f18e5f062c123a07e008a1855bbdc13f6f894d9f/resources/TTL-Nova-24-60w.clb): 87 presets.
- [Big Blue Laser](https://github.com/Jayson-Q/Makerspace-Projects/blob/ad7f9d36d6c9677a8a36c737123a111e9709b3be/Device%20Configs/Lightburn/Big%20Blue%20Laser.clb): 14 presets including a sub-layer.
- [FabLab Karlsruhe](https://github.com/fablab-ka/OMtech-Max-1060/blob/139a9c4a160079e3ccc9c31c2d5c54175d322e02/LightBurn/Fablab-Karlsruhe.clb): 59 presets including Image settings.

Those files are used for local format checks and are not bundled with Beam
Bench or endorsed as settings for another machine. Input is limited to 16 MiB,
500,000 XML nodes, and 10,000 resulting entries. DTDs and external entities
are rejected. UTF-8 files with or without a BOM are accepted.
