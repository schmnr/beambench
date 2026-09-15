# PDF graphics state regression

`opaque-graphics-state.pdf` is synthetic test artwork created for this repository.
It contains a red triangle 72 points (25.4 mm) wide and a blue rectangle. Its page
inherits an `ExtGState` resource from the page tree, with full opacity, normal
blending, no soft mask, and ordinary line settings. It contains no customer data.

The import service tests read it as both PDF and PDF-compatible AI and verify
geometry, color layer routing, undo availability, and plan invalidation.
