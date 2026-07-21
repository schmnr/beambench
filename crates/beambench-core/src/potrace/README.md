# Potrace-derived tracing code

The Rust files in this directory are a port and modification of Potrace 1.16.

Original work:

- Potrace: <https://potrace.sourceforge.net/>
- Copyright (C) 2001-2019 Peter Selinger
- License: GNU General Public License version 2 or, at your option, any later
  version (`GPL-2.0-or-later`)

Beam Bench modifications were made on 2026-04-16. The original C algorithm
was translated to Rust, its data structures and numeric operations were
adapted to Beam Bench, bitmap preparation and path conversion were integrated
with Beam Bench types, and Rust tests were added. The resulting files remain
licensed under GPL-2.0-or-later.

The complete GPL version 2 text is available at
[`LICENSES/GPL-2.0.txt`](../../../../LICENSES/GPL-2.0.txt). Potrace is a
trademark of Peter Selinger; this notice describes code provenance and does
not imply endorsement by the Potrace project.
