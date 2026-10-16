# Material recipes and LightBurn import

A material preset can hold a complete, ordered recipe of up to 11 sub-layers.
**Material Library → From Layer** captures the whole selected layer, including
operations, minimum and maximum power, air assist, Z offset, custom G-code,
output states, and all vector/raster settings. Applying a recipe restores its
complete stack in order. The operation is undoable through the existing
project undo system. Presets store templates without entry IDs; each application
creates fresh sub-layer IDs.

Library edits to name, material, thickness, and notes retain the recipe. The
basic operation/speed/power/pass controls edit the first sub-layer while
retaining compatible settings and the remaining sub-layers. Changing its
operation resets settings that no longer apply to that operation and creates
the required settings with defaults. Apply a recipe to a layer to edit its
advanced settings, then use From Layer to save that recipe.

Existing JSON presets without an `entries` array keep their previous partial
update behavior: they update common settings on the target layer's first
sub-layer and leave its other settings and sub-layers alone. They continue to
load, import, and export. New complete recipes include `entries` as well as
legacy summary fields for the first entry. Quality-test dialogs use only the
first recipe entry and warn when a recipe has additional sub-layers.

## Import a LightBurn library

Open **Material Library → Import** and select a `.clb` file. Beam Bench shows
converted recipes and skipped entries before saving. Expand a recipe to review
all its sub-layers, including minimum power, air assist, Z offset, kerf, and
output states. Choose **Import** to append the reviewed recipes, or **Cancel**
to leave your library unchanged. Imported presets receive new IDs.

Each LightBurn material Entry becomes one preset, with its base settings and
SubLayer settings kept together in source order. Disabled sub-layers are
retained with output disabled. A disabled parent layer disables every entry.
If any sub-layer is invalid or unsupported, the whole recipe is reported as
skipped; the importer does not save an incomplete sequence.

Saving uses the same persistence path as JSON import. A failed write leaves
the in-memory library unchanged and the review open for retry. The save commits
exactly the reviewed data without rereading the source file.

## Conversion

| LightBurn source | Beam Bench recipe |
| --- | --- |
| Material name, thickness/title, description | Material, thickness, and preset name |
| No Thickness (`Thickness=-1`) | 0 mm, with original metadata retained in notes |
| `Cut`, `Scan`, `Offset` / `OffsetFill`, `Image` | Line, Fill, Offset Fill, Image |
| `speed` in mm/s | Speed in mm/min, multiplied by 60 |
| `minPower`, `maxPower`, `numPasses` | Minimum/maximum power and passes |
| `runBlower`, `zOffset`, `kerf` | Air assist, Z offset, kerf offset |
| Layer `output`, per-entry `suboutput` | Output enabled state |
| Fill/Image/Offset Fill `interval`, `angle`, `biDirectional`, `floodFill` | Line interval, scan angle, bidirectional scanning, flood fill |
| `crossHatch` | Crosshatch, two angle passes 90 degrees apart |
| `overscan=0` | Overscan distance 0 mm |
| Image `ditherMode`, `negative`, `passThrough` | Raster mode, inversion, pass-through |
| Vector `perforate`, `perfLen`, `perfSkip` | Perforation enabled; source distances converted to on/off milliseconds at the imported speed |

Supported Image modes are Threshold, Ordered, Atkinson, Dither
(Floyd-Steinberg), Stucki, Jarvis, Newsprint, Halftone, Sketch, and Grayscale.
The algorithms in the two applications can produce different results.
Unknown operations and Image modes, including 3D Slice, are skipped.

LightBurn omits values equal to its built-in defaults. Missing values use
100 mm/s, 20% minimum/maximum power, one pass, air assist on, 0 mm Z/kerf,
0.1 mm interval, 0-degree scan angle, bidirectional on, crosshatch/flood fill
off, and Stucki Image mode. Invalid explicit values are skipped rather than
clamped or replaced with defaults.

## Kerf compensation

Kerf Offset is a normal per-sub-layer setting in the layer's vector controls.
Material recipes save and restore it, and LightBurn imports preserve the signed
offset distance without doubling or halving it. The value is the path offset,
usually half the measured width of material removed by a cut.

Closed Line contours, including legacy Cut/Score operations, are compensated
when generating the execution plan. A positive offset expands outer contours
and contracts holes; negative offsets reverse those directions. Further nested
islands alternate again. Nesting is evaluated across objects on the same
sub-layer and does not depend on path winding or the optimization switches.
Open paths and raster/Offset Fill operations are unchanged.

Compensation changes the execution plan, not the artwork. Preview and machine
output use the same compensated paths. Ordering, tab gaps, travel moves, job
bounds, and duration use the resulting geometry. Zero offset preserves the
previous planning behavior. Existing artwork-based job-origin placement is
preserved, so compensation may extend the output beyond the artwork bounds.

Offsets that collapse or split a contour stop planning with an error instead
of silently dropping a cut or relocating its tabs onto multiple contours.
Repair the contour or reduce the offset. Verify the preview and calibrate the
offset for the machine, material, and cutting settings before making fitted parts.

## Remaining conversion limits

Enabled percentage overscan cannot be translated to a fixed distance without
machine acceleration. Imported recipes use Beam Bench's default 2.5 mm distance
in that case; review it for your machine. Disabled overscan maps to 0 mm.

Laser selection, frequency, pulse width, controller timing, Z steps per pass,
manual/spacing-based tabs, global repeats, fill grouping and continuous angle
increments are not automatically converted. Full native Beam Bench presets do
preserve the vector/raster settings that Beam Bench itself represents, including
tabs and grouping; these source-specific conversions still need review.

The original LightBurn Entry XML stays in the preset notes, including unknown
settings, and remains in JSON exports. Notes are text and are never executed.
The importer does not infer a laser model, rescale power for another laser,
or connect to a machine.

## Format evidence and validation

The conversion follows LightBurn's [material-library hierarchy](https://docs.lightburnsoftware.com/latest/Reference/MaterialLibrary/),
[built-in defaults](https://docs.lightburnsoftware.com/2.2/Reference/CutSettingsEditor/WorkingWithDefaults/),
[developer's parameter descriptions](https://forum.lightburnsoftware.com/t/looking-for-a-definitive-list-of-parameters-stored-in-a-clb-file/191415/2),
and [Line mode distance/kerf semantics](https://docs.lightburnsoftware.com/2.1/Reference/CutSettingsEditor/LineMode/).
The XML format has no published schema. Tests use small self-authored examples.
The structure is also checked against these public libraries, which are not
bundled with Beam Bench or endorsed as settings for another laser:

- [TTL Nova 24 60W](https://code.ttlmakerspace.com/TTL/DesignForLasers/src/commit/f18e5f062c123a07e008a1855bbdc13f6f894d9f/resources/TTL-Nova-24-60w.clb).
- [Big Blue Laser](https://github.com/Jayson-Q/Makerspace-Projects/blob/ad7f9d36d6c9677a8a36c737123a111e9709b3be/Device%20Configs/Lightburn/Big%20Blue%20Laser.clb), including a sub-layer.
- [FabLab Karlsruhe](https://github.com/fablab-ka/OMtech-Max-1060/blob/139a9c4a160079e3ccc9c31c2d5c54175d322e02/LightBurn/Fablab-Karlsruhe.clb), including Image settings.

Input is limited to 16 MiB, 500,000 XML nodes, 10,000 material entries and
11 sub-layers per recipe. DTDs and external entities are rejected. UTF-8 files
with or without a BOM are accepted.
