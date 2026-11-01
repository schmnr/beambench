# Creality Falcon A1 10W preset evidence

- Preset: `creality_falcon_a1_10w`, version 1.
- Scope: stock Falcon A1 10W through its USB serial GRBL connection.
- Evidence level: Documentation-based. Physical Beam Bench testing is pending.
- Sources checked: 2026-11-01. Requested in private feedback issue #81.
- Excluded: A1 Pro 20W, A1C, Falcon2, changed controllers/modules, rotary
  accessories, proprietary camera calibration and vendor application features.

## Sources and defaults

Creality's [A1 product page](https://www.crealityfalcon.com/products/falcon-a1-10w-enclosed-laser-engraver-and-cutter-all-in-one-kit)
links its [LightBurn device profile](https://www.crealityfalcon.com/cdn/shop/files/Creality_Falcon_A1_LightBurn_Device_File.lbdev?v=7882378004817155979).
The downloaded profile's SHA-256 is
`4a0602f340da64e645269fa34b8ecc659d7ccaa43bac9fb27cc7648e57a2e770`.
Only factual settings are extracted; the vendor file and camera matrices are
not redistributed. The [manufacturer's connection guide](https://wiki.creality.com/zh/laser-engraver/falcon-a1/random-data/connection-failed)
distinguishes USB-only A1 from Wi-Fi-equipped A1 Pro/A1C models.

| Setting | Preset | Basis |
| --- | --- | --- |
| Workspace | X 381 mm, Y 305 mm | Device profile `Width=381`, `Height=305`. Product copy lists 305 x 381 without identifying axes; the device profile resolves that ordering. |
| Controller/transport | GRBL, USB serial, 115200 baud | Device profile `Name=GRBL`, `Type=Serial`, `BaudRate=115200`. No Wi-Fi endpoint is inferred. |
| Origin | Bottom left | Positive workspace, no X/Y mirrors or axis swap in the profile; existing GRBL laser convention. Physical orientation needs confirmation. |
| Power scale | S1000 initially | Profile `S_Scale=1000`. The connected controller's actual `$30` remains authoritative. Optical 10W is not the S scale. |
| Power mode | Dynamic M4 | Manufacturer selects LightBurn's GRBL driver, not GRBL-M3. LightBurn's [GRBL configuration guide](https://docs.lightburnsoftware.com/2.0/Guides/GRBLConfiguration/) identifies that driver with GRBL 1.1f+ variable power. Verify firmware and `$32` on connection. |
| Homing | Off; no automatic home | Profile `HomeOnStartup=false`; conservative Beam Bench capability default. Applying a preset neither writes `$22` nor runs `$H`. |
| Automatic air | None | `AirAssistM7=false` does not establish that the pump accepts M8. Unknown commands remain disabled. |
| Profile maximum speed | 6000 mm/min | Conservative Beam Bench starting value, not the manufacturer's maximum. Simulation speed/acceleration fields are not imported as physical limits. |
| Transfer and output | Buffered; no custom header/footer; no forced S on every G1; feed moves for overscan | Existing GRBL adapter and profile `TransferMode=0`, empty start/end G-code, `ForceSValueOutput=false`, `UseG0ForOverscan=false`. |

The vendor file has `EnableGrblJCommand=false`. LightBurn documents this as its
[continuous-jog option](https://docs.lightburnsoftware.com/2.1/Reference/DeviceSettings/BasicSettings/),
not a declaration of firmware command support. Beam Bench retains its GRBL
1.1 `$J` adapter; physical jog/cancel behavior is unverified. This preset does
not add a legacy jogging implementation or copy laser-on framing preferences.
Z, rotary, camera offsets and automatic accessory commands stay disabled.

The preset is selected explicitly in the picker. Generic GRBL banners, USB
chip identities and ambiguous Falcon model names cannot select it automatically.
Existing saved profiles are unchanged until the owner explicitly applies it.

## Validation

Shared software regressions apply this preset to a saved profile and bound
project, then use the real planner/emitter. They check the 381 x 305 workspace,
bottom-left placement, 25% mapping to S250, M4/M5, and absence of homing/air
commands. A connected mock reporting `$30=255` retains that value when the
preset is previewed or applied. The description uses the existing translated
documentation-based evidence wording in all 23 languages.

No physical A1 was connected. Connection/reconnection, laser-off jog and frame,
vector/raster output, pause/resume and cancellation remain Not run. Camera,
enclosure controls and vendor cloud/application integration are outside this
preset's scope. An owner report can establish additional coverage.
