# Report #55 settings-save fix, October 14, 2026

Prepared in the working tree on the 0.2.28 source. Not released.
Report: https://github.com/schmnr/beambench-feedback/issues/55.

## Behavior

After a failed API startup, saving a layout or another unrelated preference
no longer retries that failed API configuration. The layout, theme and other
changes persist normally, even when Settings resubmits unchanged API fields.

Startup still attempts the saved API configuration and records failures.
Native appearance applies at startup even if the API cannot start. An actual
API configuration change still runs authentication and listener validation
before the service commits settings or the desktop applies a new theme.

The existing settings callback now receives the previous settings, or no
previous settings at startup. The API runtime compares its three persisted
fields: enabled, port and localhost-only. Preference import/reset rollback
passes the attempted settings as the previous state when restoring the original
configuration. No new persisted state, dependencies or frontend schema is added.

## Verification

The new desktop regression failed before the API-change guard was added, with
the same authentication error as the original report. It passes after the fix.
Its process has an explicitly empty API token and isolated test persistence.

The regression exercises both missing network authentication and an occupied
loopback port, then verifies:

- A layout, theme and autosave change saves successfully with unchanged API
  fields and is present after constructing a fresh service context from disk.
- No API listener starts as a side effect of those unrelated updates.
- A changed, invalid API configuration rejects the complete update without
  changing memory, settings bytes, theme callback state or emitted events.
- Disabling the API works, and changing to localhost-only starts a working
  loopback listener without requiring a network token.

A service regression also forces a preferences write failure after side
effects apply and verifies the reverse transition is used for rollback.

Commands and results:

```sh
cargo test -p beambench-tauri
cargo test -p beambench-service --lib ops::preferences::tests
cargo test -p beambench-service --lib ops::app::tests
cargo fmt --all -- --check
git diff --check
```

The full desktop suite passes all 50 tests, including existing API listener
rollback and settings-worker responsiveness cases. The new regression covers
the failed-startup state without opening an unauthenticated network listener.
All six preferences tests and all six app-settings tests pass. Formatting and
diff whitespace checks pass.
The [validation output](feedback-triage-2026-10-14/report55-fix-validation.txt)
records the failing regression before the guard and all passing suites afterward.

Validation ran on ARM64 macOS. Native Windows/Linux execution and physical
laser operation were not run; the changed settings/API code is shared across
desktop platforms. No production preferences were modified.

During release CI, the first Windows run exposed a nonportable test fixture:
Windows deliberately moves an existing destination aside, so a directory at
the settings-file path did not force a failure. The fixture now temporarily
places a file at the parent-directory path after backup/activation, forcing a
save failure on every OS. The Windows source and packaging checks now include
the desktop tests, so the #55 regression runs on the originally reported platform.
