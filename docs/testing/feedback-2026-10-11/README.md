# Feedback investigation evidence October 11 2026

See [the investigation](../../feedback-investigation-2026-10-11.md) for findings and limitations. [validation.json](validation.json) records the checkout, commands, test counts, and external input hashes. The test logs preserve all 16 targeted invocations. Counts include overlapping filtered tests.

## Linux probes

The valid probes used Ubuntu 24.04 amd64, emulated on an ARM64 Mac, with Xvfb. They establish a matching assertion in the shipped renderer and a separate display-initialization failure. They do not reproduce the original JPG interaction or the Fedora desktop environment.

| Evidence | Result |
| --- | --- |
| [bundled-colrv1.log](bundled-colrv1.log) | Released WebKit 2.50.4 crashes with the reported COLRv1 assertion. |
| [bundled-ascii.log](bundled-ascii.log) | Released WebKit 2.50.4 survives the ASCII control. |
| [system-colrv1.log](system-colrv1.log) | Host WebKit 2.52.6 survives the same color-font probe. |
| [no-display.log](no-display.log) | Released desktop panics during GTK initialization. |
| [x-display.log](x-display.log) | Released desktop reaches frontend readiness with Xvfb. |
| [released-gtk-hook.sh](released-gtk-hook.sh) | Unmodified launcher hook from the AppImage, including forced X11. |

ANSI color sequences were removed from the two desktop logs. The AppImage and font binary are external inputs and are not committed here. Their hashes are in the validation record.

To repeat in an isolated Linux environment, place the input files and probe scripts in `/evidence`. Obtain the exact [0.2.26 AppImage](https://github.com/schmnr/beambench/releases/tag/v0.2.26) and the [Noto COLRv1 v2.048 font](https://fuchsia.googlesource.com/third_party/github.com/googlefonts/noto-emoji/+/refs/tags/v2.048/fonts/Noto-COLRv1.ttf?format=TEXT). The latter endpoint returns base64 text; decode it to `Noto-COLRv1.ttf` and verify both hashes before testing. Install PyGObject, the WebKit2 4.1 introspection package, Xvfb, xauth, Fontconfig, and squashfs-tools. The investigation used Ubuntu's WebKit 2.52.6 for the host control; record the version if the package has changed.

Extract the release payload directly when the AppImage runtime cannot run under emulation:

```sh
unsquashfs -offset 944632 -d /evidence/appdir /evidence/Beam.Bench_0.2.26_amd64.AppImage
cp -L /usr/bin/python3 /evidence/appdir/usr/bin/probe-python
```

The interpreter location matters. WebKit must resolve the matching helper executables from the payload, rather than combine a system helper with the released library. Run the released-library probe and ASCII control:

```sh
PYTHONHOME=/usr LD_LIBRARY_PATH=/evidence/appdir/usr/lib WEBKIT_DISABLE_DMABUF_RENDERER=1 WEBKIT_DISABLE_COMPOSITING_MODE=1 timeout 18 xvfb-run -a /evidence/appdir/usr/bin/probe-python /evidence/font-probe.py
PYTHONHOME=/usr LD_LIBRARY_PATH=/evidence/appdir/usr/lib WEBKIT_DISABLE_DMABUF_RENDERER=1 WEBKIT_DISABLE_COMPOSITING_MODE=1 timeout 18 xvfb-run -a /evidence/appdir/usr/bin/probe-python /evidence/font-probe-plain.py
```

The original [font probe](font-probe.py) loads four emoji from the external COLRv1 font. The [ASCII control](font-probe-plain.py) uses plain text and an absent font URL. Both stop after ten seconds if the web process survives. The process-termination message determines the crash result; the probe itself can exit normally after logging a renderer crash.

Run the same font script with the system interpreter and libraries for the host control:

```sh
WEBKIT_DISABLE_DMABUF_RENDERER=1 WEBKIT_DISABLE_COMPOSITING_MODE=1 timeout 18 xvfb-run -a /usr/bin/python3 /evidence/font-probe.py
```

The renderer overrides allow the Xvfb environment to run without accelerated rendering. None of these commands disables the WebKit sandbox. An earlier invalid probe setup mixed system helper executables with released libraries and crashed on plain text; those logs are excluded.
