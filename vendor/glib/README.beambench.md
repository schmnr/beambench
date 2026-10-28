# Beam Bench GLib security backport

The library code matches crates.io `glib` 0.18.5 except for the two-line
upstream fix to `VariantStrIter::impl_get` in `src/variant_iter.rs`.

- Advisory: [RUSTSEC-2024-0429 / GHSA-wrw7-89jp-8q8g](https://rustsec.org/advisories/RUSTSEC-2024-0429.html).
- Upstream fix: [gtk-rs-core commit 05dff0ee696f9bcd8617cd48c4b812d046d440cb](https://github.com/gtk-rs/gtk-rs-core/commit/05dff0ee696f9bcd8617cd48c4b812d046d440cb).
- Change: declare the output pointer mutable and pass `&mut p` to
  `g_variant_get_child`. Passing an immutable reference to a variadic C output
  argument violates Rust's aliasing rules and can leave a null pointer under
  optimization.

Tauri's GTK3 bindings require the 0.18 API. Replacing it with GLib 0.20 is not a
compatible package upgrade. Keep the upstream version and license intact rather
than claiming this is a 0.20 release. `Cargo.toml` patches the crates.io package
to this source on every platform where it is used.

Run the safe-API iterator regression with optimization enabled:

```sh
cargo run --locked --release --manifest-path scripts/glib-variant-probe/Cargo.toml
```

It covers string iteration, `nth`, `nth_back`, reverse and mixed traversal,
`count`, UTF-8 strings and `last`. Remove this backport when the GTK/Tauri dependency stack can use an
upstream release containing the fix. The original MIT license is in `LICENSE`.
