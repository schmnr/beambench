# Linux packaging incidents

Read this record before changing AppImage packaging or publishing a release.
Build on Ubuntu 22.04 to retain the GLIBC_2.35 floor, then start the exact signed
package on old and new distributions. Never infer startup from source tests,
`ldd`, a live launcher process or a substituted media probe alone.

## 0.2.14 to 0.2.15, Kubuntu graphics startup

The 0.2.15 hotfix removed bundled Wayland, GLib and GStreamer libraries after
they conflicted with newer host Mesa/WebKit libraries and caused
`EGL_BAD_PARAMETER`. The original 0.2.14 package contained 29 incompatible
libraries. The repair audit and dependency resolution passed on Ubuntu 22.04
and 26.04, but the release record explicitly marked native Kubuntu launch as
not run. See [PR 48](https://github.com/schmnr/beambench/pull/48) and
[the release record](releases/0.2.15.md).

## 0.2.31 and 0.2.32, failure before native logging

A [public Kubuntu report](https://www.facebook.com/groups/beambench/permalink/1523283879541164/)
said 0.2.30 started and subsequent releases did not. The owner identifies the
reporter as the same person affected by an earlier startup problem. The retained
0.2.15 release evidence confirms that earlier Kubuntu incident; it does not
independently establish the reporter's identity or Kubuntu version.

Commit `f51dd19f` added a desktop wrapper to restore GStreamer discovery after
linuxdeploy's ELF launcher modified the environment. The wrapper's
`#!/usr/bin/env bash` invokes the host's `env` with bundled `usr/lib` ahead of
host libraries. Ubuntu 26.04's rust-coreutils `env` requires `LIBSYSTEMD_254`,
but the AppImage's `libsystemd.so.0` exports only through `LIBSYSTEMD_252`.
The loader exits before Rust can create `diagnostics/startup.log`:

```text
/usr/bin/env: .../usr/lib/libsystemd.so.0: version `LIBSYSTEMD_254' not found
```

SHA-verified published 0.2.30, 0.2.31 and 0.2.32 packages were compared on
Ubuntu 22.04, 24.04 and 26.04. All three started on the two older bases. Only
0.2.30 started on 26.04. Removing bundled `libsystemd.so*` restored 0.2.31 and
0.2.32 without changing their desktop executables or glibc floor. A fully
repacked candidate reached `Frontend ready` on all three hosts in both X11 and
mixed Plasma/Wayland environments.

The existing media test substituted a host shell probe for the desktop binary.
That covered camera plugins and fonts, but did not prove that the actual
frontend could start. A clean deterministic audit also checked only libraries
already known to cause trouble. Both gaps allowed the package to pass.

## Required prevention and troubleshooting

- Audit and repair before signing. Reject bundled `libsystemd.so*` as well as
  the other host ABI libraries in `scripts/harden-linux-appimage`.
- Start the final signed package with `scripts/test-linux-appimage-desktop.py`.
  Keep all three host versions and both environments in the release workflow.
  Require frontend readiness and reject exits or timeouts. Retain camera,
  font, synthetic-frame and inherited update-relaunch tests separately.
- For a new startup report, compare the last working and first failing
  published packages on the same host. Check packaging changes before asking
  the reporter to reinstall runtime packages or edit environment variables.
- Capture terminal stderr even when `startup.log` is absent. Loader failures
  can occur before native logging starts. Keep the exact symbol error and
  inspect which bundled library overrides the host library.
- Record test limits. Containers use Xvfb, software rendering, fresh homes,
  no network and no physical laser or camera. Mixed Wayland variables exercise
  fallback policy, not native KWin/Wayland. Native checks remain separate.
- Correct published packages with a higher-version hotfix. Preserve immutable
  previous release files and advance stable only after final-artifact checks.
