# Release process

Beam Bench releases are built from version tags by the platform workflows in
`.github/workflows`. Release assets include signed desktop packages, standalone
CLI archives, checksums, and corresponding source.

## Prepare a release

1. Add the release notes to `CHANGELOG.md`.
2. Set the same version in the workspace `Cargo.toml`, `tauri-app/package.json`,
   and `tauri-app/src-tauri/tauri.conf.json`.
3. Refresh both lockfiles and run the project quality checks.
4. Copy `docs/testing/release-smoke-record.md`, complete the high-risk smoke
   record, and commit it with the release source. Record unavailable hardware or
   platforms as `Not run`; do not silently treat them as passed.
5. Build the corresponding-source archives with
   `scripts/build-gpl-source-archives`.
6. Commit the exact release source and create a matching `v<version>` tag.

## Build and publish

Run the macOS, Windows, and Linux release workflows for the same tag. Verify the
signatures, checksums, installers, CLI archives, and source archives before
publishing the release or updating the application manifest.

For package validation, use a prerelease version and matching tag. The Linux and
Windows workflows save signed workflow artifacts without attaching them to a
GitHub Release by default. Enable `attach_to_release` only for an authorized
release upload. The local macOS script stages artifacts without uploading them.

Each platform workflow runs the critical planner, machine-service, and frontend
tests against the checked-out tag before packaging. A source-test failure is a
release failure even if an installer can still be built locally.

The macOS workflow retries the Tauri packaging command up to three times. This
covers temporary Apple timestamp or notarization service failures that can
occur after compilation has completed. A run that still fails after all three
attempts must be inspected rather than published.

The Windows workflow treats the signed NSIS setup executable as the required
desktop and updater artifact. The MSI is optional because it is not used by the
website or application updater. Enable `include_msi` only when an MSI is needed.
An MSI failure does not discard a valid signed NSIS build, and the workflow
uploads the generated WiX inputs as a diagnostic artifact when MSI packaging
fails.

The Linux workflow repairs the generated AppImage before signing it. The repair
removes host-level systemd, Wayland, GLib, and GStreamer libraries that linuxdeploy can
copy from the build runner even though the host graphics and WebKit stack must
provide them. `scripts/harden-linux-appimage --check <artifact>` is the
deterministic release audit and must pass on the final signed binary.
The generated ELF launcher overrides GStreamer's system plugin paths. A
desktop executable wrapper restores the user's original values after that
launcher runs, allowing GStreamer's normal host discovery when unset. Both
wrappers are required; changing only the GTK hook is too early.

The Linux workflow also runs the final AppImage's complete launcher chain
against installed host `appsink`, `v4l2src`, and `videoconvert` plugins and a
three-frame synthetic capture pipeline. To repeat this check on Linux with
SquashFS tools and GStreamer base/good plugins installed:

```sh
BEAMBENCH_TEST_APPIMAGE=/absolute/path/to/Beam.Bench.AppImage \
  python3 scripts/test-linux-appimage-runtime.py
```

This test substitutes a media probe for the extracted desktop binary while
retaining the packaged launchers. It does not validate a physical camera or
change the signed AppImage.

The release also launches the unmodified signed AppImage in disposable Ubuntu
22.04, 24.04 and 26.04 containers. Each host must reach the actual desktop's
`Frontend ready` callback and remain alive. Both X11 and mixed Plasma/Wayland
environment variables are exercised with Xvfb. This is software-rendered X11
coverage, not a native KWin/Wayland or physical hardware test. Docker's nested
WebKit sandbox limitation is handled only inside the isolated test containers.
The package is mounted read-only, with a fresh home, an unprivileged user and
network access disabled. Linux artifacts cannot be attached to the release until
these checks pass. Evidence is uploaded as a workflow artifact.

```sh
python3 scripts/test-linux-appimage-desktop.py /absolute/path/to/Beam.Bench.AppImage \
  --output linux-desktop-smoke.json
```

Keep the oldest-supported Ubuntu 22.04 build runner even when adding newer test
hosts. Read [Linux packaging incidents](testing/linux-packaging-incidents.md)
before changing packaging. In particular, a substituted media probe and a clean
library audit missed the 0.2.31 startup regression. All three checks are required.

The same test covers automatic relaunch environments from older AppImages.
It checks camera capture and font discovery after the previous mount is gone,
including the GStreamer overrides exported before 0.2.31. Keep this transition
coverage when changing either launcher. See
[Linux AppImage relaunch](testing/linux-appimage-relaunch.md) for the scope and
remaining native updater checks.

Dispatch the platform workflows one at a time. They share a same-tag concurrency
group, and GitHub retains at most one pending run in that group.

Upload immutable versioned assets before replacing the stable update manifest.
Validate the finished manifest with:

```sh
cd tauri-app
npm run update:validate-manifest -- latest.json --check-urls
```

If a release must be corrected after publication, publish a higher-version
hotfix; the updater only moves forward.
