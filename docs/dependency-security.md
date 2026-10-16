# Dependency security status

Reviewed on 2026-10-15. This records remaining dependency work; it does not
dismiss or suppress advisories.

The frontend uses Vitest 5, which removes the vulnerable Tinypool dependency
and updates `@vitest/mocker`. The lockfile also updates `source-map-js` and
`brace-expansion`. An npm override selects patched `postcss-selector-parser`
7.1.6 for the existing Tailwind 3 build, avoiding an unrelated CSS migration.
Changes are validated with the full frontend suite, performance budgets, lint,
and a production build. Vitest migration details are documented in its
[migration guide](https://vitest.dev/guide/migration/).

`npm audit --omit=dev` reports no vulnerabilities. The full audit still reports
five high-severity package findings from one underlying advisory in the build
dependency chain. The native Linux dependency has a separate advisory below.

## Braces in development tooling

[GHSA-vfj7-8cjw-p6xm](https://github.com/advisories/GHSA-vfj7-8cjw-p6xm)
affects `braces` through 3.0.3, the newest published version checked during this
review. Tailwind 3 depends on it through Chokidar, Fast Glob, and Micromatch.
These are development dependencies, not frontend runtime packages.

The five npm findings are `braces`, `chokidar`, `fast-glob`, `micromatch`, and
`tailwindcss`; they are not five independent vulnerabilities. Recheck for an
upstream patch. If none is available, removing this chain requires a reviewed
Tailwind 4 migration with stylesheet and desktop rendering validation. Do not
merge the failing automatic Tailwind 4 update without that migration.

## GLib in the Linux desktop runtime

[GHSA-wrw7-89jp-8q8g](https://github.com/advisories/GHSA-wrw7-89jp-8q8g)
affects `glib::VariantStrIter` in GLib Rust bindings before 0.20.0. The locked
Linux dependency is `glib` 0.18.5, reached through GTK 0.18 and Tauri's native
menu/window/webview stack. It is not an npm dependency.

The fixed 0.20 line is incompatible with GTK's current 0.18 dependency
requirement. Track the upstream GTK/Tauri dependency transition or assess a
maintained backport separately. Do not force a mismatched GLib version into the
native graph or dismiss the alert without addressing the affected code.

## Rechecking

```sh
cd tauri-app
npm audit
npm audit --omit=dev

# From the repository root:
cargo tree --locked --target x86_64-unknown-linux-gnu -i glib
```

GitHub dependency alerts and npm audit can update on different schedules.
Check both after lockfile changes; a clean production npm audit does not cover
native Rust dependencies.
