# ACMER GRBL USB preset evidence

- Evidence level: Documentation-based. Physical Beam Bench testing is pending.
- Sources checked: October 26, 2026.
- Scope: the exact stock diode configurations below, with GRBL 1.1f-or-newer
  laser-mode firmware over USB serial. Wi-Fi/app control, IR modules, extension
  frames, rotary attachments and replacement controllers are outside this scope.

## Exact configurations

| Preset ID | Stock model/module | Workspace X x Y, mm |
| --- | --- | --- |
| `acmer_p1_s_pro_6w` | P1 S Pro 6W | 380 x 370 |
| `acmer_p1_s_pro_10w` | P1 S Pro 10W | 380 x 370 |
| `acmer_p2_10w` | P2 10W | 420 x 400 |
| `acmer_p2_20w` | P2 20W | 420 x 400 |
| `acmer_p2_33w` | P2 33W | 420 x 400 |
| `acmer_s2_pro_36w` | S2 Pro 36W | 400 x 400 |
| `acmer_s2_pro_48w` | S2 Pro 48W | 400 x 400 |

These are separate model entries even where settings match, so the selected
module and its future evidence remain explicit. They do not replace the existing
ACMER S2 preset or rewrite saved profiles.

## Sources and settings

ACMER's [download center](https://acmerlaser.com/pages/download) links the
manufacturer-owned Drive folders below. The manuals were read as text and their
device-setup screenshots were visually inspected. Only factual settings are
used; vendor PDFs, firmware and software are not redistributed.

### P1 S Pro 6W and 10W

- [Stock TF-card documents](https://drive.google.com/drive/folders/1xLTSu-qMafuKbbZh96kMTqNY__KZGDLr).
- [P1 S Pro LightBurn guide](https://drive.google.com/file/d/1VQ4_PYdR2ttkLFq1FIDgM4gWMjQSB4WT/view),
  PDF page 6: GRBL, Serial/USB, X=380, Y=370, front-left origin, automatic
  homing unchecked. PDF page 8 specifies 115200 baud. Page 9 identifies S1000
  and its device-settings screenshot shows buffered transfer.
- [P1 S Pro FAQ](https://drive.google.com/file/d/1KgkmR9mfFGejIl2rscsVIOqfVJvYWhV6/view),
  Q2/Q3 independently specify 115200 baud and lower-left origin.
- [P1 S Pro assembly manual](https://drive.google.com/file/d/1x-jnkyIB48E15k30FuEgciTuMuPIH0lL/view)
  explicitly describes the stock 6W and 10W focus configurations. The
  [6W/10W product page](https://acmerlaser.com/products/acmer-p1-s-pro-6w-best-budget-laser-engraver)
  also specifies a workspace up to 380 x 370 mm.

Use the model-creation page's explicit 380 x 370 dimensions. The later generic
device-settings screenshot shows 400 x 400, which conflicts with the model
instructions and is not used as travel evidence.

### P2 10W, 20W and 33W

- [Stock TF-card documents](https://drive.google.com/drive/folders/1BJy9afj9lrKQUX3XeYqOLVUESp6GQHA4).
- [P2 LightBurn guide](https://drive.google.com/file/d/1I6n82E461kyIph-tfAJnqxwO7B16cu9b/view),
  PDF page 6: GRBL, Serial/USB, X=420, Y=400, front-left origin and automatic
  homing unchecked. Page 8 specifies 115200 baud; page 9 identifies S1000 and
  buffered transfer. Its generic 400 x 400 screenshot does not override the
  explicit model-creation dimensions.
- [P2 manual V1.1](https://drive.google.com/file/d/1yWWALtm4pNMpVd6s5POYVEa2n4Gz4m-X/view)
  lists 420 x 400 mm travel and includes 10W, 20W and 33W configurations.
  Section 7.1.1 explicitly selects 115200 baud.
- The [P2 product page](https://acmerlaser.com/products/acmer-p2-laser-engraver-cutter-machine)
  lists these three stock modules. Extension and IR packs are separate.

### S2 Pro 36W and 48W

- [Stock TF-card documents](https://drive.google.com/drive/folders/1GGCxv3tQiiAKkOxgIg4_zB8F1lhbwriZ).
- [S2 Pro manual V1.1](https://drive.google.com/file/d/1kEqGridnhEVFkXtB0zASGaK2qw78tYYA/view),
  PDF page 5: 400 x 400 mm, 36W/48W and USB. Page 17's LaserGRBL screenshot
  selects 115200 baud. Page 18 explicitly describes M4 engraving, M3 cutting
  and a maximum power value of 1000. Page 20's setup screenshot selects GRBL,
  Serial/USB, 400 x 400 and front-left; page 21 also specifies lower-left job
  origin. The [product page](https://acmerlaser.com/products/acmer-s2-pro-laser-engraver)
  confirms both modules and the 400 x 400 mm workspace.

## Shared defaults and deliberate limits

All seven presets use 115200 baud, front-left/bottom-left coordinates,
buffered transfer, initial S1000 and 100% maximum controller-scale power.
An active connected controller's valid `$30` still takes precedence when
previewing or applying a preset. Wattage identifies a module, not its PWM scale.

The default is M4 dynamic power using the existing GRBL adapter. The manufacturer
guides select the GRBL 1.1f-or-newer device type, and the S2 Pro manual explicitly
documents M4. Verify laser mode `$32=1` before output. See the upstream
[GRBL 1.1 laser-mode contract](https://github.com/gnea/grbl/wiki/Grbl-v1.1-Laser-Mode).
No controller settings are written by applying these presets.

The profile starts at 6000 mm/min. This is Beam Bench's conservative starting
limit, not a manufacturer maximum or a material cutting recipe. Controller
limits still apply. No job header/footer, per-G1 S quirk, or G0 overscan is added.

Homing starts disabled pending an owner's switch/firmware check. New profiles
do not automatically home; applying a preset preserves an existing owner's
`home_on_connect` preference. A homing-capable controller or a displayed Home
button does not prove installed switches.

Automatic air-assist commands remain blank. P2 and S2 Pro have documented pumps,
but neither the presence of a pump nor a LightBurn M8 radio button proves the
controller command/interface configuration. Configure the pump separately only
after verifying its documented control path. The presets do not claim automatic
pump, network, focus or accessory compatibility.

## Other ACMER candidates reviewed

- P1 S Pro 20W/36W/48W: ACMER publishes module-specific parameter sheets, but
  the inspected setup guide explicitly covers the 6W/10W configuration.
  Establish the larger modules' usable X/Y travel before copying dimensions.
- P1: its [separate LightBurn guide](https://drive.google.com/file/d/19KMmr51HAlTWslKghUjyGafR1uwKPksD/view)
  describes rear-left origin and 400 x 410 mm. Do not apply P1 S Pro's front-left
  defaults to this different model. Reconcile the exact stock revision and
  coordinate conventions before adding it.
- S1: the manufacturer's currently linked
  [English manual](https://drive.google.com/file/d/1owTeh_uJkgfUMpDTB3CmTOk8lb7etuHU/view)
  describes a 130 x 130 mm mini engraver with 2.5W/3.5W modules. That does not
  establish defaults for the separately reported S1 6W configuration.
- K1, P3 and X1 need their own control, module and geometry review. GE1 and
  fiber/galvo models must not be represented by a generic GRBL diode preset.
- The older ACMER S2 entry is retained unchanged. Its existing defaults do not
  establish new-model compatibility.

## Verification

Permanent catalog regressions exercise each new preset through real profile
application, workspace synchronization and planner/G-code generation. They
verify M4/S250 at 25% of S1000, bottom-left Y placement, M5 shutdown, and absence
of homing or pump commands. Connected `$30=255` preview/application coverage
includes every new preset. UI regressions cover explicit preview/apply and
the translated evidence/advisory text.

Physical USB connect/reconnect, laser-off jog/frame, vector/raster output,
pause/resume/cancel, homing and pumps have not been tested on these machines.
Keep documentation-based status until an owner supplies reviewed results under
the [preset validation process](../machine-presets.md).

Examined setup-file SHA-256 values:

| File | SHA-256 |
| --- | --- |
| P1 S Pro LightBurn guide | `dad117ca4e6a5fd468394e61724b9f399d34ccaa14329146fc602c045ba9142b` |
| P2 LightBurn guide | `f7d4d9ce67b7b8e15db972d82e9287cacd94069d82a4f1521b711fe3029630f0` |
| S2 Pro manual V1.1 | `c578710b59d1b3eef28e36d4af8707494c6cf3ce65606d03551a58460600d2fa` |
