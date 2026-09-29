# MGL LightBurn import investigation

**Update, 28 September:** the asymmetric native LightBurn reference check in
[the follow-up review](2026-09-28-generated-reproductions.md) reproduces a bitmap
Y-inversion bug. The earlier orientation conclusion below is superseded.

BeamBench was losing the per-image power settings in a generated MGL calibration grid. The local fix retains those settings through import, planning, and G-code export. The reported upside-down import was not reproduced with this sample.

## Reproduction

Generated a real `.lbrn` file using [MGL 2.2.6 Online](https://kpc29laser.fr/MGL/MGL.html), in Powerscale / Image / Oeil1 / Jarvis mode, with a 6 × 6 grid and 243 DPI. The downloaded file uses the generator's default 2,000–5,000 mm/min speeds and 10–80% power range. These differ from the customer's screenshot; this is a representative sample, not the customer's original file.

Sample filename: `grille_test_VP_Lbrn_V2000-5000-P10-80-243Dpi__(Kpc29Laser)_19-9-2026_15-48-33.lbrn`.

SHA-256: `d576c8b13faf1338efe654c61ca9a23c8d9b5c82b184df5deb0d696bb3cb7433`.

Opened that same file in native LightBurn 1.6.03 and exported GRBL G-code. Imported it through BeamBench's actual service importer, built execution plans, and generated GRBL commands in isolated project contexts for both top-left and bottom-left workspace origins. BeamBench was tested from the local v0.2.24 branch based on `e065f29d`, including the pending raster memory changes. The production LightBurn import code is unchanged by those changes. No physical laser test was performed.

## Reproduced defect

The file assigns one speed layer to each row, with layer maximum power set to 100%. Each bitmap carries a `PowerScale` attribute to specify its column's power. This is consistent with LightBurn's documented [per-shape Power Scale](https://docs.lightburnsoftware.com/2.0/Reference/ShapeProperties/) behavior.

| Image column | Source PowerScale | LightBurn S value | BeamBench S value |
| --- | ---: | ---: | ---: |
| 1 | 10% | 100 | 1000 |
| 2 | 24% | 240 | 1000 |
| 3 | 38% | 380 | 1000 |
| 4 | 52% | 520 | 1000 |
| 5 | 66% | 660 | 1000 |
| 6 | 80% | 800 | 1000 |

Both exports use a maximum S value of 1000. Both also contain S500 for the separate 50% text layer and S0 for laser-off moves. Before the fix, BeamBench imported all 36 images with `power_scale = 1.0` and planned all 36 image segments at 100% maximum power. Row speeds remained distinct. The importer/planner gave no warning about losing the column settings.

Two code paths caused the defect:

1. `crates/beambench-core/src/import_lbrn.rs`: `LbrnShape::Bitmap` and its parser did not retain `PowerScale`. The service importer consequently left `ProjectObject.power_scale` at its default 1.0.
2. `crates/beambench-planner/src/builder.rs`: the image branches used layer maximum/minimum power directly, without applying the object's power scale. This affected both cardinal and arbitrary-angle image passes. Importing the attribute alone would not correct output.

## Fix and reference checks

The bitmap parser now normalizes `PowerScale` from 0–100 to 0–1 and defaults to 1 when absent. The service importer retains it on the object. Image planning applies it to each image and angle pass after pixel processing, so cached images can still share their pixels while using different output powers.

Dithered images scale the firing power directly. Grayscale images retain the layer minimum and scale the range above that minimum. At full scale, existing layer powers are unchanged.

A separate synthetic file was opened in native LightBurn 1.6.03 with minimum 20%, maximum 80%, and image scales of 50% and 100%. Its exported GRBL commands confirmed:

- Threshold mode at 50% scale uses S400, versus S800 at full scale.
- Grayscale mode at 50% scale has a 20–50% range, using S200 through S500; full scale retains the 20–80% range.

The synthetic test artwork contains authored black, gray, and white stripes. Native output is saved locally as `/tmp/beambench-power-scale-reference.gc`. No hardware was operated.

The service regression imports synthetic calibration images and checks both planned power ranges and the commands from prepared export. It covers MGL column powers, zero/full scale, nonzero minimum power, 0/45/90-degree scan angles, and rotated image objects. Parser tests also cover omitted scale, bounds, both project format versions, and different power scales on images that reuse source data.

The real MGL sample now retains all six column scales on all 36 images. Under both top-left and bottom-left workspace origins, the resulting GRBL S-value set matches native LightBurn exactly: `0, 100, 240, 380, 500, 520, 660, 800`. S500 belongs to the text layer. Decoded pixels and upright image transforms are unchanged. This comparison checks power levels, not byte-for-byte toolpath equivalence.

Validation completed:

- Core, planner, service, and GRBL regression suites: 2,361 passed, 2 skipped. Nextest also flagged `discovery_candidate_connect_rejects_unknown_identity` as leaky; the suite exited successfully.
- The final import-to-export regression, including rotated image objects, passed separately after its last test-case addition.
- Clippy completed for core, planner, and service with all targets. Existing warnings remain; none point to the new power-scaling code or regression test.
- Formatting checks for the three changed source files and `git diff --check` passed.

Projects already saved after the old importer discarded their scales must be re-imported from the original LightBurn file. The missing values cannot be recovered from the saved BeamBench project.

## Orientation: not reproduced

The generated file has `MirrorX="False"` and `MirrorY="false"`. Its bitmaps have identity linear transforms and differ only in position. They appear upright in LightBurn.

Under both tested BeamBench workspace origins:

- All 36 imported image linear transforms are identity, with no reflection or rotation.
- Decoded source image pixels match the imported grayscale assets. PNG encoding changes, but the pixel comparison confirms that image content is not flipped during import.
- The fastest row appears above the slowest row, matching LightBurn.
- The importer converts LightBurn's Y-up positions to BeamBench's Y-down canvas coordinates. It compensates for bitmap-local coordinates in `lbrn_semantic_object_transform`, so the coordinate conversion does not itself invert image content.

This does not establish what happened to the customer's file. Their original `.lbrn`/`.lbrn2`, BeamBench version, and clarification of whether the whole grid or individual images needed flipping are still needed to reproduce that report. Other source transforms and mirror flags have not been compared against native LightBurn in this investigation.

## Diagnostic command

The diagnostic example imports into memory without opening or replacing an app project or connecting to hardware:

```sh
cargo run -p beambench-service --example lbrn_import_probe -- /path/to/grid.lbrn
```

It prints object transforms, positions, power scales, planned powers, and distinct GRBL S values for both workspace origins. Its decoded-pixel comparison is intended for MGL's top-level opaque grayscale bitmaps. The sample file and exported G-code are local investigation artifacts and are not included in the repository.
