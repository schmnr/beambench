# Geometry dependency upgrade

Beam Bench now resolves `geo 0.31.0`, `i_overlay 4.0.7`, and `i_tree 0.16.0`.
The previous chain resolved `geo 0.29.3`, `i_overlay 1.9.4`, and `i_tree 0.8.3`.
All three paths through Beam Bench core, geo, and the nesting library use the
same updated overlay dependency. No affected i_tree version remains in Cargo.lock.

[RUSTSEC-2025-0165](https://rustsec.org/advisories/RUSTSEC-2025-0165.html)
affects i_tree versions below 0.10.0. A fresh OSV batch query reported no matches
for geo 0.31.0, i_overlay 4.0.7, i_tree 0.16.0, i_float 1.15.0,
i_shape 1.14.0, or i_key_sort 0.6.0. This is an advisory lookup, not a proof
that the packages contain no defects.

## Compatibility changes

The overlay upgrade changes its default contour direction. The direct boolean
and nesting calls explicitly retain clockwise outer contours and counterclockwise
holes. Beam Bench also keeps its existing even-odd fill and disables additional
result cleanup in its direct boolean calls. The geo simplification calls pass
their tolerance by value, as required by the new API.

The latest published u-nesting-d2 release checked, 0.9.0, still depends on
i_overlay 1.9. Upgrading that library alone would leave the vulnerable dependency.
The existing 0.3.1 implementation is therefore vendored, with only its overlay
requirement and two overlay call sites changed. The upstream source revision,
license, patch explanation, and removal condition are in
[the vendor patch note](../../vendor/u-nesting-d2/BEAMBENCH-PATCH.md).
The crate is a workspace member so normal workspace tests cover it.

The license-report generator now includes vendored dependencies in its inventory
and notice collection. THIRD_PARTY_LICENSES.md has been regenerated.

## Validation

- All 1,965 Beam Bench tests pass across core (923), planner (329), service (668), and streamer (45).
- All 160 default-feature nesting-library unit tests and 24 integration tests pass, bringing the total to 2,149 passing tests.
- The new regression verifies outer/hole traversal direction and area through both subtraction and even-odd normalization. Existing suites cover holes, offsets, masks, planning, nesting containment, padding, rotation, imported vectors, and overlap stress cases.
- Core/service and desktop compilation pass. Clippy completes successfully for core, planner, service, and the vendored nesting library, with warnings. This was not a warnings-as-errors run.
- Formatting and whitespace checks pass; the license report includes the vendored crate.

No frontend source changed, so the frontend suite was not rerun for this dependency upgrade.

The Linux glib advisory remains unresolved. This change does not modify the
GTK stack, updater workflow, or network API. No physical-controller or Linux/
Windows runtime tests were performed.
