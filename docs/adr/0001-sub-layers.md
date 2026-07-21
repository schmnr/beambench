## M2 ADR — Sub-Layers

Date: 2026-04-17

### Decision

Beam Bench layers use a multi-entry cut-settings model:

- `Layer` is a shell with layer identity and display metadata only.
- `Layer.entries: Vec<CutEntry>` owns cut settings.
- `entries.len()` is constrained to `1..=11`.

This is a schema break with no backward-compat wrapper. The project is still pre-release, so the model should be corrected now instead of carrying a permanent compatibility shape.

### Locked UI scope

M2 ships end-to-end with a minimal stacked-list editor:

- sub-layers are edited as collapsible cards
- reorder uses explicit up/down buttons
- both `LayerSettingsPanel` and `CutSettingsEditor` host the same shared control
- no drag-and-drop library
- no tabbed multi-editor in M2

### Compatibility rules during migration

- `entries[0]` is the compatibility source of truth for legacy single-operation surfaces that are not yet sub-layer-aware.
- Event payloads that previously reported one layer operation continue to emit `operation = entries[0].operation` and add `entries_count`.
- Legacy "change layer mode" behavior updates `entries[0]` only.
- Layer-family bucketing also reads `entries[0].operation` in M2.

These are intentional compatibility bridges, not the final long-term multi-entry semantics for every surface.

### Planner behavior

- The planner reinterprets the same object set once per cut entry.
- Dispatch is driven by `CutEntry.operation`, not by the old one-operation-per-layer assumption.
- Entries execute strictly left-to-right.
- `output_enabled = false` suppresses emission for that entry.
- `cut_entry_id` is attached to vector and raster plan segments so downstream progress can distinguish phases within one layer.

### Routing semantics

For service-layer routing/validation:

- `NeedsImage` means any entry on the layer is `Image`.
- `NeedsNonImage` means no entry on the layer is `Image`.

This keeps the mixed-content invariant conservative while the rest of the app migrates to entry-aware behavior.

### Material presets

In M2, material presets remain layer-targeted:

- presets apply to `entries[0]`
- multi-entry layers return a warning payload indicating only the primary entry was targeted

Per-entry preset targeting is deferred to M3 and should be tracked in `BUG_GAP_LOG.md` as follow-up work.
