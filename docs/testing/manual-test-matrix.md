# Beam Bench Manual Test Matrix

This is the master traceability index for Beam Bench manual testing.

- Source of truth for what must be tested manually
- Maps source documents and visible product surfaces to stable case IDs
- Separates software-only coverage from connected-machine and hardware-family coverage
- Tracks non-executable or unimplemented source rows explicitly as `Deferred`, `Unimplemented`, or `N/A`

Detailed procedures live in:

- [manual-test-cases-no-laser.md](manual-test-cases-no-laser.md)
- [manual-test-cases-with-laser.md](manual-test-cases-with-laser.md)

## Coverage Audit

Reviewed product inventories:

- Visible shell/runtime inventory:
  - `8` top-level menus
  - `6` toolbars/status surfaces
  - `11` panels
  - `18` dialogs

Manual case inventory in this suite:

- `NL-001` through `NL-074`: software-only executable cases
- `HW-001` through `HW-034`: connected-machine executable cases
- `CAM-001` through `CAM-007`: camera executable cases
- `DSP-001` through `DSP-005`: DSP-family executable cases
- `GAL-001` through `GAL-004`: galvo-family executable cases
- `GAP-001` through `GAP-012`: explicit unimplemented/deferred/N/A records
- `I18N-001` through `I18N-013`: i18n release smoke cases (pre-tag verification)

## Suite Summary

| Suite | Hardware Requirement | Count | Primary Document |
| --- | --- | ---: | --- |
| `NL` | `None` | 74 | [manual-test-cases-no-laser.md](manual-test-cases-no-laser.md) |
| `HW` | `Laser` | 34 | [manual-test-cases-with-laser.md](manual-test-cases-with-laser.md) |
| `CAM` | `Laser+Camera` | 7 | [manual-test-cases-with-laser.md](manual-test-cases-with-laser.md) |
| `DSP` | `DSP` | 5 | [manual-test-cases-with-laser.md](manual-test-cases-with-laser.md) |
| `GAL` | `Galvo` | 4 | [manual-test-cases-with-laser.md](manual-test-cases-with-laser.md) |
| `GAP` | `N/A` | 12 | This matrix |
| `I18N` | `None` | 13 | [i18n-release-smoke.md](i18n-release-smoke.md) |

## Coverage Summary By Subsystem

| Subsystem | Case IDs | Hardware Requirement |
| --- | --- | --- |
| Startup, shutdown, autosave, recovery | `NL-001` to `NL-007` | `None` |
| Menus, toolbars, panels, layout, dialogs | `NL-008` to `NL-022` | `None` |
| Project lifecycle, import, persistence | `NL-023` to `NL-028` | `None` |
| Canvas navigation, grid, snap, status readouts | `NL-029` to `NL-032` | `None` |
| Selection model and gating | `NL-033` to `NL-035` | `None` |
| Creation tools and non-machine tools | `NL-036` to `NL-040` | `None` |
| Layers, cut settings, transforms, arrange, properties | `NL-041` to `NL-049` | `None` |
| Text, barcode, variable text | `NL-050` to `NL-054` | `None` |
| Vector editing and path operations | `NL-055` to `NL-059` | `None` |
| Raster/image workflows | `NL-060` to `NL-061` | `None` |
| Import/export/preview | `NL-062` to `NL-064` | `None` |
| Libraries, macros, settings, diagnostics, shortcuts, accessibility | `NL-065` to `NL-074` | `None` |
| Profiles, discovery, connection, console, jog, positioning, job runtime | `HW-001` to `HW-034` | `Laser` |
| Camera capture, overlay, calibration, alignment | `CAM-001` to `CAM-007` | `Laser+Camera` |
| DSP capability/runtime verification | `DSP-001` to `DSP-005` | `DSP` |
| Galvo capability/runtime verification | `GAL-001` to `GAL-004` | `Galvo` |

## Feature Sweep Traceability

| Source Ref | Requirement Area | Mapping | Status |
| --- | --- | --- | --- |
| `FSW-001` | Build health | `GAP-009` | `N/A` automated gate |
| `FSW-002` | Project/file lifecycle | `NL-001`, `NL-005`, `NL-023` to `NL-028`, `NL-062` | Mapped |
| `FSW-003` | Recovery/autosave/diagnostics | `NL-003`, `NL-004`, `NL-007`, `NL-072` | Mapped |
| `FSW-004` | Layers/objects/history | `NL-033` to `NL-049`, `NL-055` to `NL-059`, `NL-073` | Mapped |
| `FSW-005` | Selection/transforms/snapping | `NL-029` to `NL-035`, `NL-045`, `NL-046` | Mapped |
| `FSW-006` | Vector editing core | `NL-055` to `NL-059` | Mapped |
| `FSW-007` | Arrays/offset/tabs/trim/radius/set-start | `NL-058`, `NL-059` | Mapped |
| `FSW-008` | Text editing | `NL-038`, `NL-050` | Mapped |
| `FSW-009` | Text on path | `NL-051` | Mapped |
| `FSW-010` | Barcode generation | `NL-052` | Mapped |
| `FSW-011` | Image/bitmap workflows | `NL-060`, `NL-061` | Mapped |
| `FSW-012` | Import: SVG/image/files | `NL-025`, `NL-026`, `NL-062` | Mapped |
| `FSW-013` | Import: DXF/PDF/AI/EPS and Lbrn `.lbrn`/`.lbrn2` projects | `NL-062` | Mapped |
| `FSW-014` | Export: SVG/DXF/PDF/GCode/EPS/AI | `NL-062`, `HW-024` | Mapped |
| `FSW-015` | Planning/preview/output | `NL-063`, `NL-064`, `HW-025`, `HW-026` | Mapped |
| `FSW-016` | Materials library | `NL-065`, `NL-066` | Mapped |
| `FSW-017` | Macros and console | `NL-069`, `NL-070`, `HW-013`, `HW-014` | Mapped |
| `FSW-018` | Device settings and profiles | `HW-001`, `HW-002`, `DSP-004`, `DSP-005`, `GAL-004` | Mapped |
| `FSW-019` | Machine session and jobs | `HW-003`, `HW-007` to `HW-034`, `DSP-005` | Mapped |
| `FSW-020` | Discovery and profile bootstrap | `HW-005`, `HW-006`, `DSP-002`, `GAL-002` | Mapped |
| `FSW-021` | DSP/Galvo runtime support | `DSP-001` to `DSP-005`, `GAL-001` to `GAL-004` | Mapped |
| `FSW-022` | Camera calibration/alignment/overlay | `CAM-001` to `CAM-007` | Mapped |
| `FSW-023` | Eventing and observability | `GAP-009` | `N/A` headless/automated |
| `FSW-024` | Desktop shell/menus/toolbars/panels/shortcuts | `NL-008` to `NL-022`, `NL-073`, `NL-074` | Mapped |
| `FSW-025` | API/CLI headless parity | `GAP-009` | `N/A` headless/automated |
| `FSW-026` | Beginner Mode | `GAP-001` | Unimplemented |
| `FSW-027` | Multi-language support | `GAP-002` | Unimplemented |
| `FSW-028` | Hotkey customization | `GAP-003` | Unimplemented |
| `FSW-029` | Settings bundles / backup restore | `GAP-004` | Unimplemented |
| `FSW-030` | Nesting | `GAP-005` | Unimplemented |
| `FSW-031` | Material test / focus test / interval test | `GAP-006`, `GAP-007`, `GAP-008` | Deferred/incomplete |
| `FSW-032` | Rotary and cylinder workflows | `GAL-004`, `GAP-012` | Partial |
| `FSW-033` | Accessibility | `NL-074` | Mapped |

## Visible Surface Inventory

### Menus

| Surface | Mapping |
| --- | --- |
| `File` | `NL-008`, `NL-023` to `NL-028`, `NL-062`, `NL-071`, `NL-072` |
| `Edit` | `NL-009`, `NL-033` to `NL-059`, `NL-073` |
| `View` | `NL-010`, `NL-029` to `NL-032`, `NL-063`, `NL-064` |
| `Tools` | `NL-011`, `NL-036` to `NL-061` |
| `Arrange` | `NL-011`, `NL-046` to `NL-059` |
| `Machine` | `HW-001` to `HW-034` |
| `Laser Tools` | `NL-012`, `GAP-006`, `GAP-007`, `GAP-008` |
| `Help` | `NL-012`, `NL-017`, `NL-072` |
| `Window` | `NL-010`, `NL-013` to `NL-016` |

### Toolbars And Status

| Surface | Mapping |
| --- | --- |
| `MainToolbar` | `NL-018`, `NL-046`, `NL-047` |
| `CreationToolbar` | `NL-019`, `NL-036` to `NL-040` |
| `ModifiersToolbar` | `NL-020`, `NL-055` to `NL-059` |
| `NodeSubToolbar` | `NL-021`, `NL-057`, `NL-058` |
| `PropertiesToolbar` | `NL-046`, `NL-048` to `NL-050`, `NL-053`, `NL-054` |
| `StatusBar` | `NL-022`, `NL-031`, `NL-032`, `HW-008`, `HW-031` |

### Panels

| Surface | Mapping |
| --- | --- |
| `Cuts / Layers` | `NL-041` to `NL-044` |
| `Move` | `NL-046`, `HW-020` to `HW-023` |
| `Console` | `HW-013`, `HW-034` |
| `Macros` | `NL-069`, `NL-070`, `HW-014` |
| `Shape Properties` | `NL-048` to `NL-050` |
| `Laser` | `HW-015` to `HW-031` |
| `Material Library` | `NL-065`, `NL-066` |
| `Color Palette` | `NL-013`, `NL-015`, `NL-031` |
| `Camera` | `CAM-001` to `CAM-007` |
| `Variable Text` | `NL-053`, `NL-054` |
| `Art Library` | `NL-067`, `NL-068` |

### Dialogs

| Surface | Mapping |
| --- | --- |
| `AdjustImageDialog` | `NL-061` |
| `BarcodeDialog` | `NL-052` |
| `CameraAlignmentDialog` | `CAM-005` |
| `CameraCalibrationDialog` | `CAM-004` |
| `CircularArrayDialog` | `NL-058` |
| `CopyAlongPathDialog` | `NL-059` |
| `DeviceSettingsDialog` | `HW-002`, `DSP-004`, `GAL-004` |
| `DockDialog` | `NL-059` |
| `FocusTestDialog` | `GAP-007` |
| `GridArrayDialog` | `NL-058` |
| `IntervalTestDialog` | `GAP-008` |
| `MaterialTestDialog` | `GAP-006` |
| `NotesDialog` | `NL-017`, `NL-071` |
| `OffsetDialog` | `NL-058` |
| `PreviewWindow` | `NL-016`, `NL-064` |
| `QualityTestShell` | `GAP-006`, `GAP-007`, `GAP-008` |
| `ResizeSlotsDialog` | `NL-059` |
| `TraceImageDialog` | `NL-061` |

## Gap Inventory

| Gap ID | Description | Source Ref | Status |
| --- | --- | --- | --- |
| `GAP-001` | Beginner Mode is not implemented | `FSW-026` | Unimplemented |
| `GAP-002` | Localization / multi-language support is not implemented | `FSW-027` | Unimplemented |
| `GAP-003` | User-customizable hotkeys are not implemented | `FSW-028` | Unimplemented |
| `GAP-004` | Settings bundles / manual settings backup-restore / preference-pack workflows are not implemented | `FSW-029`, `LB 16`, `LB 19.*` | Unimplemented |
| `GAP-005` | Nesting workflow is not implemented | `FSW-030` | Unimplemented |
| `GAP-006` | Material Test dialog exists but the full product workflow is deferred/incomplete | `FSW-031`, `LB 11` | Deferred |
| `GAP-007` | Focus Test dialog exists but the full product workflow is deferred/incomplete | `FSW-031`, `LB 11` | Deferred |
| `GAP-008` | Interval Test dialog exists but the full product workflow is deferred/incomplete | `FSW-031`, `LB 11` | Deferred |
| `GAP-009` | Build health, eventing, API/CLI parity, and summary rows are not manual desktop cases; they remain automated/headless verification | `FSW-001`, `FSW-023`, `FSW-025`, `LB Summary Counts` | `N/A` |
| `GAP-010` | File parity gaps remain for unsupported formats or features: run saved machine files, PLT/HPGL import, raster workspace export, print, new window | `LB 1.1`, `LB 1.5`, `LB 2.5` | Missing |
| `GAP-011` | Selection/shape parity gaps remain for tab-cycle, contained-size filters, immediate post-create handles, center-out creation, and shape-specific handles | `LB 1.2`, `LB 1.4`, `LB 2.2` | Missing |
| `GAP-012` | Advanced capability-specific gaps remain for rotary/cylinder workflows and some controller/detail settings despite partial runtime support | `FSW-032`, `LB 12`, `LB 20.*`, `LB 21.*` | Partial |

## Unmapped Rows

No silent omissions remain at the matrix level.

Handling rule for anything not executable today:

- `Deferred` items are mapped to `GAP-*`
- `Unimplemented` items are mapped to `GAP-*`
- headless or automated-only items are marked `N/A`
- informational summary rows are marked `N/A`

There are no applicable implemented or runtime-reachable source areas left intentionally unmapped.
