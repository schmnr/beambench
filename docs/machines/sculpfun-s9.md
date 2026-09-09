# Sculpfun S9 preset evidence

- Preset: `sculpfun_s9`, version 1.
- Scope: original S9 5.5W, standard frame, GRBL 1.1f or newer, USB serial.
- Evidence level: Documentation-based. Physical Beam Bench testing is pending.
- Sources checked: 2026-09-08.
- Excluded configurations: S9 Pro, extension frames, replacement control boards,
  rotary setups, and assumptions about automatic air assist or added homing.

## Sources and defaults

| Setting | Default | Basis |
| --- | --- | --- |
| Workspace | X 410 mm, Y 415 mm | Current [manufacturer product page](https://www.sculpfun.com/products/sculpfun-s9-laser-engraver) |
| Connection | GRBL, serial, 115200 baud | Section 2 of the [S6/S9 manual](https://cdn.shopify.com/s/files/1/0628/0695/0066/files/User_Manual_of_SCULPFUN_S6_S9_series_8.31.pdf?v=1770011394), linked from the [manufacturer download center](https://www.sculpfun.com/pages/download-center) |
| Origin | Bottom left | [Manufacturer software setup guide](https://www.sculpfun.com/blogs/blog/setting-up-the-software), device settings |
| Power scale | S1000 represents 100% | Same setup guide, S-value maximum; check against the actual controller's `$30` |
| Power mode | Dynamic, M4 | GRBL 1.1f-or-newer scope, described by the setup guide; older firmware needs separate configuration |
| Homing | Off | Setup guide enables homing only when switches are fitted and firmware is configured |
| Automatic air assist | No on/off commands, no startup delay | Stock preset does not assume an automatic pump or relay; configure installed upgrades separately |
| Maximum profile speed | 3000 mm/min | Conservative Beam Bench starting value, not a manufacturer-rated maximum; controller limits still apply |
| Maximum power percentage | 100 | Full controller scale, not a recommended material-processing power |
| Transfer and custom commands | Buffered, no custom header or footer | Existing standard GRBL path; no vendor startup commands established |

The older combined S6/S9 manual lists 410 x 420 mm and uses S6 wording in much
of its content. The current S9 product page repeatedly lists 410 x 415 mm. This
preset deliberately starts with the smaller workspace. Owners should compare
usable travel with their controller and frame before increasing it. An extension
kit needs its own dimensions.

The software guide covers S6/S9/S10 setup and explains that homing and firmware
versions vary. This record does not assert that every S9 has the same board.
Automatic model suggestion is not added: a generic GRBL banner cannot establish
the exact machine, accessories, or work area.

## Validation

The automated S9 regression applies the preset to an active profile and bound
project, checks stock settings, and generates a small vector job through the
planner and GRBL emitter. It checks bottom-left placement, S250 at 25% power,
M4/M5, and absence of homing or coolant commands even when a layer requests air.
The catalog-wide test checks unique IDs, versions, valid machine settings, and
paired air commands. Existing profile UI tests cover reviewing and applying a
preset with explicit confirmation.

Software checks passed on 2026-09-08: 25 profile tests, 16 profile-dialog tests,
and 172 locale parity/extraction checks across 23 languages. TypeScript, Rust
formatting, targeted ESLint, issue-form YAML parsing, and local documentation
links passed. Service Clippy completed with existing warnings and no new
diagnostic signatures.

No physical S9 was connected for this change. A user reported successful use
with Generic GRBL, but provided no firmware/settings export or operation-level
test results. That report motivates the entry and does not promote this preset
to Community-tested.

| Physical check | Result |
| --- | --- |
| Connect and reconnect | Not run |
| Jog with laser off | Not run |
| Frame with laser off | Not run |
| Small vector job | Not run |
| Small raster job | Not run |
| Pause and resume | Not run |
| Cancel and recovery | Not run |
| Homing / automatic air assist | Outside the stock preset scope |

Use **Share machine test results** beside the app's preset picker, following the
[catalog contribution process](../machine-presets.md). No account is required;
the support page also offers email links. Record Beam Bench version, preset version,
OS, firmware, modifications, and the operations actually tested. Add reviewed
report links here, including unresolved failures.
