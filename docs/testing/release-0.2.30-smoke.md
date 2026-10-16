# Release 0.2.30 smoke record

- Version/tag: 0.2.30 / v0.2.30, resolving the commit containing this record.
- Date: October 15, 2026.
- Tester: Codex. The owner authorized final verification and publication.
- Scope: LightBurn libraries, complete material recipes, kerf compensation,
  startup layout restoration, Falcon2 Pro presets, and tooling updates.
- Hardware: no physical laser connected or operated.

## Results

| Area | Result | Evidence or required publication check |
| --- | --- | --- |
| Material recipes | Pass | Full ordered stacks round-trip through persistence; advanced fields survive library edits; applying creates fresh IDs and supports Undo; legacy JSON keeps partial-apply behavior |
| LightBurn import | Pass | Conversion, invalid/unsupported recipe rejection, limits and atomic persistence regressions; 159 public-library recipes and 160 sub-layers round-trip without skips |
| Import review | Pass | Desktop and 800 x 600 layouts, keyboard expansion and reviewed import/cancel behavior; native OS file-picker flow not exercised manually |
| Kerf planning | Pass | Twelve planner regressions cover signed and nested contours, winding, open paths, tabs, per-entry/pass isolation, invalid offsets, collapse/split failures and output bounds |
| Preview/output agreement | Pass | Compensated preview and GRBL points match for both Y origins; source artwork remains unchanged |
| Saved layout, feedback #56 | Pass | Delayed settings restoration across two launches retains panel widths/heights and tabs |
| Machine presets | Pass | Falcon2 Pro 22W/40W/60W source and identification regressions; documented USB GRBL defaults and bed sizes |
| Frontend | Pass | 2,789 tests, production build, lint and performance budget; existing TransformSection hook warning remains |
| Source CI before version bump | Pass | All four jobs at 2ab32ea47e2d820ca4490a851b9072fe539933cc, run 38844395282 |
| Release source CI | Pending | All four required jobs must pass before tagging |
| Localization | Pass, automated | All 23 app locales include import/recipe controls and preset descriptions; native menu/language-switch smoke not repeated manually |
| Packages and updater | Pending | Verify final signed artifacts, version, isolated macOS launch/project/preferences and older-version updater engine before stable publication |
| Windows/Linux packages | Pending | Exact-tag packaging source tests, signed Windows setup, final AppImage audit and CLI checks |
| Corresponding source | Pending | Exact-tag archive, dependency sources, notices and source digest verification |
| Physical laser, camera, fitted-part dimensions | Not run | Hardware unavailable; kerf and presets have software validation only |
| Native Windows/Linux/Intel macOS installation | Not run locally | Machines unavailable; platform CI and artifact audits supply recorded coverage |
| Unchanged project/raster/laser lifecycle manual matrix | Not run | Existing automated regressions run in source CI and packaging; no machine operated |

## Release decision

Build and distribution checks are publication gates. Record their final results
in `release-verification.json` alongside the release assets. Version 0.2.29
remains stable until the required 0.2.30 packages and metadata are verified.

Known dependency advisories remain documented in [dependency-security.md](../dependency-security.md):
one Braces advisory in development tooling and the incompatible GLib 0.18
native dependency. They are not dismissed. Imported material values require
machine-specific calibration; the [conversion limits](../lightburn-material-import.md)
and physical-validation limits remain public documentation.
