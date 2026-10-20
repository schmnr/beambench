# AlgoLaser Alpha MK2 20W preset request

[Feedback #58](https://github.com/schmnr/beambench-feedback/issues/58), submitted
October 20, 2026, from Windows and Beam Bench 0.2.30, asks for an
"algolaser mk2 20w" preset and links the Alpha MK2 product page.

The envelope says `kind: connectivity`, but the feature and action are both
`machine_preset_request`. The report has no active profile, port, firmware,
connection events, controller traffic, warning, or error. Those fields are
deliberately absent from preset requests. This is evidence of a missing catalog
entry, not a reproduced handshake or job failure. Keep the request open.

## Manufacturer evidence

Reviewed October 20, 2026:

- [Alpha MK2 product specifications](https://algolaser.com/products/algolaser-alpha-mk2-diode-laser-engraver-algolaser)
  list a 400 x 410 mm workspace for the 20W model and support for LightBurn and
  LaserGRBL. The 40W model has different travel and must not share guessed
  dimensions with this request.
- [Alpha MK2 support](https://algolaser.com/pages/algolaser-alpha-mk2-machine-support)
  publishes separate 20W setup, Wi-Fi, software and firmware references.
  The landing page alone does not establish baud rate, power scale, origin,
  homing or supported air-assist commands.

There is no AlgoLaser preset in the current catalog. The existing GRBL adapter
is a possible basis, but a generic preset does not establish exact model
support. Follow [the preset evidence requirements](../machine-presets.md)
before adding an entry. The next useful evidence is the manufacturer's device
profile or exact controller settings, followed by owner validation.

The older [AlgoLaser job rejection report #30](https://github.com/schmnr/beambench-feedback/issues/30)
concerns a DIY KIT MK2. It established a successful TCP connection and a later
command rejection. That is a different model and is not proof that the Alpha
MK2 needs the same workaround. No controller or streaming change is justified
by #58's current diagnostics.
