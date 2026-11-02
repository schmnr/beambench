# SVG text encoding, feedback 82

Report: [r-8321f6eecfbd480dbefa7d04d61ce0ae](https://github.com/schmnr/beambench-feedback/issues/82).
The November 2, 2026 report uses Beam Bench 0.2.38 on Windows with the pt-BR
locale. Import fails with `provided data has not an UTF-8 encoding`. No source
SVG or project is attached, so the original file's encoding and validity cannot
be established from the report.

## Reproduction and change

On the release source, valid UTF-16 and declared Windows-1252/ISO-8859-1 SVGs
reproduce the exact parser error. The shared core importer passes raw bytes to
usvg, which accepts only UTF-8. The editable-text pass also expects UTF-8.

Decode XML before either pass. Recognize UTF-8/UTF-16 byte order marks, UTF-16
XML declaration byte patterns, and supported ASCII-compatible encoding labels
in the leading XML declaration. Keep UTF-8 as the default. Decompress SVGZ
before decoding so compressed artwork follows the same import path.

Use strict decoding. Unknown encodings and damaged input return an error with
a UTF-8 re-save instruction. Do not guess the Windows system code page or
replace invalid characters, since that could silently change imported text.
The encoding_rs and quick-xml versions were already in Cargo.lock and in the
third-party license inventory; this change adds direct core dependencies.

## Validation

- `cargo test -p beambench-core -p beambench-service --locked` passed, including
  990 core unit tests, 907 service unit tests and eight service integration
  tests. One existing service test requires a native desktop power manager and
  remains ignored. Core integration tests and doc tests also passed.
- Targeted Clippy and the workspace formatting check passed. Existing Clippy
  warnings outside this change remain.
- Before the fix, the UTF-16 and declared legacy-encoding regressions fail with
  the report's exact error. Compressed UTF-8 also loses editable text because
  the original pre-parser sees compressed bytes.
- Core regressions compare accented editable text, vector paths, physical
  bounds, transforms and color grouping with the UTF-8 result. Cases include
  both UTF-16 byte orders with/without BOM, UTF-8 BOM, both XML quote styles,
  declaration whitespace, Windows-1252 euro signs, and SVGZ.
- A Windows-1252 regression uses bytes that are also valid UTF-8 to verify that
  the explicit declaration wins over a successful UTF-8 decode.
- Invalid UTF-8, undeclared legacy text, unknown encodings, truncated multibyte
  sequences and invalid/truncated UTF-16 fail without changing the project.
- Service regressions exercise file-path and base64 imports, physical sizing,
  undo/redo, and whole-batch rollback with unchanged layers and plan cache.

The original file and a native Windows desktop import remain untested. This
fix addresses the reproduced encoding limitation; it does not establish that
the reporter's unavailable SVG is valid or rule out a mislabeled binary file.
No release or reporter correspondence is part of this change.
