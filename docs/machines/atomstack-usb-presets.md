# AtomStack USB preset evidence

- Catalog: 36 exact configurations at preset version 1; 33 new entries and the
  three existing Pro V2 entries retained unchanged.
- Evidence level: Documentation-based. Physical Beam Bench testing is pending.
- Sources checked: November 11, 2026.
- Scope: stock frames and the named modules, using the existing GRBL 1.1 USB
  adapter. M4 requires compatible dynamic-power firmware and laser mode. These
  presets do not establish network, camera, autofocus, rotary or conveyor support.

## Exact configurations

All dimensions are X x Y in millimetres. A blank air setting means that Beam
Bench sends no automatic pump commands. Wattages identify the named model;
original A5 input ratings are explicitly distinguished from optical output.

| Preset ID | Model | Workspace | USB baud | Power mode | Air on/off |
| --- | --- | --- | --- | --- | --- |
| `atomstack_swift_7w` | Swift 7W | 300 x 300 | 460800 | M4 | None |
| `atomstack_swift_12w` | Swift 12W | 300 x 300 | 460800 | M4 | None |
| `atomstack_a5_10w` | A5 10W, original input rating | 410 x 400 | 115200 | M3 | None |
| `atomstack_a5_15w` | A5 15W, original input rating | 410 x 400 | 115200 | M3 | None |
| `atomstack_a5_20w` | A5 20W, original input rating | 410 x 400 | 115200 | M3 | None |
| `atomstack_a5_30w` | A5 30W, original input rating | 410 x 400 | 115200 | M3 | None |
| `atomstack_a5_pro` | A5 Pro, original | 410 x 400 | 115200 | M3 | None |
| `atomstack_a5_pro_plus` | A5 Pro+ | 410 x 400 | 115200 | M3 | None |
| `atomstack_a5_m30` | A5 M30 | 410 x 400 | 115200 | M3 | None |
| `atomstack_a5_m40` | A5 M40 | 410 x 400 | 115200 | M3 | None |
| `atomstack_a5_m50` | A5 M50 | 410 x 400 | 115200 | M3 | None |
| `atomstack_a5_m50_pro` | A5 M50 Pro | 410 x 400 | 115200 | M3 | None |
| `atomstack_a5_pro_v2` | A5 Pro V2 | 400 x 365 | 115200 | M4 | None |
| `atomstack_a6_pro` | A6 Pro | 365 x 305 | 115200 | M4 | None |
| `atomstack_a12_pro` | A12 Pro | 365 x 305 | 115200 | M4 | None |
| `atomstack_a24_pro` | A24 Pro | 360 x 280 | 115200 | M4 | None |
| `atomstack_x12_pro` | X12 Pro | 365 x 305 | 115200 | M4 | None |
| `atomstack_a10_pro_v2` | A10 Pro V2, existing entry | 400 x 365 | 115200 | M4 | None |
| `atomstack_a20_pro_v2` | A20 Pro V2, existing entry | 400 x 365 | 115200 | M4 | None |
| `atomstack_a40_pro_v2` | A40 Pro V2, existing entry | 385 x 400 | 115200 | M4 | None |
| `atomstack_a20_pro` | A20 Pro, original | 400 x 400 | 115200 | M3 | None |
| `atomstack_a30_pro` | A30 Pro | 400 x 400 | 115200 | M3 | None |
| `atomstack_a40_pro` | A40 Pro, original | 400 x 400 | 115200 | M4 | M7 / M9 |
| `atomstack_x40_pro` | X40 Pro, original | 400 x 400 | 115200 | M4 | M7 / M9 |
| `atomstack_s40_pro` | S40 Pro, original | 400 x 400 | 115200 | M4 | M7 / M9 |
| `atomstack_a40_max` | A40 Max | 800 x 400 | 115200 | M4 | M7 / M9 |
| `atomstack_x40_max` | X40 Max | 800 x 400 | 115200 | M4 | M7 / M9 |
| `atomstack_s40_max` | S40 Max | 800 x 400 | 115200 | M4 | M7 / M9 |
| `atomstack_a70_pro` | A70 Pro | 500 x 400 | 115200 | M4 | M8 / M9 |
| `atomstack_a70_max` | A70 Max | 850 x 800 | 115200 | M4 | M8 / M9 |
| `atomstack_x70_max` | X70 Max | 850 x 800 | 115200 | M4 | None |
| `atomstack_p1_5w` | P1 5W blue diode | 110 x 110 | 460800 | M4 | None |
| `atomstack_kraft_lite_40w` | Kraft Lite 40W | 500 x 320 | 460800 | M4 | None |
| `atomstack_hurricane_55w` | Hurricane 55W CO2 | 500 x 300 | 460800 | M4 | None |
| `atomstack_glacier_90w` | Glacier 90W | 800 x 800 | 460800 | M4 | None |
| `atomstack_glacier_120w` | Glacier 120W | 800 x 800 | 460800 | M4 | None |

## Shared choices and connection behavior

All entries start with front-left/bottom-left coordinates, S1000, 100% maximum
controller-scale power, buffered transfer, no job header/footer, no per-G1 S
requirement and no G0 overscan. A valid connected controller `$30` takes
precedence when previewing and applying a preset. Verify `$32=1`; applying a
preset does not write firmware settings. M3/M4 are the existing adapter's
[GRBL laser modes](https://github.com/gnea/grbl/wiki/Grbl-v1.1-Laser-Mode).

The profile speed starts at 6000 mm/min, except the ten original A5 entries at
3000 mm/min. These are Beam Bench starting limits, not advertised maximum
speeds or material settings. Controller limits still apply.

Homing capability starts off pending the owner's hardware and firmware check.
New profiles do not automatically home. Applying a preset preserves an owner's
existing `home_on_connect` preference. No preset sends `$H` or disables hardware
safety systems. A device's own power-on homing remains independent. Verify its
reference before using absolute coordinates; Current Position is available for
appropriately positioned jobs without a homing reference.

Swift, P1, Kraft Lite, Hurricane and Glacier need 460800 rather than the 115200
used by the older AtomStack presets. Applying a preset now updates both the
profile's default baud and any remembered serial baud, and previews both changes.
The remembered port and controller selection are preserved. Network endpoints
and profiles without a remembered connection are unchanged. Existing saved
profiles require explicit preset application; upgrades do not silently rewrite
them.

Native discovery already supports 460800 and first probes at the selected baud
without resetting the board. Manufacturer LightBurn options such as
`EnableGrblJCommand=false` express that application's policy; they are not proof
that a firmware lacks GRBL jogging. No new protocol or jog fallback is claimed
from those options. Hardware connection, jogging and long jobs need owner tests.

## Manufacturer evidence

The [product support page](https://atomstack.com/pages/product-support) and
[machine guide](https://atomstack.com/pages/machine-guide) link the manuals and
profiles below. Historical QR support records are manufacturer-linked resources.
Only factual settings are extracted; vendor profiles, macros and manuals are
not bundled with Beam Bench.

### Swift

The [Swift FAQ](https://wiki.atomstack.com/en/swift/faq) identifies both 7W and
12W models, 300 x 300 mm travel and 460800 baud. The manufacturer's
[Swift device profile](https://cdn.shopify.com/s/files/1/0790/2586/4977/files/Device_SWIFT_V1.02.lbdev?v=1762832298)
specifies GRBL, Serial/USB, 460800, 300 x 300, S1000, front-left coordinates,
buffered transfer and no automatic homing or job header/footer. The
[usage guide](https://wiki.atomstack.com/en/swift/usage) covers physical controls
and focusing. Swift Mini is a different configuration and is excluded.

### Original A5 and M-series

The [A5 installation manual V2.1](https://ncstatic.clewm.net/rsrc/2021/1210/19/beb699503547fb7c994f911857e1447b.pdf)
explicitly names A5 10W/15W/20W/30W/Pro on its cover. These historical wattages
are electrical input labels, not a claim of that much optical power. The
[A5 user manual V2.1](https://ncstatic.clewm.net/rsrc/2021/1210/19/88f1ba6bf58859fac1075d7898186999.pdf),
PDF pages 10, 14 and 24, documents 115200, recommends M3 generally and M4 for
binary engraving, and sets up GRBL with 410 x 400 mm and front-left coordinates.

The manufacturer links the same
[M-series manual](https://ncstatic.clewm.net/rsrc/2021/1130/16/c7b4a7e65bc6aacb69ac4c4e6bebcec2.pdf)
from its M30, M40 and Pro+ support records. The
[M50 manual](https://ncstatic.clewm.net/rsrc/2022/0516/09/f666839f6f7fa3852801ff8e4ad7fe70.pdf),
PDF pages 13, 16 and 22–23, and
[M50 Pro manual](https://ncstatic.clewm.net/rsrc/2022/0516/10/ff4c303ee743ae08158ad132bd08a68b.pdf),
pages 18, 21 and 27–28, repeat the connection, power-mode and 410 x 400/front-left
setup. Their examples label S800 as 80%, consistent with S1000. Beam Bench
starts these entries with M3 and leaves automatic pump control blank.

### Pro V2, A6/A12/A24 Pro and X12 Pro

The [A5/A10/A20 Pro V2 manual](https://cdn.shopify.com/s/files/1/0790/2586/4977/files/A5_10_A20_PRO_V2_C_20240508_1_2__compressed.pdf?v=1728975233)
explicitly includes A5 Pro V2. Use the conservative 400 x 365 mm starting frame
from this V2 family; the [existing V2 evidence record](common-grbl-usb-presets.md#atomstack-v2)
explains the larger conflicting dimensions and A40 V2's separate 385 x 400 frame.
The manufacturer's
[GRBL setup guide](https://cdn.shopify.com/s/files/1/0790/2586/4977/files/LightBurn_Software_User_Manual.pdf?v=1715150625),
PDF pages 5–6, establishes USB, 115200, front-left, S1000 and buffered transfer.

The newer revision C [A6/A12/A24 Pro manual](https://ncstatic.clewm.net/rsrc/2024/0112/16/dadc6f8b1d48e0314658b6a55dcce350.pdf)
names all three models on its cover. PDF pages 10 and 16 give 365 x 305 mm,
115200 and front-left, with automatic homing off. An older revision shows
360 x 300 mm; owners of that revision must retain its smaller travel. The
[A24 Pro product specification](https://atomstack.com/products/atomstack-a24-pro)
also lists the smaller 360 x 280 mm, which is the selected A24 starting frame.
The [X12 Pro specification](https://atomstack.com/products/atomstack-x12-pro)
establishes its own 365 x 305 mm workspace and USB LightBurn/LaserGRBL support;
115200 and S1000 follow the manufacturer's GRBL setup guide.

### Original A20/A30 and A/X/S40 Pro/Max

The [A20 Pro manual](https://ncstatic.clewm.net/rsrc/2022/0706/14/daf2bfe5a0b91569d0abc273c9f18ce5.pdf)
and [A30 Pro manual](https://ncstatic.clewm.net/rsrc/2024/0112/10/33a25dd7fe62d5f031e1f6e78db9c4ec.pdf),
PDF pages 18, 21 and 27–28, document 115200, M3 for general output and front-left.
Their generic setup screenshot shows 410 x 400 mm; the original A20 is also
listed as 400 x 400 in manufacturer comparisons. Both presets use the smaller
400 x 400 starting frame. This is an explicit conservative choice, not evidence
that every A30 revision has exactly that maximum travel. Automatic pump control
stays blank rather than being inferred from a supplied external pump.

The [40 Pro manual](https://ncstatic.clewm.net/rsrc/2024/0112/10/25070bb642eb31a5f454e6965e1ea017.pdf)
explicitly co-lists X40 Pro, A40 Pro and S40 Pro. Its
[automatic-air instructions](https://ncstatic.clewm.net/rsrc/2024/0112/10/05743a625fc4f1e68069149d7dc79780.pdf),
PDF page 2, establish **M7**, 400 x 400 mm, front-left, S1000, 115200 and buffered
transfer. The [40 Max manual](https://ncstatic.clewm.net/rsrc/2024/0112/14/017f72a7fe26cb357fcbe8d58b56b792.pdf)
co-lists X40 Max, A40 Max and S40 Max; its
[air instructions](https://ncstatic.clewm.net/rsrc/2024/0112/14/8c2b4808641ab56e6d9c4b60de592249.pdf)
also require M7. The [X40 Max product specification](https://atomstack.com/products/atomstack-x40-max-210w-professional-grade-laser-engraving-and-cutting-machine)
sets the stock Max frame to 800 x 400 mm. These aliases have explicit common
manufacturer documentation. The Pro and Max families retain separate travel.
M9 turns off the documented GRBL coolant output; commands are emitted only for
cuts with air assist enabled. These settings do not apply to Pro V2.

### A70 Pro/Max and X70 Max

The [A70 FAQ](https://wiki.atomstack.com/en/A70/faq) distinguishes the 500 x 400 mm
Pro from the 850 x 800 mm Max. The
[combined manual](https://cdn.shopify.com/s/files/1/0790/2586/4977/files/A70_PRO_MAX_User_manual_-__compressed.pdf?v=1728976225),
PDF pages 22–23, specifies GRBL, USB, front-left, 115200 and no automatic homing.
The [A70 pump manual](https://ncstatic.clewm.net/rsrc/2024/0515/16/6ea9cffb4c3a7a6a954f3f5037c55e87.pdf),
PDF page 2, explicitly selects **M8**, S1000, 115200 and buffered transfer.
Its generic 410 x 410 screenshot is not used for the model's workspace. The
pump's physical switch must also be on for software control.

The [X70 Max specification](https://atomstack.com/products/atomstack-x70-max-360w-laser-engraver)
independently documents 850 x 800 mm and USB LightBurn/LaserGRBL support. The
manufacturer's general GRBL guide supplies 115200/S1000. Automatic air is blank
because the A70 pump document alone does not establish X70 wiring. Focus and
select any hardware power mode using the machine's controls first.

### P1, Kraft Lite and Hurricane

Manufacturer-published GRBL USB profiles specify exact connection and output
defaults for each of these configurations:

- [P1 profile](https://ncstatic.clewm.net/rsrc/2025/0911/10/a27deed7d7f542dea577b4f25c87510b.lbdev):
  460800, 110 x 110 mm, S1000, front-left, buffered and no automatic homing.
  The [P1 overview](https://wiki.atomstack.com/en/P1) distinguishes the blue
  5W module's 110 x 110 mm area from the IR module's 105 x 100 mm. Only blue is
  included. The [usage guide](https://wiki.atomstack.com/en/P1/usage) confirms baud.
- [Kraft 40W profile](https://cdn.shopify.com/s/files/1/0790/2586/4977/files/Kraft_Device_40File_V1.1.4.lbdev?v=1762496066):
  460800, 500 x 320 mm, S1000, front-left, buffered and no automatic homing.
  The [Kraft usage guide](https://wiki.atomstack.com/en/Kraft/usage) covers manual
  or physical-button focusing. The Lite 40W entry excludes the Dual model's
  blue/IR/combined switching macros.
- [Hurricane profile](https://ncstatic.clewm.net/rsrc/2025/1212/09/39c59d4928804fbcb6e34323bace2091.lbdev):
  GRBL despite this being a CO2 machine, 460800, 500 x 300 mm, S1000, front-left,
  buffered and no automatic homing. The
  [Hurricane usage guide](https://wiki.atomstack.com/en/hurricane/usage) documents
  460800 and physical focusing. Vendor Z, tube/water and other device macros
  are not copied or replaced by the base XY preset.

Focus with the machine's controls before framing. The presets do not configure
camera calibration, motorized Z, conveyor travel or rotary attachments. Keep
factory cooling and interlocks active. Automatic air is blank pending exact
model-specific command evidence.

### Glacier

The [Glacier overview](https://wiki.atomstack.com/en/Glacier) and FAQ give
800 x 800 mm for both 90W and 120W. Its
[USB connection guide](https://wiki.atomstack.com/en/Glacier/troubleshoot/connection-usb)
explicitly selects GRBL and 460800. The
[manufacturer device-settings image](https://wiki.atomstack.com/glacier/image3.png)
shows S1000, front-left and buffered transfer; it reuses a smaller device's
workspace, so Glacier travel comes from its own specifications instead.

The [usage guide](https://wiki.atomstack.com/en/Glacier/usage) requires software
power-mode selection. Select the desired optical power mode in AtomStack Studio
and close it before connecting Beam Bench. Both preset advisories state this
precondition. No vendor mode-switching commands are added to jobs. Use the
machine controls to focus first. Camera, chiller management, Z, conveyor and
rotary functions remain outside this base XY preset; existing interlocks must
stay active. A displayed OEM GRBL banner is not used to guess the exact module
or classify its firmware as stock gnea/grbl.

## Remaining candidates

Original A10 Pro, X7/S10 Pro, X20/S20, X30/S30 and A20 Max need their own revision
and settings evidence. Their names or shared USB chips do not establish aliases.
Swift Mini, Fusion, extension frames, Ultra revisions and replacement modules
also need separate records. Kraft Dual and P1 IR need their distinct optical-mode
workflows. Atelier/P2 galvo and 1064 nm fiber machines need verified protocols;
a generic GRBL preset does not establish support. iKier is outside this batch.

## Validation

Automated coverage applies all 36 entries through the real profile service,
workspace synchronization, planner and G-code emitter, including vector and PNG
grayscale raster output. It checks dimensions/orientation, M3/M4, S250 at 25%,
laser-off completion, exact M7/M8/M9 gating, no vendor headers and no homing.
Connected `$30` retention is checked for every entry.

A reproduced regression initially failed because applying Swift changed the
default baud but retained the old remembered USB baud. Tests now cover the
preview, persisted connection, unchanged port/driver/network preferences,
460800 discovery before reset/fallback, and desktop close/reopen/connect for
every 460800 entry. Desktop preset workflows and settings text cover all entries
and all 23 locales.

Physical connection, axis directions, measured framing, vector/raster output,
pause/resume/cancel and fitted accessories are **Not run** on these machines.
Use the [owner validation checklist](../machine-presets.md#owner-validation)
before upgrading an entry to Community-tested. No release or hardware test is
implied by passing the software suites.
