use lopdf::{Document, ObjectStream, Stream, xref::XrefEntry};
use std::collections::{BTreeMap, BTreeSet};

const OBJECT_LIMIT: usize = 100_000;

pub(super) fn expand(doc: &mut Document, remaining: &mut usize) -> Result<(), String> {
    if doc.reference_table.entries.len() > OBJECT_LIMIT {
        return Err("PDF exceeds the 100,000 object limit".into());
    }
    let mut containers: BTreeMap<u32, Vec<(u32, u32)>> = BTreeMap::new();
    for (&id, entry) in &doc.reference_table.entries {
        if let XrefEntry::Compressed { container, index } = entry {
            containers
                .entry(*container)
                .or_default()
                .push((id, *index as u32));
        }
    }
    for (container, references) in containers {
        let source = doc
            .get_object((container, 0))
            .and_then(lopdf::Object::as_stream)
            .map_err(|e| format!("Cannot resolve PDF object stream: {e}"))?;
        if !source.dict.has_type(b"BeamBenchDeferredObjects") {
            return Err("PDF compressed object references an invalid container".into());
        }
        let bytes =
            super::decode_artwork_stream(source, (*remaining).min(super::PDF_EAGER_STREAM_LIMIT))
                .map_err(|e| format!("PDF object stream expansion limit exceeded: {e}"))?;
        *remaining = remaining
            .checked_sub(bytes.len())
            .ok_or("PDF expanded content limit exceeded")?;
        let first = source
            .dict
            .get(b"First")
            .and_then(lopdf::Object::as_i64)
            .ok()
            .and_then(|n| usize::try_from(n).ok())
            .ok_or("Invalid PDF object stream offset")?;
        let count = source
            .dict
            .get(b"N")
            .and_then(lopdf::Object::as_i64)
            .ok()
            .and_then(|n| usize::try_from(n).ok())
            .filter(|n| *n <= OBJECT_LIMIT)
            .ok_or("Invalid PDF object stream count")?;
        let header: Vec<usize> = std::str::from_utf8(
            bytes
                .get(..first)
                .ok_or("Invalid PDF object stream header")?,
        )
        .map_err(|e| e.to_string())?
        .split_whitespace()
        .map(str::parse)
        .collect::<Result<_, _>>()
        .map_err(|_| "Invalid PDF object stream index")?;
        if header.len() != count * 2
            || header.chunks_exact(2).any(|pair| {
                pair[0] > u32::MAX as usize || pair[1] >= bytes.len().saturating_sub(first)
            })
        {
            return Err("Invalid PDF object stream index".into());
        }
        let indices: Vec<_> = header
            .chunks_exact(2)
            .map(|pair| (pair[0] as u32, pair[1]))
            .collect();
        if indices.windows(2).any(|pair| pair[0].1 >= pair[1].1)
            || indices
                .iter()
                .map(|pair| pair.0)
                .collect::<BTreeSet<_>>()
                .len()
                != count
        {
            return Err(
                "PDF object stream contains duplicate or unordered offsets/identifiers".into(),
            );
        }
        for (id, index) in references {
            let index = index as usize;
            let &(indexed_id, offset) = indices
                .get(index)
                .ok_or("PDF compressed-object index does not match its cross-reference")?;
            if indexed_id != id {
                return Err(
                    "PDF compressed-object index does not match its cross-reference".into(),
                );
            }
            let end = indices
                .get(index + 1)
                .map_or(bytes.len(), |pair| first + pair.1);
            // The library parses each object through the end of the stream.
            // Isolate each declared slice so overlapping/nested string starts
            // cannot multiply a 1 MiB stream into gigabytes of parsed objects.
            let header = format!("{id} 0 ");
            let mut content = header.as_bytes().to_vec();
            content.extend_from_slice(&bytes[first + offset..end]);
            let mut stream = Stream::new(
                lopdf::dictionary! {"N"=>1,"First"=>header.len() as i64},
                content,
            );
            let mut parsed = ObjectStream::new_with_limit(
                &mut stream,
                Some(super::PDF_EAGER_STREAM_LIMIT + header.len()),
            )
            .map_err(|e| format!("Invalid PDF object stream: {e}"))?
            .objects;
            let object = parsed
                .remove(&(id, 0))
                .ok_or("Malformed PDF compressed object")?;
            doc.objects.insert((id, 0), object);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{Object, dictionary};

    fn insert(doc: &mut Document, container: u32, id: u32, length: usize) {
        let header = format!("{id} 0 ");
        let bytes = format!("{header}({})", "a".repeat(length)).into_bytes();
        let mut stream = Stream::new(
            dictionary! {"Type"=>"BeamBenchDeferredObjects", "N"=>1, "First"=>header.len() as i64},
            bytes,
        );
        stream.compress().unwrap();
        doc.objects.insert((container, 0), Object::Stream(stream));
        doc.reference_table.entries.insert(
            id,
            XrefEntry::Compressed {
                container,
                index: 0,
            },
        );
    }

    #[test]
    fn aggregate_object_expansion_is_charged_before_parsing() {
        let mut doc = Document::new();
        insert(&mut doc, 1, 10, 512 * 1024);
        insert(&mut doc, 2, 11, 512 * 1024);
        let mut remaining = 768 * 1024;
        assert!(
            expand(&mut doc, &mut remaining)
                .unwrap_err()
                .contains("limit")
        );
        assert!(doc.objects.contains_key(&(10, 0)));
        assert!(!doc.objects.contains_key(&(11, 0)));
    }

    #[test]
    fn object_stream_index_must_match_authoritative_xref() {
        let mut doc = Document::new();
        insert(&mut doc, 1, 10, 16);
        doc.reference_table.entries.insert(
            10,
            XrefEntry::Compressed {
                container: 1,
                index: 1,
            },
        );
        let mut remaining = super::super::PDF_CONTENT_LIMIT;
        assert!(
            expand(&mut doc, &mut remaining)
                .unwrap_err()
                .contains("cross-reference")
        );
    }
    #[test]
    fn overlapping_nested_string_offsets_cannot_amplify_parsed_memory() {
        let mut doc = Document::new();
        let header = "10 0 11 1 ";
        let bytes = format!("{header}(({}))", "a".repeat(500_000)).into_bytes();
        doc.objects.insert((1,0), Stream::new(lopdf::dictionary! {"Type"=>"BeamBenchDeferredObjects","N"=>2,"First"=>header.len() as i64},bytes).into());
        doc.reference_table.entries.insert(
            10,
            XrefEntry::Compressed {
                container: 1,
                index: 0,
            },
        );
        doc.reference_table.entries.insert(
            11,
            XrefEntry::Compressed {
                container: 1,
                index: 1,
            },
        );
        let mut remaining = super::super::PDF_CONTENT_LIMIT;
        assert!(
            expand(&mut doc, &mut remaining)
                .unwrap_err()
                .contains("Malformed")
        );
    }
}
