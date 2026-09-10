# BlazeX M3 10W preset and homing

Preset `blazex_m3_10w`, version 1, covers the reported M3 10W with a
200 x 150 mm workspace. It is based on manufacturer documentation and an owner
configuration. Community operation reports will refine it; owning the hardware
is not a prerequisite for adding or correcting a preset.

## Evidence and defaults

Sources checked 2026-09-10:

- [Manufacturer M3 product page](https://blazexlaser.com/products/blazex-m3-laser-engraving-machine-for-wood-acrylic-leather-metal?variant=46999354081334)
  identifies the 10W option and LightBurn, CutLabX and LaserGRBL compatibility.
  Current marketing mixes M3 and M3 Pro names and advertises higher speeds;
  those claims do not override the submitted controller configuration.
- [Manufacturer M3 user manual, filed for FCC certification](https://fccid.io/m/03ddd75342f06cb8c7f0705811693c2583677bae57e3ec325f5ada8a39e769bc.pdf)
  covers USB and Wi-Fi operation and a rotary accessory.
- Owner-submitted diagnostics establish a working GRBL-family TCP connection
  on port 8080, firmware banner `Grbl 1.3a`, travel of 200 x 150 mm, S1000,
  bottom-left profile origin and a 7000 mm/min profile limit. The associated
  preset request identifies endstop switches and a rotary connector marked Z.
  Reporter identifiers, addresses and diagnostic bundles remain private.
- [GRBL command reference](https://github.com/gnea/grbl/blob/master/doc/markdown/commands.md)
  documents `$H`. [GRBL settings](https://github.com/gnea/grbl/blob/master/doc/markdown/settings.md)
  distinguish homing enable `$22` from direction `$23`.

| Setting | Choice and basis |
| --- | --- |
| Workspace | 200 x 150 mm, matching reported controller travel |
| Controller | GRBL family; do not infer grblHAL from an ESP32 or generic banner |
| USB | 115200 baud, standard GRBL starting configuration; TCP is the reported connection |
| Network | Owner's machine address, TCP port 8080 |
| Power | S1000, dynamic M4, matching the reported setup |
| Speed | 7000 mm/min starting profile limit, not a claim about maximum hardware speed |
| Origin | Bottom left, matching the reported profile; does not set switch direction |
| Homing | Available for reported endstop-equipped machine; controller `$22` is read on connection |
| Home on connect | Optional per-profile preference, off initially; not changed by preset application |
| Air assist | No automatic coolant commands; accessory control is not established |
| Z movement | Disabled; the reported connector serves a rotary accessory |
| Transfer | Standard buffered GRBL; no custom startup/footer commands |

## Homing correction

A GRBL controller can report 0,0 after power-on without knowing its physical
position. Beam Bench now explains that in the Move panel. Use Home after
connecting, or enable **Home on connect** in Device Settings for automatic homing
on each completed connection. It is available to GRBL profiles independently of
whether a named machine preset exists.

The profile's Homing checkbox describes configuration; it does not write
controller settings. Configure installed switches in the GRBL settings tab.
Automatic homing skips rotary mode and a controller reporting `$22=0`.
An explicit opt-in still permits OEM controllers that omit `$22` from their
settings response. Normal Home guards reject active jobs, unsupported controllers
and invalid motion states. Firmware decides the homing axes and switch direction;
Beam Bench does not derive `$23` from artwork origin or enable Z homing.

The option runs once during connection registration, not during UI refresh,
status polling, application hydration or emergency-stop recovery. It uses the
same `$H` dispatch and completion tracking as manual Home. Coordinates remain
unreferenced until the existing completion checks succeed. Exported/imported
profiles leave connection motion off so the receiving owner chooses it locally.

New reports include the controller's `$22` and `$23` separately from the saved
profile flag, when available. This makes community follow-up actionable without
requiring direct file transfers to the developer.

## Verification and community follow-up

Software regressions exercise opt-in behavior, startup Alarm, rotary exclusion,
controller-disabled homing, one dispatch per connection, coordinate validity,
profile persistence and preset defaults. No physical machine was operated.
Community checks: apply the preset, connect, Home, reconnect with Home on connect,
frame with the laser off, and run a small vector/raster test. Use the in-app bug
report or Share machine test results if an operation fails, with the machine
connected when possible.
