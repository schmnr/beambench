# Common GRBL USB preset evidence

- Presets: the 24 exact configurations listed below, version 1.
- Evidence level: Documentation-based. Physical Beam Bench testing is pending.
- Sources checked: October 20, 2026.
- Scope: the listed base frames and modules using GRBL 1.1-compatible firmware
  over USB serial. M4 entries require GRBL 1.1f or newer with laser mode enabled.

## Exact configurations

| Preset ID | Model and module | Workspace X x Y, mm | Power mode | Air commands |
| --- | --- | --- | --- | --- |
| `ortur_lm2_pro_s2` | Ortur Laser Master 2 Pro S2 (LU2-2 / LU2-4) | 400 x 400 | M4 dynamic | None |
| `ortur_lm3_10w` | Ortur Laser Master 3 10W (LU2-10A) | 400 x 400 | M4 dynamic | None |
| `ortur_lm3_20w` | Ortur Laser Master 3 20W (LU3-20A) | 400 x 380 | M4 dynamic | None |
| `ortur_lm3_40w` | Ortur Laser Master 3 40W (LU3-40A) | 400 x 380 | M4 dynamic | None |
| `ortur_lm3_le_10w` | Ortur Laser Master 3 LE 10W (LU2-10A) | 400 x 400 | M4 dynamic | None |
| `aufero_laser_1` | Aufero Laser 1 (LU2-2 / LU2-4) | 180 x 180 | M4 dynamic | None |
| `aufero_laser_2` | Aufero Laser 2 (LU2-2 / LU2-4) | 390 x 390 | M4 dynamic | None |
| `atomstack_a10_pro_v2` | AtomStack A10 Pro V2 | 400 x 365 | M4 dynamic | None |
| `atomstack_a20_pro_v2` | AtomStack A20 Pro V2 | 400 x 365 | M4 dynamic | None |
| `atomstack_a40_pro_v2` | AtomStack A40 Pro V2 | 385 x 400 | M4 dynamic | None |
| `creality_falcon2_12w` | Creality Falcon2 12W | 400 x 415 | M4 dynamic | None |
| `creality_falcon2_22w` | Creality Falcon2 22W | 400 x 415 | M4 dynamic | None |
| `creality_falcon2_40w` | Creality Falcon2 40W | 400 x 415 | M4 dynamic | None |
| `sculpfun_s10` | Sculpfun S10 10W | 385 x 385 | M3 constant | None |
| `sculpfun_s30_5w` | Sculpfun S30 5W | 380 x 385 | M4 dynamic | M8 / M9 |
| `sculpfun_s30_ultra_11w` | Sculpfun S30 Ultra 11W | 590 x 595 | M4 dynamic | M8 / M9 |
| `sculpfun_s30_ultra_22w` | Sculpfun S30 Ultra 22W | 590 x 595 | M4 dynamic | M8 / M9 |
| `sculpfun_s30_ultra_33w` | Sculpfun S30 Ultra 33W | 590 x 595 | M4 dynamic | M8 / M9 |
| `twotrees_tts_55` | Two Trees TTS-55 (original) | 300 x 300 | M3 constant | None |
| `twotrees_tts_55_pro` | Two Trees TTS-55 Pro | 300 x 300 | M3 constant | None |
| `twotrees_tts_10_pro` | Two Trees TTS-10 Pro | 300 x 300 | M3 constant | None |
| `twotrees_tts_20_pro` | Two Trees TTS-20 Pro | 418 x 418 | M3 constant | None |
| `twotrees_ts2_10w` | Two Trees TS2 10W | 450 x 450 | M3 constant | None |
| `twotrees_ts2_20w` | Two Trees TS2 20W | 410 x 410 | M3 constant | None |

## Shared control settings

All entries use 115200 baud, bottom-left/front-left coordinates, initial S1000,
100% maximum controller-scale power, buffered transfer, no job header/footer,
no per-G1 S requirement and no G0 overscan. A valid connected controller `$30`
takes precedence in preview and application. Verify `$32=1`. The listed optical
wattages are model identifiers, not the source of the power scale.

The profile speed starts at 6000 mm/min, except Aufero Laser 1 and Sculpfun S10
at 3000 mm/min. These are Beam Bench starting limits, not manufacturer speed
ratings or material settings. Controller limits still apply.

Homing capability starts off pending the owner's switch and firmware check.
Automatic homing is off in a new profile; applying a preset preserves an
existing `home_on_connect` preference. None of these presets sends `$H`, writes
controller settings or changes a device's own boot behavior. Some Ortur firmware
homes the hardware at power-on independently of Beam Bench.

S30 5W and S30 Ultra have documented M8/M9 pump control, used only on cuts with
air assist enabled. Other pump commands are left blank. A LightBurn M7/M8 radio
button alone does not establish an installed, automatic pump interface.

## Ortur and Aufero

The model-specific manufacturer support tables establish the workspace, baud,
serial transport and S0–1000 power scale:

- [Laser Master 2 Pro S2](https://ortur.net/pages/support-olm2-pro-s2):
  400 x 400 mm. This entry covers the listed LU2-2 and LU2-4-SF/LF modules.
- [Laser Master 3](https://ortur.net/pages/support-olm3): LU2-10A is
  400 x 400 mm; LU3-20A and LU3-40A are 400 x 380 mm. Separate presets avoid
  retaining the 10W Y travel after changing modules.
- [Laser Master 3 LE](https://ortur.net/pages/support-olm3le): 400 x 400 mm
  for the LU2-10A configuration. Larger replacement modules need their own setup.
- [Aufero Laser 1](https://ortur.net/pages/support-al1): 180 x 180 mm,
  LU2-2 / LU2-4 modules, manufacturer maximum speed 5000 mm/min.
- [Aufero Laser 2](https://ortur.net/pages/support-al2): 390 x 390 mm,
  LU2-2 / LU2-4 modules. LU2-10A and other upgrades are excluded.

The LM3 support page links the
[LU3-20A installation instructions](https://cdn.shopify.com/s/files/1/0503/4734/4056/files/1-OLM3_LU3-20A-UserManual-en-de-fr-231016.pdf?v=1711532217),
whose last page explicitly reduces `$131` from 400 to 380. That is evidence
for travel, not a firmware-setting command Beam Bench sends.

The linked manuals primarily cover assembly, accessories and the vendor app.
USB operation uses the existing GRBL adapter. M4 follows
[standard GRBL 1.1 laser mode](https://github.com/gnea/grbl/wiki/Grbl-v1.1-Laser-Mode).
Bottom-left is Beam Bench's GRBL starting convention; physical axis orientation
and work/machine-zero behavior still need verification. No vendor accessory
macros, automatic pump, rotary or network workflow is assumed.

## AtomStack V2

[AtomStack's support page](https://atomstack.com/pages/product-support) links
separate manuals for
[A10/A20 Pro V2](https://cdn.shopify.com/s/files/1/0790/2586/4977/files/A5_10_A20_PRO_V2_C_20240508_1_2__compressed.pdf?v=1728975233)
and [A40 Pro V2](https://cdn.shopify.com/s/files/1/0790/2586/4977/files/F03-0262-0AA1_A40_PRO_V2_20240426.pdf?v=1728966709).
They establish USB PC control with LightBurn/LaserGRBL. The manufacturer's
[LightBurn guide](https://cdn.shopify.com/s/files/1/0790/2586/4977/files/LightBurn_Software_User_Manual.pdf?v=1715150625),
PDF pages 5–6, selects GRBL, Serial/USB and front-left, and shows S1000,
115200 baud and buffered transfer. M4 uses the stated GRBL 1.1f-or-newer scope.

Model-specific workspaces come from the current manufacturer specifications:

- [A10 Pro V2](https://atomstack.com/products/atomstack-a10-pro-v2-laser-engraver):
  its highlights list 400 x 365 mm, while the table and
  [manufacturer's Chinese page](https://www.atomstack.cn/products/a10-pro-v2.html)
  list 410 x 380 mm. Use the smaller 400 x 365 mm initial workspace.
- [A20 Pro V2](https://atomstack.com/products/atomstack-ace-pro-v2):
  the specification table lists 400 x 365 mm.
- [A40 Pro V2](https://atomstack.com/products/atomstack-ace-a40-pro-v2-laser-engraver):
  the basic frame is 385 x 400 mm; the extension is excluded.

These entries use laser-spot positioning. The A40 V2 manual describes auxiliary
crosshair positioning with separate calibration, a smaller area and a vendor
`[ESP501]` operation. It is disabled at the factory and is not configured by
these presets. Existing crosshair mode, autofocus/Z setup, hardware power-mode
switching, infrared replacements and recovery of offline jobs need separate
review. No vendor macros or configuration files are copied or sent.

Original, non-V2 A10/A20/A40 Pro configurations are not represented by these
IDs. Older manufacturer download links and reused product URLs do not establish
that their travel and accessories match V2. Keep those revisions under review
until exact profiles or independently verifiable specifications are available.

## Standard Creality Falcon2

[Creality's Falcon2 manual](https://cdn.shopifycdn.net/s/files/1/0799/8714/7049/files/Creality_Falcon2_User_Manual.pdf?v=1718179969),
PDF page 8, gives 400 x 415 mm for all three 12W/22W/40W columns. PDF page 15
establishes USB PC operation with LightBurn and LaserGRBL. The
[manufacturer wiki](https://wiki.creality.com/en/laser-engraver/falcon2) groups
these three standard models, separately from Falcon2 Pro and Pro S.

[Creality's FAQ](https://eu.crealityfalcon.com/pages/faq) directs Falcon2 users
to the series' `Creality Falcon2.lbdev` file. A
[public primary configuration in Creality's community forum](https://forum.creality.com/t/profile-settings/36308/5)
provides that file's settings: GRBL, Serial, 115200 baud, S1000, 400 x 415 mm,
no axis mirrors, buffered transfer and empty startup/end code. This is a
community-posted configuration for a 40W request, not verified manufacturer
publication or a Beam Bench physical test. The manufacturer manual corroborates
the series workspace and USB software path. These entries use the series
configuration as the initial power/serial default, with the live controller's
valid `$30` taking precedence. M4 follows GRBL 1.1 laser mode. Bottom-left is
the GRBL starting convention, with physical positioning still unverified.

Automatic homing from the shared file is deliberately not enabled. Integrated
pump control, beam-mode changes, offline TF-card operation and vendor safety/
enclosure controls are not configured by the preset. No Pro defaults are used
as evidence for standard Falcon2 dimensions.

## Sculpfun

[The manufacturer's S10 setup guide](https://www.sculpfun.com/blogs/blog/setting-up-the-software)
provides GRBL serial setup, bottom-left origin, S1000 and a linked S10 device
profile. That profile gives 385 x 390 mm, 115200 baud and empty startup/end code.
The [current S10 product page](https://www.sculpfun.com/products/sculpfun-s10-laser-engraver-machine)
contains differing areas, including 385 x 385 mm in the overview and 390 x 400 mm
in the table. Use the smaller 385 x 385 mm initial area. The
[S10 manual](https://cdn.shopify.com/s/files/1/0628/0695/0066/files/User_Manual_of_SCULPFUN_S10_Laser.pdf?v=1729223760),
PDF pages 4–6, documents 115200 baud, M3/M4 and the absence of stock limit
switches. M3 is the starting mode because the manual recommends it for uses
other than 1-bit dithering. Owners with verified M4 firmware can select dynamic
power for engraving. No automatic air interface is assumed for its manual pump.

[The S30 5W product specification](https://www.sculpfun.com/products/sculpfun-s30-laser-engraver-machine-5w)
establishes 380 x 385 mm, USB and an automatic pump controlled by M8. S1000,
origin and GRBL 1.1f-or-newer M4 follow the same manufacturer's software setup
and [settings guide](https://www.sculpfun.com/blogs/blog/settings-guide).
This is a separate entry from S30 Pro 10W and Pro Max 20W.

[The S30 Ultra product specification](https://www.sculpfun.com/products/sculpfun-s30-ultra-33w-laser-engraving-and-cutting-machine-lightburn-core-license-key)
explicitly groups the 11W/22W/33W configurations with the same frame, adapter,
cabling, 590 x 595 mm usable area and M8 air support. The
[manufacturer comparison](https://www.sculpfun.com/pages/machine-comparison)
also gives 590 x 595 mm for 22W/33W. Older 600 x 600 mm advertising describes a
larger area than the current usable specification. Use 590 x 595 mm for all
three standard configurations. The common manufacturer's GRBL/S1000 setup
applies; 115200 baud follows the standard GRBL 1.1 serial protocol. M9 is the
standard GRBL coolant-off command. Supplied limit switches do not establish an
owner's configured homing cycle, so homing starts off.

## Two Trees

[The manufacturer download center](https://twotrees3d.com/pages/download) and
[older help center](https://twotrees3d.com/pages/help-center-1) provide these
model-specific manuals. Only factual settings are extracted; manuals and
third-party profiles are not redistributed.

- [Original TTS manual](https://www.mediafire.com/file_premium/bcqz4t1az8nmmgv/TTS-Manual%25EF%25BC%2588%25E6%2589%258B%25E5%2586%258C%25EF%25BC%2589.pdf/file):
  PDF page 5 lists the 5.5W module, 300 x 300 mm and a 32-bit ESP32 board. PDF pages
  17–18 document M3 for general use, M4 for 1-bit engraving, S1000 and 115200 baud.
- [TTS-55 Pro / TTS-10 Pro manual](https://www.mediafire.com/file_premium/emv1b1umc6cwqoj/TTS-Pro_Mamual-A3.pdf/file):
  PDF pages 4, 8 and 10 give the respective 5.5W/10W modules, 300 x 300 mm,
  S1000, GRBL USB, 115200 baud, front-left and disabled automatic homing.
- [TTS-20 Pro product specification](https://twotrees3d.com/products/tts-20-20w-laser-engraver-machine):
  418 x 418 mm. Its [own troubleshooting guide](https://wiki.twotrees3d.com/en/LaserEngravingMachine/TTS-20PRO/CommonFaultInformation)
  establishes S1000, 115200 baud and M3/M4 behavior. Bottom-left is the GRBL
  starting convention pending physical validation.
- [TS2 10W manual](https://www.mediafire.com/file_premium/ihygdpu9puceoc6/TS2_Manual-EN-DE.pdf/file):
  PDF pages 6 and 14–15 establish 450 x 450 mm, S1000, M3/M4 and 115200 baud.
  Its [USB setup guide](https://wiki.twotrees3d.com/en/LaserEngravingMachine/TS210W/SoftwareConnectionTutorial)
  selects GRBL, front-left and no automatic homing.
- [TS2 20W manual](https://www.mediafire.com/file_premium/tkymryewmp7206o/TS2-20W-Manual-EN-DE.pdf/file):
  PDF pages 6 and 13–15 establish **410 x 410 mm**, S1000, M3/M4 and 115200 baud.
  Its [troubleshooting page](https://wiki.twotrees3d.com/en/LaserEngravingMachine/TS220W/CommonFaultInformation)
  reuses 450 x 450 mm language from 10W. The exact 20W manual takes precedence.
  Bottom-left is the GRBL starting convention pending physical validation.

The Pro models' own wiki troubleshooting guides recommend M3 constant power for
cutting and M4 for engraving. These presets start with M3, consistent with the
manuals' general-use recommendation. Verified M4 can be selected for engraving.
No automatic air commands are established here. TS2 Z/autofocus is not enabled
by these presets; use a verified manual focus setup. Extension frames, Max
revisions, newer 40W modules and wireless/offline controls are excluded.

## Validation and remaining work

Shared backend regressions apply all 24 entries, synchronize the bound project,
and generate a rectangle through the real planner and GRBL emitter. They check
workspace, bottom-left coordinates, S250 at 25%, M3/M4 and M5, paired M8/M9 only
for requested S30 air assist, absence of unintended air/homing commands, and
retention of a connected controller's `$30=255` in preview and application.
Catalog checks validate unique IDs and settings. UI regressions preview and
explicitly apply every entry and verify dimensions, air commands and power mode.
Guidance is present in all 23 supported locales, with technical tokens checked.

No physical laser was connected or operated. Connection/reconnection, jog,
frame, vector/raster output, pause/resume and cancellation still require
[owner validation](../machine-presets.md#owner-validation). Exact firmware,
module, frame and accessory coverage must accompany any future physical report.
These are manual preset choices; a generic GRBL banner or shared USB chip does
not identify a model. Saved profiles are not silently rewritten.

The [D1/D1 Pro native Wi-Fi adapter](xtool-d1.md) has separate stock-module
presets on unreleased `main`. Its USB serial lifecycle is still under investigation. Other
unlisted revisions, original AtomStack Pro models, Elegoo Phecda, NEJE and
proprietary/galvo families remain outside this batch.
