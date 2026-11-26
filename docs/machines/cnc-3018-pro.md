# CNC 3018 Pro laser conversion with Awoopecker V3.1

Preset `cnc_3018_pro_awoopecker_v3_1`, version 1, covers the laser conversion
with working limit switches described in feedback #83. The owner supplied a
GRBL 1.1f CBEAM settings dump and reported running Beam Bench-generated jobs
through another sender. Physical testing of the new preset in Beam Bench is
pending. [Owner report, reviewed November 26, 2026](https://github.com/schmnr/beambench-feedback/issues/83)

| Preset setting | Evidence and scope |
| --- | --- |
| USB serial, 115200 baud, buffered GRBL | The report identifies a CH340 and a 128-byte receive buffer. Select the port assigned by the host. |
| 300 × 180 mm, 2000 mm/min XY maximum | The owner reported `$130=300`, `$131=180` and `$110=$111=2000`. The machine has 42 mm of Z travel. Laser jobs use XY by default. |
| Rear-left workspace origin, homing available | The owner observed the XY head position at the rear left after homing, with `$22=1`, `$23=3` and 1 mm pull-off. |
| Dynamic power (M4), S1000 | The owner reported `$30=1000`, `$31=0` and laser mode `$32=1`. Air assist and custom job commands have no documented configuration and default to empty. |

The owner measured machine coordinates of X −299, Y −179 after homing.
The preset describes the workspace orientation. It does not set a work offset.
Check that work zero corresponds to the intended rear-left origin before using
absolute placement, then frame with the laser off. **Return to Homed XY** uses
the measured position captured in the current connection. [Homing and return tests](../testing/return-to-homed-xy.md)

Automated coverage exercises preview and explicit application of this preset,
saved serial settings, workspace placement, and vector and raster output.
A generic GRBL banner or CH340 adapter cannot identify this conversion.
