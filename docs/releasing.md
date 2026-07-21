# Release process

Beam Bench releases are built from version tags by the platform workflows in
`.github/workflows`. Release assets include signed desktop packages, standalone
CLI archives, checksums, and corresponding source.

## Prepare a release

1. Add the release notes to `CHANGELOG.md`.
2. Set the same version in the workspace `Cargo.toml`, `tauri-app/package.json`,
   and `tauri-app/src-tauri/tauri.conf.json`.
3. Refresh both lockfiles and run the project quality checks.
4. Build the corresponding-source archives with
   `scripts/build-gpl-source-archives`.
5. Commit the exact release source and create a matching `v<version>` tag.

## Build and publish

Run the macOS, Windows, and Linux release workflows for the same tag. Verify the
signatures, checksums, installers, CLI archives, and source archives before
publishing the release or updating the application manifest.

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
