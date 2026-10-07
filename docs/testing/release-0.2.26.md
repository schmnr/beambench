# 0.2.26 release candidate verification

Date: 2026-10-06. The release owner requested preparation and publication of the next version. The intended tag is `v0.2.26`; the tag records the exact release source after the preparation PR is merged. Cargo, npm and Tauri versions are 0.2.26. Network GRBL remains Experimental.

## Source verification

The application fixes were merged through [PR #68](https://github.com/schmnr/beambench/pull/68). All four CI jobs passed on its final source: Linux Rust build, workspace tests, Clippy and formatting; frontend tests, performance budgets, lint and production build; macOS serial initialization; Windows planning, machine-service and frontend regressions. [CI evidence](https://github.com/schmnr/beambench/actions/runs/37566594503).

The source tests exposed missing generic-font mappings on Linux. The corrected SVG importer uses bundled Liberation Sans when the requested generic mapping cannot resolve. All 27 local SVG import tests and a regression using an empty font database passed. The final CI run passed with that correction.

Local macOS release environment preflight passed: Developer ID identity, notarization credentials, updater credentials, Node 22 and both Rust architecture targets are available. Each platform packaging path must verify the exact tagged source and its final packages before publication.

## Smoke record

No physical laser is connected or operated for this release. Native package results are recorded in the final release-verification artifact after builds complete; a pending result below is not a pass.

| Area | Candidate GUI check | Result | Evidence / remaining work |
| --- | --- | --- | --- |
| Launch | Install/unpack, launch and create a project | Not run | Pending signed macOS candidate; native Windows/Linux desktops unavailable. |
| Project safety | New/Open/Close and unsaved work | Not run | Store/component regression suites passed; native GUI check pending. |
| Canvas safety | Replacement, clear, import and stale artwork | Not run | Document, node-tool and stale-response regressions passed; native GUI check pending. |
| Undo | Create/delete/transform/clear undo and redo | Not run | Automated edit/history tests passed; native GUI check pending. |
| Raster | Import, invert and generate Preview repeatedly | Not run | Planner/raster and frontend automated suites passed; native GUI check pending. |
| Raster output | Normal and negative output vs processed image | Not run | No physical controller; automated raster geometry coverage passed. |
| Preview | Vector/raster preview and cancellation | Not run | Automated cancellation coverage passed; native GUI check pending. |
| Placement | Workspace origins, Frame and output bounds | Not run | Automated placement/planning regressions passed; physical controller unavailable. |
| Start From | Absolute/current/user origin | Not run | Automated placement regressions passed; physical controller unavailable. |
| Material Test | Frame and Start bounds and anchor | Not run | Automated quality-test regressions passed; physical controller unavailable. |
| Machine state | Home/origin/jog/alarm/reconnect | Not run | Simulated controller and connection tests passed; physical controller unavailable. |
| Job lifecycle | Start/pause/resume/cancel/complete/double-start | Not run | Automated safety and lifecycle tests passed; physical controller unavailable. |
| Localization | Native language/menu/font checks | Not run | Locale and error-mapping tests passed; native GUI check pending. |
| Updates | Version, download and signature | Not run | Pending final artifact and manifest validation, then isolated updater smoke. |
| Packages | Names, launch and updater compatibility | Not run | Pending builds, signing and package audits. |

## Release decision

Ready to build candidates after the release preparation PR passes CI. Publish only after final artifacts, signatures, checksums, corresponding source and public URLs are verified. Keep unavailable native platforms and physical hardware checks explicitly recorded as Not run. Do not interpret automated simulated-controller tests as physical hardware validation.
