# Ortur and Aufero USB serial presets

- Scope: the listed standard frames, factory extension kits, laser modules and GRBL 1.1f or newer configurations below.
- Evidence: manufacturer documentation checked November 20 and 26, 2026. Software
  validation covers preset application, saved settings, and vector and raster
  output. Physical Beam Bench validation is pending.

| Preset ID | Stock configuration | X × Y (mm) | Initial speed (mm/min) |
| --- | --- | --- | --- |
| `ortur_lm1` | Laser Master 1, original | 150 × 160 | 3000 |
| `ortur_lm2` | Laser Master 2, original | 400 × 430 | 3000 |
| `ortur_lm2_s2` | Laser Master 2 S2, LU2-2 or LU2-4 | 390 × 410 | 5000 |
| `ortur_lm2_s2_10w` | Laser Master 2 S2 10W (LU2-10A) | 390 × 400 | 5000 |
| `ortur_lm2_s2_ir_1_3w` | Laser Master 2 S2 1.3W IR (LR4-1A) | 390 × 400 | 5000 |
| `ortur_lm2_s2_etk1` | Laser Master 2 S2 + ETK1.0 extension (LU2-2 / LU2-4) | 390 × 800 | 5000 |
| `ortur_lm2_s2_10w_etk1` | Laser Master 2 S2 10W + ETK1.0 extension (LU2-10A) | 390 × 790 | 5000 |
| `ortur_lm2_pro_s2_etk1` | Laser Master 2 Pro S2 + ETK1.0 extension | 400 × 800 | 6000 |
| `ortur_lm3_5_5w` | Laser Master 3 5.5W (LU2-4 SF / LF) | 400 × 400 | 6000 |
| `ortur_lm3_10w_etk2` | Laser Master 3 10W + ETK2.0 extension (LU2-10A) | 400 × 850 | 6000 |
| `ortur_lm3_20w_etk2` | Laser Master 3 20W + ETK2.0 extension (LU3-20A / B) | 400 × 830 | 6000 |
| `ortur_lm3_le_2_1w` | Laser Master 3 LE 2.1W (LU2-2) | 400 × 400 | 6000 |
| `ortur_lm3_le_20w` | Laser Master 3 LE 20W (LU3-20A / B) | 400 × 380 | 6000 |
| `aufero_laser_2_10w` | Aufero Laser 2 10W (LU2-10A) | 390 × 390 | 6000 |
| `aufero_laser_2_extended` | Aufero Laser 2 + Y-axis extension (LU2-2 / LU2-4 / LU2-10A) | 390 × 600 | 6000 |
| `ortur_lm2_pro_s1` | Laser Master 2 Pro S1, original Pro frame | 400 × 400 | 6000 |
| `ortur_lm2_pro_s2_10w` | Laser Master 2 Pro S2, LU2-10A | 400 × 400 | 6000 |
| `ortur_lm3_le_5w` | Laser Master 3 LE, LU3-5 or LU2-4 SF/LF | 400 × 400 | 6000 |
| `ortur_lm_h10` | Laser Master H10, LU2-4, LU2-10A or LU3-20A/B | 300 × 300 | 6000 |
| `ortur_h20_10w` | H20, 10 W diode | 420 × 300 | 6000 |
| `ortur_h20_20w` | H20, 20 W diode | 420 × 300 | 6000 |
| `ortur_h20_40w` | H20, 40 W diode | 410 × 275 | 6000 |
| `ortur_h20_ir_2w` | H20, LR4-2A 2 W infrared | 420 × 275 | 6000 |
| `ortur_r1_5w` | R1, LU3-5B 5 W diode | 100 × 100 | 3000 |
| `ortur_r1_ir_1_3w` | R1, LR4-1A 1.3 W infrared | 100 × 80 | 3000 |
| `ortur_r2_10w` | R2, 10 W diode | 300 × 210 | 6000 |
| `ortur_r2_20w` | R2, 20 W diode | 300 × 190 | 6000 |
| `ortur_r2_ir_1_3w` | R2, LR4-1A 1.3 W infrared | 300 × 210 | 6000 |
| `ortur_f20` | F20, stock flatbed | 150 × 150 | 6000 |
| `ortur_f20_pro` | F20 Pro, stock flatbed | 150 × 150 | 6000 |
| `aufero_laser_3` | Aufero Laser 3, LU3-5 or LU2-10B | 400 × 400 | 6000 |

The existing Laser Master 2 Pro S2 diode, Laser Master 3, Laser Master 3 LE
10 W, and Aufero Laser 1/2 entries are covered in the [earlier catalog record](common-grbl-usb-presets.md).

## Connection and output settings

Use **Devices → Connection → Serial (USB / COM port)**, select the laser's
port and **GRBL**, and start at 115200 baud. The connection can try alternate
rates when the initial rate does not respond. **Direct USB (Lihuiyu)** scans
for direct USB Lihuiyu boards. A CH340 or USB CDC serial port belongs under
Serial. The preset does not identify an exact laser from the port or banner.

All entries start with front-left origin, S1000, M4 dynamic power and buffered
transfer. Use the live controller's `$30` power scale and verify laser mode
(`$32=1`) before output. Front-left comes from the original manuals and is the
starting GRBL convention where newer model documentation does not specify an
origin. Verify coordinate direction and the footprint with a laser-off frame.
Speed limits here are starting values, not cutting or engraving recipes.

Homing support and automatic air commands start disabled. Applying a preset
does not write firmware settings or run motion. An existing user preference
for homing on connect is preserved. Controller boot homing is independent of
Beam Bench. Vendor camera, focus, enclosure, offline, network, aftermarket extensions,
rotary and module-offset workflows are outside this preset evidence.

## Model sources and limits

- The [original Laser Master manual](https://www.totalbug.com/wp-content/uploads/2021/09/manuel_ortur-laser-master1.pdf)
  gives X150/Y160 for GRBL Serial/USB. The [original Laser Master 2 manual](https://m.media-amazon.com/images/I/C1fDyk%2BmWgS.pdf)
  gives X400/Y430 with a front-left origin. [Ortur's v1.40 firmware notes](https://forum.lightburnsoftware.com/t/ortur-laser-master-series-v1-4-firmware-release/30442)
  identify the GRBL 1.1f to 1.1h transition for the two originals. The
  [Laser Master 2 v1.40 guide](https://manuals.plus/m/d394b9c98e57396626c207e2777adf4520fa66b73f9038284ac7ea47c5b2b986.pdf)
  shows actual serial communication at 115200. Earlier manual screenshots
  show other rates, so 115200 is a starting rate, not a firmware requirement.
- [Laser Master 2 S2 support](https://ortur.net/pages/support-olm2-s2)
  lists the base frame and a 5000 mm/min speed limit. [Ortur's comparison table](https://ortur.net/pages/product-comparisons)
  includes the LU2-10A option. The [Laser Master 2 Pro manual](https://tesztarena.hu/wp-content/uploads/2021/07/OLM2_Pro_User_Manual_EN.pdf)
  gives 400 × 400 at 115200. The [Pro S1 firmware guide](https://manuals.plus/m/dc323c9430b1eb6bd0ac8678fc4ea9f32ffac9432a5b4e29b21f46f354078380.pdf)
  confirms the Pro S1 identity, GRBL and 115200. [Pro S2 support](https://ortur.net/pages/support-olm2-pro-s2)
  lists the 400 × 400 frame and the LU2-10A module. These three models are
  distinct from the original Laser Master 2.
- [Laser Master 3 LE support](https://ortur.net/pages/support-olm3le) lists
  the LU2-4 SF/LF modules and a 400 × 400 frame. [Laser Master H10 support](https://ortur.net/pages/support-olm-h10)
  lists the H10's 300 × 300 area and supported modules. Both list 115200.
- [H20 support](https://ortur.net/pages/support-h20) lists serial control at
  115200. The [H20 product page](https://ortur.net/products/ortur-h20-laser-engraver-cutter-machine)
  gives workspaces by module. [Ortur's module guide](https://ortur.net/blogs/ortur-blog-center/ortur-laser-module-guide-how-to-pick-the-perfect-one-for-your-ortur-laser-engraver)
  identifies the LR4-2A variant for the H20. Camera calibration and offset workflows
  need their own setup.
- [R1 support](https://orturtech.com/pages/r1-support) and [the R1 product page](https://ortur.net/products/ortur-r1-laser-engraver)
  distinguish its two stock modules. [R2 support](https://ortur.net/pages/support-r2)
  lists 115200. The [Ortur FAQ](https://ortur.net/pages/faq) confirms that
  the R2 20 W has 20 mm less Y travel than the other two modules.
- [F20 support](https://orturtech.com/pages/f20-support) and [F20 Pro support](https://orturtech.com/pages/f20-pro-support)
  list 150 × 150, USB LaserGRBL control, the same M2P ESP-S2 mainboard and
  a 10000 mm/min rated speed. Their workspaces follow the model-specific
  support pages. [F10 support](https://ortur.net/pages/support-f10)
  confirms that 400 × 3000 is the F10 conveyor specification.
- [Aufero Laser 3 support](https://orturtech.com/pages/al3-support) specifies
  USB PC control and gives 400 × 400 for the listed 5 W and 10 W modules.
  For R1, F20, F20 Pro and Aufero Laser 3, 115200 is the normal GRBL starting
  rate because their support tables do not specify a rate. S1000 is the GRBL
  1.1 starting scale where Ortur does not publish one. Confirm it against live `$30`.

### Model and module checks

For the Laser Master 2 S2, the LU2-2 and LU2-4 modules use 390 × 410 mm.
Both the LU2-10A and the LR4-1A need Y limited to 400 mm. The module manuals
specify the limit plate adjustment and the matching `$131`. Users of an
earlier source build who chose the combined S2 preset for a 10 W head should
apply the 10 W preset to that profile. Saved profiles keep their settings until
the operator previews and applies a preset.
[Ortur machine specification](https://orturtech.com/pages/machine-specification),
[LU2-10A setup, page 2](https://cdn.shopify.com/s/files/1/0730/0372/9203/files/OLM2S2_Exclusive_Folding_-_LU2-10A_Follow_up_Manual.pdf)
and [LR4-1A setup, page 2](https://cdn.shopify.com/s/files/1/0730/0372/9203/files/OLM2S2_Exclusive_-_LR4-1A_Follow_up_Manual.pdf).

The standard Laser Master 3 supports the LU2-4 SF/LF at 400 × 400 mm and
the LU3-20B at 400 × 380 mm. On the LE, the LU2-2 uses 400 × 400 mm and
the 20 W head uses 400 × 380 mm. Ortur also sells the LE with a 5 W LU3-5
on the 400 × 400 mm frame. The Aufero Laser 2 LU2-10A has 390 × 390 mm travel.
[Laser Master 3 module table](https://orturtech.com/pages/olm3-support),
[LE module table](https://orturtech.com/pages/olm3-le-support),
[LE 5 W specifications](https://ortur.net/products/ortur-laser-master-3-lite),
[machine specification by power](https://orturtech.com/pages/machine-specification)
and [Aufero Laser 2 specifications](https://orturtech.com/pages/al2-support).

### Factory extension kits

Choose an extension entry only after installing the kit for that frame.
Check physical travel and the configured `$130` (X) and `$131` (Y) before
framing. The presets describe the installed geometry and do not configure the
controller. The ETK1.0 S2 10 W and ETK2.0 LM3 20 W have shorter Y travel
than their lower-power counterparts. The LE is excluded from the extended
entries because Ortur's FAQ says it cannot use the LM3 extension kit.
[Factory kit and frame pairings](https://ortur.net/blogs/ortur-blog-center/ortur-laser-engraver-accessory-guide),
[module-specific extension travel](https://orturtech.com/pages/machine-specification),
[Aufero Laser 2 extension](https://orturtech.com/pages/al2-support)
and [Ortur FAQ](https://ortur.net/pages/faq).

The [F10 manual](https://www.manualslib.com/manual/3684795/Ortur-F10.html?page=69)
calls for conveyor-specific setup and material handling. It needs a separate
review of feed coordinates, framing and job completion. The Aufero Laser 3 support table omits the 20 W module's travel. Its manual
shows a 20 mm limit-plate adjustment for different head sizes without naming
the module for each position. A dedicated AL3 20 W preset needs confirmation
of the usable X and Y travel for that installation.
[Aufero Laser 3 manual, PDF page 21](https://cdn.shopify.com/s/files/1/0730/0372/9203/files/AL3_manual.pdf).
