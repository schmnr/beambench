# LightBurn curve fixtures

`smooth-circle.lbrn` and `smooth-circle.lbrn2` are the same 155 mm circle,
drawn in LightBurn 1.6.03 on macOS on 2026-09-09, converted with Edit >
Convert to Path, and saved in both formats. The root attributes and shape
are unchanged from those native saves. Unrelated settings and thumbnails
were removed. These are original test drawings, not customer artwork.

The compact file uses an `S` suffix for each smooth vertex. The legacy file
stores the same flag separately as `sm="1"`. Importing either file must
produce identical closed cubic paths with all eight control handles intact.
