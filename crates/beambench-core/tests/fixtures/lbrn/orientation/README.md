# LightBurn bitmap orientation reference

Generated and checked on 28 September 2026 with native LightBurn 1.6.03.
No controller was operated. `00.lbrn2` is synthetic, with a 4 x 3 PNG:

```text
black white white white
black black white white
black black black white
```

The bitmap is 40 x 30 mm, centered at source coordinates 60,80, with an
identity XForm. Native LightBurn displays the **three-cell row at the top**
and the one-cell row at the bottom. Thus embedded row zero maps to local
negative Y, not positive Y. `native-00.gc`, exported by LightBurn using absolute
GRBL coordinates, starts at Y65.099 with a 10 mm burn and ends near Y95 with
30 mm burns. The file is an offline reference, not a hardware test job.

`native-00.lbrn2` is a native Save As of the input. The other native files were
created by changing only the input root MirrorX/MirrorY flags before opening
and saving in the same bottom-left-origin LightBurn profile:

| Input flags X/Y | Native saved bitmap XForm |
| --- | --- |
| false/false | 1 0 0 1 60 80 |
| false/true | 1 0 0 -1 60 80 |
| true/false | -1 0 0 1 60 80 |
| true/true | -1 0 0 -1 60 80 |

All saved roots are false/false, and all decoded PNG pixels remain identical.
LightBurn recenters this single object's mirror conversion. These files verify
pixel orientation and matrix signs; they do not establish absolute placement
across different machine bed sizes.

The service regression `lbrn_asymmetric_bitmap_orientation_survives_import_and_planning`
checks black and white cell centers after parsing and planning, across both
Beam Bench origins, both LightBurn format versions, all four root mirror flag
combinations, eight object transforms, and a translated parent group.
