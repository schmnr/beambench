# Fix validation evidence

See [the release record](../feedback-fixes-2026-10-11.md) for implementation scope
and limitations. `validation.json` records commands, overlapping test counts and
input hashes. Logs omit ANSI colors and replace the maintainer's checkout path.
The binary AppImage, font and manufacturer manuals are external inputs and are
not added to the repository.

## Linux renderer and package

The input is the official [0.2.26 AppImage](https://github.com/schmnr/beambench/releases/tag/v0.2.26),
SHA-256 `834ada69b7090bbb21c4a7c5280912144e6b3b7045290ee1d7b0b40322b5f5d6`.
The hostile test font is [Noto COLRv1 v2.048](https://fuchsia.googlesource.com/third_party/github.com/googlefonts/noto-emoji/+/refs/tags/v2.048/fonts/Noto-COLRv1.ttf),
SHA-256 `94c6c870925aaafd3fdda338fc4b2e4dfb74260c9aefa87d4aa8a145a993a5a2`.

Use an isolated Linux environment with GTK3, WebKitGTK 4.1 headers, gcc,
Fontconfig, Xvfb/xauth, Weston, Python3 and squashfs-tools. These checks used
Ubuntu 24.04 amd64 under emulation. Install the hostile font into that isolated
environment's font directory and refresh its cache. Extract a copy of the input
with `unsquashfs -o 944632`, preserving the original input file.

Compile [font-probe.c](font-probe.c) using:

```sh
gcc font-probe.c -o appdir/usr/bin/font-probe $(pkg-config --cflags --libs gtk+-3.0 webkit2gtk-4.1)
```

In this disposable extracted payload, back up `usr/bin/beambench-tauri`, then
replace it with the probe. Launch from the extracted `appdir` through `AppRun`,
so library and subprocess discovery use the release's complete environment.
With `FONTCONFIG_FILE=/etc/fonts/fonts.conf`, the probe terminates with the exact
COLRv1 assertion ([font-before.log](font-before.log)). With `FONTCONFIG_FILE`
pointing to the repository's `scripts/linux-appimage-fonts.conf`, the same probe
survives ([font-after.log](font-after.log)). Use `timeout 18 xvfb-run -a ./AppRun`.
Restore the backed-up application afterwards. No sandbox-disabling override is
used. Direct mixed-host-library probes that failed even on ASCII are excluded.

Repair a separate AppImage copy using `scripts/harden-linux-appimage`, then
audit with `--check`. [package-audit.log](package-audit.log) records the actual
repair. Extract its payload and run the normal application sequentially:

- X11 through Xvfb: [x11-startup.log](x11-startup.log).
- Native Wayland through headless Weston, with `DISPLAY` unset:
  [wayland-startup.log](wayland-startup.log).
- The final package through Weston, with `DISPLAY=:99999` and a valid
  `WAYLAND_DISPLAY`: [display-fallback-startup.log](display-fallback-startup.log).

Unset `GDK_BACKEND`, `FONTCONFIG_FILE` and `BEAMBENCH_APPIMAGE_FONT_POLICY` for
these app launches so the package itself chooses its policy. Set an appropriate
private `XDG_RUNTIME_DIR` for Weston. Each app reaches `Frontend ready`; a
15-second timeout then stops the idle app. Run sequentially because the desktop
has a single-instance lock. X11 and native-Wayland logs preceded the final
addition of the X11-to-Wayland fallback; font policy and those selection branches
were unchanged. The fallback log uses the final repaired package.

`python3 scripts/test-linux-appimage-runtime.py` exercises policy selection,
real Fontconfig filtering, and real SquashFS repair. No tests are skipped in
the recorded Linux run. [linux-host-opener.log](linux-host-opener.log) comes
from a small Linux crate using the exact helper functions from `host_opener.rs`
and `open` 5.3.3. A fixture `xdg-open` records its environment and local target;
no real browser, file manager or network request is launched.

## Software checks

Full backend and UI logs record the complete suites. The later profile and
toast regressions are recorded separately. `tauri-tests.log` covers the full
desktop Rust suite on macOS. Clippy warnings are retained for review. Empty
TypeScript/ESLint output means success; exit codes are recorded in the manifest.
The initial [investigation evidence](../feedback-2026-10-11/README.md) also
contains targeted PDF, TCP, serial, raster and prior-release regression checks.
