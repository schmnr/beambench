# Beam Bench patches to lopdf 0.44.0

Source: crates.io `lopdf` 0.44.0, MIT license retained in `LICENSE`.

- `Reader::read_unencrypted` rejects encryption and limits xref object counts
  before loading objects. It avoids the metadata API's early ObjStm expansion
  and the encrypted loader's bypass of the object filter.
- Object-stream parsing stops each object at its next declared offset and rejects
  unordered/overlapping offsets. Nested strings cannot amplify decoded bytes
  into repeated copies of the rest of the stream.
- PNG predictor row allocations respect the caller's decompression limit and
  use checked arithmetic, including when decoding cross-reference streams.

- A strict content decoder bounds operation allocation before parsing the whole
  stream and rejects malformed trailing content instead of returning partial artwork.

Beam Bench defers object-stream expansion through the existing filter API and
charges aggregate decoded bytes in its importer. The upstream general loader
keeps its existing encryption behavior; Beam Bench calls the restricted method.
Regression coverage is in `beambench-core::import_pdf` and `tests/pdf_artwork.rs`.
