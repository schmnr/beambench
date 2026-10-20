# Linux AppImage relaunch

Tauri 2.11.5 restarts an AppImage by spawning its external `APPIMAGE` path with
the running process's environment. The new AppImage gets a new mount, while
inherited library, plugin and font paths can still refer to the previous mount.

The GTK runtime hook removes path components owned by the previous Beam Bench
mount before linuxdeploy adds the new mount's paths. New launches export their
mount identity for the next restart. Older launches are identified by Beam
Bench's bundled font marker, or the legacy GStreamer path when custom fonts
were selected. Other host paths, including similarly named directories, remain
intact. An explicitly empty plugin override remains empty when no stale bundle
component was removed.

The bundled font file is rebound only when Beam Bench selected it. Custom font
configuration stays unchanged. The executable wrapper still restores the
incoming GStreamer system paths after linuxdeploy runs.

## Regression coverage

Run the platform-independent policy cases with:

```sh
python3 scripts/test-linux-appimage-runtime.py
```

On Linux, install SquashFS tools, Fontconfig and GStreamer base/good plugins,
then exercise the complete packaged launcher chain:

```sh
BEAMBENCH_TEST_APPIMAGE=/absolute/path/to/Beam.Bench.AppImage \
  python3 scripts/test-linux-appimage-runtime.py
```

The packaged probe checks real `appsink`, `v4l2src` and `videoconvert` loading,
three synthetic frames, and font discovery. It repeats with inherited 0.2.30,
0.2.31, custom-font and mount-marker environments whose old mount does not
exist. Unit cases also preserve explicit empty/custom plugin settings, spaces,
empty path components, renamed AppImages and unrelated directories with the
same name prefix.
These restart cases fail against the published 0.2.31 runtime policy.

The test replaces only the extracted desktop binary, leaving the AppRun shell,
ELF launcher and executable wrapper intact. It does not modify the supplied
AppImage, install an update, open the updater UI, or capture a physical camera.
When releasing, repeat it against the final signed artifact and record native
Linux updater/UI or physical-device checks separately. Existing unsaved-project
and busy-machine update guards remain in place.
