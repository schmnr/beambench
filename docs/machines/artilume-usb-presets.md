# Artilume USB preset evidence

- Evidence level: Documentation-based. Physical Beam Bench testing is pending.
- Sources checked: November 13, 2026.
- Catalog: 18 stock machine/module configurations, all at preset version 1.
- Scope: the existing GRBL USB adapter, with the named module physically fitted
  and focused. No Wi-Fi, TF-card transfer, camera or rotary setup is implied.

## Exact configurations

Travel is X x Y in millimetres. All entries use 115200 baud, front-left
coordinates, S1000 and dynamic power M4. Verify the controller's `$30` and
`$32=1`; a live `$30` value takes precedence over the preset's starting value.
The 6000 mm/min profile limit is a conservative starting setting, not a claim
about maximum firmware speed or a material cutting recommendation.

| Preset ID | Machine and fitted module | X x Y | Homing flag |
| --- | --- | --- | --- |
| `artilume_t1_2_5w` | T1 2.5W | 150 x 200 | On |
| `artilume_t1_3w` | T1 3W | 150 x 200 | On |
| `artilume_t1_6w` | T1 6W | 150 x 200 | On |
| `artilume_t1_7w` | T1 7W | 150 x 200 | On |
| `artilume_t1_10w` | T1 10W | 150 x 200 | On |
| `artilume_t1_12w` | T1 12W | 150 x 200 | On |
| `artilume_t1_ir_1w` | T1 1W IR, 1064 nm | 150 x 200 | On |
| `artilume_t1_p_3w` | T1-P 3W | 150 x 200 | On |
| `artilume_t1_p_7w` | T1-P 7W | 150 x 200 | On |
| `artilume_t1_p_12w` | T1-P 12W | 150 x 200 | On |
| `artilume_t1_p_ir_1w` | T1-P 1W IR, 1064 nm | 150 x 200 | On |
| `artilume_u1_3w` | U1 3W | 200 x 150 | Off pending owner verification |
| `artilume_u1_7w` | U1 7W | 200 x 150 | Off pending owner verification |
| `artilume_u1_12w` | U1 12W | 200 x 150 | Off pending owner verification |
| `artilume_u1_ir_1w` | U1 1W IR, 1064 nm | 200 x 150 | Off pending owner verification |
| `artilume_a4_7w` | A4 7W | 400 x 400 | On |
| `artilume_a4_12w` | A4 12W | 400 x 400 | On |
| `artilume_a4_22w` | A4 22W | 400 x 400 | On |

The homing flag describes the documented homing workflow. Applying a preset
does not enable firmware homing, send `$H`, or opt a new profile into homing on
connection. The existing owner preference for automatic homing is preserved.
Live GRBL settings remain authoritative. Check physical switches and `$22=1`
before requesting Home. U1 starts with homing off because its reviewed setup
videos use Current Position and do not establish its switch configuration.

Air commands, air delays and job headers/footers are blank. The reviewed sources
do not establish an automatic pump command for these configurations. Focus is
manual. Existing Z, rotary, camera and network preferences require their own
setup; the presets do not configure those accessories or rewrite firmware.

## Manufacturer sources and setting rationale

The [download center](https://artilumelab.com/pages/artilume-download-center)
links all three manuals. The [manufacturer FAQ](https://artilumelab.com/pages/faq)
explicitly states 115200 baud for all machines. Only factual settings are
extracted. Vendor manuals, profiles, drivers, videos and application code are
not bundled or redistributed with Beam Bench.

### T1 and T1-P

The [T1 manual](https://cdn.shopify.com/s/files/1/0648/5711/2624/files/T1-User-Manual_English.pdf?v=1785118815)
co-documents T1 and T1-P on PDF pages 5-6. PDF page 4 lists the older 2.5W,
6W and 10W labels as well as 3W, 7W, 12W and 1064 nm 1W. The current
[T1 product](https://artilumelab.com/products/artilume-t1-foldable-laser-engraver)
and [T1-P product](https://artilumelab.com/products/t1-foldable-laser-engraver-with-app-control-riser-protective-casing-portable-engraving-machine-for-beginners)
list the current 3W/7W/12W modules and IR option. Legacy T1-P power labels are
not separately added without a model-specific listing.

Relevant manual PDF pages:

- 10: remove shipping protection from the two limit switches, install the
  module and focus it. Unfold and secure the machine before moving it.
- 18: connect over USB at 115200.
- 20: M4 dynamic power, `$32=1`, and a LaserGRBL example with S-MAX 1000
  corresponding to 100 percent.
- 21: home to front-left before framing or engraving.
- 23: manually select GRBL, Serial/USB, X 150 x Y 200 and front-left origin.
- 26-29: module-specific material tables, including M4 for the IR module.

The [T1 LightBurn setup guide](https://artilumelab.com/blogs/product-tutorials/operating-the-t1-using-lightburn)
independently specifies **X 150 and Y 200**. Generic wizard screenshots in the
manual show 400 x 400; these contradict its model-specific written instructions
and are not used as travel evidence. The IR entry selects a physically changed
module. It does not issue a wavelength-switching macro.

### U1

The [U1 manual](https://cdn.shopify.com/s/files/1/0648/5711/2624/files/ARTILUME_U1_Instruction_Manual_English.pdf?v=1784879564),
PDF page 5, documents USB, current 3W/7W/12W and 1064 nm 1W modules, and a
7000 mm/min English specification. The Chinese column lists older power and
speed figures. Current module configurations are independently confirmed by
the [U1 module catalog](https://artilumelab.com/products/artilume-u1-laser-modules).

The manual's printed area is 150 x 200 without axis labels, while the
manufacturer FAQ specifies 200 x 150 for U1. Its current product page also
contains an unrelated 400 x 400 specification block. Resolve this using the
manufacturer's setup videos linked by the QR codes on **manual PDF page 4**:

- [First Time LightBurn Connection Instructional Video](https://www.youtube.com/watch?v=91Ljw-mP0_0),
  published by ARTILUME. At approximately 55 seconds the imported manufacturer's
  profile visibly has **X 0-200 and Y 0-150**, with origin bottom-left. The
  tutorial uses Current Position. It shows a U1, not the foldable T1.
- [First time connecting LaserGRBL](https://www.youtube.com/watch?v=_sngR2guDPE),
  published by ARTILUME. At approximately 36 seconds the USB connection is at
  115200 with a `Grbl_ESP32` startup sequence. At approximately 44-48 seconds,
  its image setup explicitly selects **M4 and S-MAX 1000 at 100 percent**.

These are the basis for U1's axis ordering, origin and power mapping. They are
not evidence of Beam Bench running on physical hardware. No automatic homing
or air command is inferred from the related T1/A4 machines.

### A4

The [A4 manual](https://cdn.shopify.com/s/files/1/0648/5711/2624/files/ARTILUME-A4-Instruction-Manual_English.pdf?v=1784886712)
is image-only; its relevant pages were rendered and visually inspected:

- PDF pages 4 and 6: 400 x 400 mm, 7W/12W/22W modules and USB.
- 15: connect at 115200.
- 17: M3/M4 explanation, `$32=1`, and S-MAX 1000 at 100 percent.
- 18: home to front-left before framing.
- 20: GRBL Serial/USB, X 400 and Y 400, front-left origin.

The [A4 product page](https://artilumelab.com/products/artilume-a4-large-format-laser-engraver)
confirms the three module options and 400 x 400 area. Basic, color and accessory
packages are not counted as separate controller configurations. The preset
does not select a hardware voltage, module or material power automatically.

## Remaining scope and owner checks

- Wi-Fi and TF-card/offline operation use separate manufacturer workflows and
  are not included in these USB presets. Close other engraving applications
  before connecting Beam Bench to the USB port.
- S1/D2 and their P variants appear in the shared T1 manual. Their Artilume
  model identity and revisions are not independently established, so they are
  not aliases in the catalog.
- Older U1 5W/10W labels, custom boards, extension frames, automatic air,
  rotary axes, cameras and other accessory configurations need separate review.
- IR presets require the correctly fitted and focused 1064 nm module. They
  are ordinary XY GRBL output, not support for a galvo or software laser switch.
- Owners should verify connection, both jog directions, Home only where
  supported, a laser-off frame, low-power vector and raster output, Stop,
  save/reopen and a longer job. Record the firmware and module in the app's
  Share machine test results report.

## Software validation

The catalog/output workflow covers all 18 entries through preset preview and
application, active-project workspace updates, M4/S250 output at 25 percent,
vector and raster planning, Y-origin conversion, and absence of automatic
air, homing or vendor macro commands in generated jobs. Connected-controller
power-scale tests include every new entry. Desktop tests cover selecting,
previewing and applying every entry through the existing profile dialog.
Locale tests check setup guidance in all 23 languages. Physical validation
remains pending.
