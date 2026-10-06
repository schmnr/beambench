# Beam Bench patches to pdf417 0.2.1

Source: crates.io `pdf417` 0.2.1. MIT license retained in `LICENSE`.
Upstream archive SHA256: `0856ab2592a12298accdee63d4a6149085f4da15dcad92e421ccf3b56ebfe7c3`.

- Remove the nightly `const_mut_refs` feature gate, which is stable in the
  supported Rust toolchain.
- Suppress this vendored crate's pre-existing Clippy warnings with a crate-level
  `allow(clippy::all)`.
- Format Rust source and examples. No encoder or table-value changes.

Verified against the upstream archive on 2026-10-06. The crate is included by
path in the workspace; project barcode tests exercise the encoder. CI retains
its existing exclusion of upstream example/test issues.
