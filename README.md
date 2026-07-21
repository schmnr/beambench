# Beam Bench

Beam Bench is desktop software for preparing and running laser-cutting and
engraving jobs. The application includes a Tauri/React interface and a Rust
workspace containing its import, geometry, planning, preview, and controller
support.

## License

Beam Bench is free software: you can redistribute it and/or modify it under
the terms of the GNU General Public License as published by the Free Software
Foundation, either version 3 of the License, or (at your option) any later
version. See [LICENSE](LICENSE) for the complete terms.

The bitmap tracing implementation contains a Rust port and modification of
Potrace 1.16, Copyright (C) 2001-2019 Peter Selinger. Those files retain
their GPL-2.0-or-later notices. See
[the Potrace notice](crates/beambench-core/src/potrace/README.md) and
[third-party notices](THIRD_PARTY_NOTICES.md).

The GPL permits commercial use, modification, and redistribution, provided
that distributed derivative versions remain under the GPL and include their
corresponding source. The software license does not grant permission to present
a fork as an official Beam Bench product or to reuse Beam Bench branding for a
modified distribution. See [TRADEMARKS.md](TRADEMARKS.md).

## Building from source

The complete source tree includes the scripts and interface files used to
build Beam Bench.

Build and test the Rust workspace:

```sh
cargo build --workspace
cargo test --workspace
```

Build the desktop application after installing the platform prerequisites for
Tauri 2 and Node.js:

```sh
cd tauri-app
npm ci
npm run tauri build
```

Release and packaging workflows are stored in `.github/workflows`, with
supporting scripts under `scripts` and `tauri-app/scripts`.

See [CONTRIBUTING.md](CONTRIBUTING.md) for development checks and the
safety-sensitive behavior expected of machine-control changes. Maintainers can
find the publication checklist in [docs/releasing.md](docs/releasing.md).

## Source for released versions

Version tags and corresponding source archives are published with each binary
release at <https://github.com/schmnr/beambench/releases>. The source archive
for a release is the preferred form for modifying that release.
