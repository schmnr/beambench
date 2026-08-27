# Release Smoke Record

Copy this file for each release candidate and commit the completed record with
the release source. A result may be `Pass`, `Fail`, or `Not run`. `Not run` is
acceptable when hardware or a platform is unavailable, but it must be explicit.

## Build

- Version / tag:
- Commit:
- Date:
- Tester:
- Platforms exercised:
- Hardware exercised:

## Results

| Area | Check | Result | Notes / evidence |
| --- | --- | --- | --- |
| Launch | Install or unpack the candidate, launch it, and create a project |  |  |
| Project safety | New, Open, Close, and recent-project actions preserve or prompt for unsaved work |  |  |
| Canvas safety | Clear/Delete All, New, Open, Close, recovery, and import do not flash stale artwork or restore deleted artwork |  |  |
| Undo | Undo and redo representative create, delete, transform, and clear operations |  |  |
| Raster | Import an image, invert it in Adjust Image, generate Preview, close Preview, and repeat |  |  |
| Raster output | Confirm normal and negative output match the visible processed image |  |  |
| Preview | Generate vector and raster previews twice and cancel one generation |  |  |
| Placement | With each workspace origin, compare artwork bounds, Frame, and generated output |  |  |
| Start From | Compare Absolute Coordinates, Current Position, and User Origin using the same artwork |  |  |
| Material Test | For each Start From mode, confirm Frame and Start use the same bounds and anchor |  |  |
| Machine state | Home, set user origin, go to origin, jog, alarm, unlock, disconnect, and reconnect |  |  |
| Job lifecycle | Start, pause, resume, cancel, complete, and attempt a blocked double-start |  |  |
| Localization | Run the applicable checks in `i18n-release-smoke.md` |  |  |
| Updates | Confirm the version, updater manifest target, download, and signature |  |  |
| Packages | Check artifact names, sizes, launch, and updater compatibility on each published platform |  |  |

## Release decision

- Automated checks:
- Known failures:
- Checks not run and why:
- Decision:
