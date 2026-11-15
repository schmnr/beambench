# Monport GRBL K40 preset evidence

- Evidence level: Documentation-based. Physical Beam Bench testing is pending.
- Sources checked: November 14, 2026.
- Scope: stock 300 x 200 mm K40 XY mechanics with the named GRBL controller,
  through the existing GRBL USB serial adapter.

| Preset ID | Configuration | X x Y, mm | Coordinates |
| --- | --- | --- | --- |
| `monport_k40_grbl_2_0` | K40 with Monport GRBL controller 2.0 | 300 x 200 | Rear-left |
| `monport_k40_pro` | K40 Pro 40W, GRBL / G320 manual | 300 x 200 | Rear-left |

Both use 115200 baud, S1000, M4 dynamic power and buffered transfer.
The profile speed limit is a conservative 6000 mm/min starting value,
not a material setting or the advertised maximum speed.
Homing capability is enabled, but applying a preset does not send Home,
change firmware or enable automatic homing on a new profile. Existing
automatic-homing preferences are preserved. Check the switches and live
`$22=1` before Home. The connected controller's `$30` overrides the
preset power-scale default; verify `$32=1` and both axis directions.

Rear-left is Beam Bench's `top_left` workspace origin. It is established
by the machine setup wizard, not inferred solely from the homing corner.
Board 2.0 can also be fitted to other machines; this preset is restricted to
stock K40 travel and coordinate wiring. Rewired or extended machines need
their own configuration.

Air commands, delays and job headers/footers are blank. An air-assist accessory
does not establish a software-controlled pump command. Focus is manual;
no Z, rotary or camera configuration is enabled by these presets. Keep water
cooling running, frame with the laser off, and start with low-power tests.

## Manufacturer sources

The [Monport download center](https://monportlaser.com/pages/download-center)
links the following manuals. Only configuration facts are used; no vendor
manual, driver, profile or application code is bundled with Beam Bench.

- [GRBL controller 2.0 manual](https://cdn.shopify.com/s/files/1/0591/5128/6471/files/GRBL_Controller_2.0_user_manual.pdf?v=1682420544):
  PDF page 5 specifies GRBL and XY control. Page 13 shows a detected
  300 x 200 mm serial device at 115200 baud and the MonPort / GRBL 1.1f
  startup, version 1.1f.20220618, OPT VML,35,254.
- [K40 Pro / G320 40W manual](https://cdn.shopify.com/s/files/1/0591/5128/6471/files/Monport_40W_co2_laser_engraver_user_manual_1.pdf?v=1688363946):
  PDF page 5 identifies G320. Pages 24–25 explicitly configure X 300,
  Y 200 and rear-left origin. Pages 27–28 show homing, laser mode,
  S1000, 115200 baud and buffered transfer. These screenshot pages were
  rendered and visually checked. Page 28 shows firmware XY limits of
  8000 mm/min; the preset starts below that value.
- [Controller 1.0 manual](https://cdn.shopifycdn.net/s/files/1/0591/5128/6471/files/GRBL_Controller_1.0_User_Manual.pdf?v=1694080961):
  pages 8–9 corroborate 300 x 200 mm, 115200 and the same GRBL startup.
  This older retrofit board does not have a separate preset because the
  reviewed guide does not establish its final coordinate wiring.

The controller guide also contains a generic, previously selected 500 x 500
front-left device. That placeholder is not the configured Monport machine
and is not used for travel or origin defaults.

## Other Monport models reviewed

- **Reno45 / Reno65 Basic:** the
  [current Reno manual](https://cdn.shopify.com/s/files/1/0591/5128/6471/files/Reno_Series_User_Manual_0929.pdf?v=1790653350)
  documents GRBL for the Basic versions, with 400 x 300 and 600 x 400 mm
  travel. Pages 64–67 require the machine's USB-drive device profile and
  controller backup. Page 66 shows 115200 baud and S1000, while page 74
  prints a conflicting 961200 baud. The reviewed material does not establish
  the final Basic origin or machine-specific commands. Obtain the actual
  factory profile and firmware settings before adding named presets.
- **Reno Pro / Pro Vision:** that same manual uses TopWisdom setup for these
  DSP variants (pages 70–72). Beam Bench has no TopWisdom adapter. They must
  not be offered as GRBL or Ruida presets.
- **Larger Ruida CO₂ machines:** Beam Bench has experimental Ethernet Ruida
  support, but a Monport-wide preset would require an exact controller,
  revision, travel, origin and tested connection configuration. A LightBurn
  or RDWorks compatibility claim alone is insufficient.
- **6W desktop diode:** the
  [manufacturer manual](https://cdn.shopify.com/s/files/1/0591/5128/6471/files/user_manual20240819_OCR.pdf)
  describes a 100 x 100 mm desktop module and imports vendor profiles.
  Its model-specific command and coordinate setup are not established
  sufficiently to add a preset.
- **MEGAS, Effi, fiber/MOPA and UV models:** no compatibility is inferred
  from the brand, laser source or a LightBurn compatibility claim.
- **Stock Nano K40:** the older
  [40W manual](https://cdn.shopify.com/s/files/1/0591/5128/6471/files/Monport_40w_plus_CO2_Machine_User_Manual.pdf?v=1654511983)
  uses LaserDRW and a board/device ID. Beam Bench's experimental Lihuiyu
  USB path is separate from GRBL serial; these presets do not convert
  or upgrade that controller.

## Software validation and remaining hardware checks

Tests cover selecting, previewing and applying both presets in the desktop
dialog, saved settings, project workspace/origin updates, vector and raster
planning, M4 power mapping, connected-controller power-scale preservation,
and absence of unsolicited air or homing commands.
The simulated-controller workflow checks laser-off framing, frame/job
agreement in Absolute Coordinates and Current Position, feed limits, job
completion and immediate Stop. Localized setup guidance is checked in all
23 languages. The generic MonPort banner does not select a bed-size preset.

Physical connection, switch wiring, tube power/current calibration, cooling,
both jog directions, manual air, low-power vector/raster output and a longer
job still need confirmation on each actual machine. Use Share machine test
results to record the fitted controller and firmware.
