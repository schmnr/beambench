# AlgoLaser Alpha MK2 20W

Preset `algolaser_alpha_mk2_20w`, version 1, covers the stock 20W flatbed over USB. The documentation below was reviewed on November 26, 2026. Physical testing in Beam Bench is pending. [Feedback #58](https://github.com/schmnr/beambench-feedback/issues/58)

| Preset setting | Evidence and choice |
| --- | --- |
| 400 × 410 mm, front-left origin | The product table gives X and Y travel by module. The setup guide identifies the front-left origin. The 40W module has different travel. [Model specifications](https://algolaser.com/products/algolaser-alpha-mk2-diode-laser-engraver-algolaser) · [Alpha MK2 setup guide](https://algolaser.com/blogs/how-to/connect-lightburn-with-algolaser-alpha-mk2-laser-engraver) |
| 30,000 mm/min engraving limit | AlgoLaser lists 30,000 mm/min for image engraving and 40,000 mm/min for travel on the 20W. Beam Bench uses the engraving figure for the profile speed limit. [Model specifications](https://algolaser.com/products/algolaser-alpha-mk2-diode-laser-engraver-algolaser) |
| GRBL USB, 115200 baud | The software setup in the Alpha MK2 manual shows the USB serial baud rate. The preset uses Beam Bench’s standard buffered GRBL sender. [Alpha MK2 manual, pp. 27–28](https://manuals.plus/m/41d2eb6c6d79639f3f52296647ec0f5e702ee191b29e7e4985b6972f86c58c04.pdf#page=28) |
| Homing available | The 20W beginner guide instructs the operator to home and frame. A new Beam Bench profile leaves homing on connect off. [20W beginner guide](https://docs.google.com/document/d/1TbH4UNg5bNcAu6CdPytjbJRpBsvThkAd/edit) |
| S1000, dynamic power (M4), M8/M9 air assist | In the manufacturer’s AlgoOS Material Files, `Aircraft_20W_Size149x70_3mmPlywood.nc` uses S1000 for a 100% cut and switches air with M8/M9. The 60% engraving in `animal_abstract_20W_Size100x77_MDF.nc` uses M4 and reaches S600. The preset uses M4 by default and sends air commands when the layer requests air. [Alpha MK2 support](https://algolaser.com/pages/algolaser-alpha-mk2-machine-support) · [AlgoOS material files](https://drive.google.com/file/d/1PNrIHcaSQQsxjQGuODuaDjRSbO_-sGov/view) |

The preset adds no custom job header, footer or pump delay. Check the connected controller’s power scale (`$30`) and laser mode (`$32`) when setting up. The profile tests cover workspace placement, power mapping, vector and raster output, and air commands with air enabled and disabled. USB connection, motion and pump operation on a physical Alpha MK2 20W still need owner validation.
