# Sculpfun S30 Pro 10W preset evidence

- Preset: `sculpfun_s30_pro_10w`, version 1.
- Scope: S30 Pro 10W, stock frame and controller, GRBL 1.1f or newer, USB serial.
- Evidence level: Documentation-based. Physical Beam Bench testing is pending.
- Sources checked: 2026-09-19.
- Requested in feedback report `r-f07720b25a154bf4befe0a4ed034e243`.
- Excluded: S30 5W, S30 Pro Max 20W, S30 Ultra, extension frames, replacement
  boards, upgraded laser modules, rotary attachments, and wireless accessories.

## Sources and defaults

| Setting | Default | Basis |
| --- | --- | --- |
| Workspace | X 380 mm, Y 385 mm | Current [manufacturer product specifications](https://www.sculpfun.com/products/sculpfun-s30-pro-laser-engraving-machine-10w) |
| Connection | GRBL, USB serial, 115200 baud | Product's USB connection; [Sculpfun software setup](https://www.sculpfun.com/blogs/blog/setting-up-the-software) selects GRBL and serial/USB; baud follows the [GRBL 1.1 protocol](https://github.com/gnea/grbl/wiki/Grbl-v1.1-Commands) |
| Origin | Bottom left | Sculpfun software setup, device settings |
| Power scale | S1000 represents 100% | Sculpfun software setup; compare with the actual controller's `$30` |
| Power mode | Dynamic, M4 | GRBL 1.1f-or-newer scope in the [manufacturer's settings guide](https://www.sculpfun.com/blogs/blog/settings-guide) |
| Homing | Off initially | Switches are supplied, but Sculpfun's software setup notes that S30 homing is often disabled in firmware. Enable after installation and firmware configuration. Applying the preset does not write controller settings. |
| Automatic air assist | M8 on, M9 off, no added delay | The exact model's product page specifies M8 and a supplied automatic pump. M9 is GRBL's coolant-off command. No required startup delay is established. |
| Maximum profile speed | 6000 mm/min | Beam Bench starting value, not a manufacturer-rated maximum. Controller motion limits still apply. |
| Maximum power percentage | 100 | Full controller scale, not a material-processing recommendation |
| Transfer and custom commands | Buffered, no custom header or footer | Existing GRBL path; no model-specific startup commands established |

The manufacturer's [older S30 series guide](https://www.sculpfun.com/blogs/blog/sculpfun-s30-series)
lists 410 x 400 mm for the 10W model. The current product specification lists
380 x 385 mm. This preset uses the smaller current specification. Owners should
check usable travel before increasing it. Extension kits need separate dimensions.

The [S30 assembly manual](https://cdn.shopify.com/s/files/1/0628/0695/0066/files/SCULPFUN_S30_Series_User_Manual_2.pdf?v=1780025023),
English page 10, shows the XY limit switches and air-pump wiring. Supplied
switches alone do not establish that an owner's homing cycle is configured.
The preset does not enable Home on connect or add a homing command to jobs.

Automatic suggestion requires an explicit Sculpfun S30 Pro 10W model string.
The existing Pro Max suggestion now requires its explicit 20W model string too.
Generic S30, GRBL, USB-chip identities, and other S30 variants do not identify
the correct preset. Users can select the preset manually when firmware provides
no exact model identity.

## Validation

The backend regression applies the preset to an active profile and its bound
project, checks workspace and controller defaults, and generates a rectangle
through the real planner and GRBL emitter. It checks bottom-left placement,
S250 at 25% power, M4/M5, M8 followed by M9, no homing command, and no M8 when
the layer's air assist is off. The existing S9 case remains in the same test.
Catalog checks cover unique IDs, versions, valid settings, and paired air commands.
Suggestion tests distinguish the 10W and 20W models and reject ambiguous names.
The profile-dialog regression covers preview, localized guidance, and application;
locale checks cover all 23 supported languages.

Software validation passed on 2026-09-19: 26 backend profile tests, 18 profile-dialog
tests, and 173 locale parity/extraction checks across 23 languages. TypeScript,
targeted ESLint, Rust formatting, diff whitespace, and local documentation links
passed. Service Clippy completed with existing warnings and no new diagnostics
in the preset or its tests.

No physical machine was connected or operated.
Connection, jogging, framing, vector and raster jobs, pause/resume, cancellation,
homing, and air assist still require an owner report before this preset can be
described as Community-tested. Use **Share machine test results** in the app,
following the [catalog process](../machine-presets.md).
