# Creality CR-Laser Falcon USB presets

The original CR-Laser Falcon 5W, 7.5W and 10W each have a version 1 preset.
These are documentation-based presets for stock frames and modules with
the CV-30 controller, using the USB serial GRBL adapter. Physical Beam Bench
testing is pending.
Sources checked November 7, 2026.

## Settings and sources

| Setting | Preset | Evidence |
| --- | --- | --- |
| Workspace | X 400 mm, Y 415 mm | Creality's [shared 5W, 7.5W and 10W manual](https://wiki.creality.com/en/laser-engraver/cr-laser-falcon/user-manual) specifies the same area for all three modules. The [10W product specification](https://www.creality.com/products/cr-laser-falcon-laser-engraver-10w) and the CR-Laser Falcon device configuration agree. |
| Controller and baud | GRBL over USB serial at 115200 | The manual covers USB-C and LightBurn/LaserGRBL. The [manufacturer's CR firmware package](https://cdn.shopify.com/s/files/1/0799/8714/7049/files/CR-Laser_Falcon_User_CV-30-Firmware_V3.0.24.zip?v=1704954781) contains a GRBL ESP32 build that identifies itself as `Grbl 1.3a`. A [public CR-Laser Falcon LightBurn configuration](https://gist.github.com/marcdurham/4168878f2a5f7b0a0b5ea121fa01094f) specifies GRBL, Serial and 115200 baud. That configuration is user-posted. |
| Origin | Front left | The LightBurn configuration uses no axis mirrors. Front left follows [LightBurn's documented GRBL convention](https://docs.lightburnsoftware.com/2.0/GetStarted/CoordinatesOriginBeginner/). Physical axis directions and zero position still need confirmation. |
| Power | S1000 initially, dynamic M4 | The CR-Laser Falcon configuration uses S1000 and LightBurn's variable-power GRBL driver. A [10W owner's controller readout](https://forum.lightburnsoftware.com/t/cant-correct-home-position/82293) independently reports `$30=1000` and `$32=1`. The connected controller's valid `$30` takes precedence. |
| Homing and air assist | Homing capability starts off. New profiles have automatic homing off. No automatic air commands. | The owner's readout shows that stock firmware can have `$22=1`, and that owner reported a homing fault. The preset sends no homing command or settings write. Verify the switches and controller before enabling homing. Creality lists a [separate air assist accessory](https://eu.crealityfalcon.com/products/cr-laser-falcon-air-assist) without establishing a stock software-controlled pump. |
| Transfer and speed | Buffered USB, no custom job commands, feed moves for overscan, initial profile limit of 6000 mm/min | The published LightBurn configuration uses buffered transfer, empty start/end code and no rapid overscan. 6000 mm/min is a Beam Bench starting limit. The live controller's axis settings can update it. |

Creality lists a CV-30 firmware package for the CR series and publishes the
same workspace and USB software setup across the three original models.
The initial S scale and serial settings use the shared CR configuration.
The controller readout above is for the 10W only. No vendor firmware or
LightBurn profile is included in Beam Bench. Extension frames, replacement
controllers, rotary attachments, offline TF-card operation and vendor
accessory controls are outside the scope of these presets.

The presets are selected manually. A GRBL banner or shared USB chip does not
identify the module wattage. Automated tests apply all three presets through
the normal profile workflow, generate GRBL from a test job, verify the
workspace and power mapping, and exercise explicit desktop preview and apply.
The hardware checks in [owner validation](../machine-presets.md#owner-validation)
are pending.

## Creality catalog audit

| Model | Beam Bench coverage |
| --- | --- |
| CR-Laser Falcon 5W, 7.5W and 10W | Added here. |
| Falcon A1 10W | Existing [A1 USB preset](creality-falcon-a1-10w.md), 381 x 305 mm. |
| Falcon2 12W, 22W and 40W | Existing [standard Falcon2 USB presets](common-grbl-usb-presets.md#standard-creality-falcon2), 400 x 415 mm. |
| Falcon2 Pro 22W, 40W and 60W | Existing [22W](creality-falcon2-pro-22w.md), [40W](creality-falcon2-pro-40w.md) and [60W](creality-falcon2-pro-60w.md) USB presets. Creality's [device files](https://www.crealityfalcon.com/products/falcon-2-pro-40w-enclosed-laser-engraver-and-cutter) give 400 x 415 mm for 22W/40W and 400 x 400 mm for 60W. |
| Falcon2 Pro 2-in-1 kits | The [2-in-1 kits](https://eu.crealityfalcon.com/products/creality-falcon2-pro-22w-40w-60w-2-in-1-laser-engraver-and-cutter) add 2W or 4W IR modules to the Pro. The existing presets cover the stock blue-diode configurations. The IR modules need separate validation. |
| Falcon Pro 10W | The [Pro manual](https://wiki.creality.com/en/laser-engraver/falcon-pro-10W/user-manual) gives 400 x 415 mm and USB. [Owners report separate CV30-Pro firmware](https://forum.creality.com/t/newbie-questions/35487). Verify the Pro's controller settings and pump behavior before adding a preset. |
| Falcon2 Pro S 22W and 40W | Creality's [22W](https://uk.crealityfalcon.com/products/falcon2-pro-s-22w-enclosed-diode-laser-engraver) and [40W](https://www.crealityfalcon.com/products/falcon2-pro-s-40w-enclosed-diode-laser-engraver) pages give a smaller 355 x 390 mm work area. Confirm serial defaults and how the autofocus and laser positioning affect the usable area. |
| Falcon A1C 5W and 10W | Creality's [USB device profile](https://wiki.creality.com/en/laser-engraver/falcon-a1c/lightburn-guide) gives GRBL, 115200 baud, S1000 and 155 x 155 mm. The [specification](https://www.crealityfalcon.com/products/creality-falcon-a1c-laser-engraver) gives 150 x 150 mm. The smaller area is a candidate once the physical origin is confirmed. |
| Falcon A1 Pro 20W/2W IR | Creality's [LightBurn profile and guide](https://wiki.creality.com/en/laser-engraver/falcon-a1-pro/lightburn-guide) use GRBL-LPC and model-specific homing and autofocus macros. Verify the controller's motion and power handling with Beam Bench's adapter before adding the dual-laser configuration. |
| Falcon T1 | The [LightBurn device profile](https://wiki.creality.com/en/laser-engraver/falcon-t1/lightburn-guide) selects GRBL-M3 and includes model-specific focus and Z macros. Confirm USB motion and power behavior for the intended head before adding a preset. |
| CV-01, CV-01 Pro and printer laser kits | Creality's [lineup guide](https://store.creality.com/blogs/all/creality-laser-engraver-series-buying-guide) treats these as separate machines and printer attachments. Each needs its own controller and frame evidence. |
