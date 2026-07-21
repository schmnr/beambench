use beambench_gcode::{AcknowledgedGcodeDialect, LineProtocolEvent, classify_response};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct ResponseCorpus {
    schema_version: u16,
    dialect: AcknowledgedGcodeDialect,
    cases: Vec<ResponseCase>,
}

#[derive(Debug, Deserialize)]
struct ResponseCase {
    line: String,
    expected: LineProtocolEvent,
}

fn assert_corpus(contents: &str) {
    let corpus: ResponseCorpus = serde_json::from_str(contents).unwrap();
    assert_eq!(corpus.schema_version, 1);
    assert!(!corpus.cases.is_empty());

    for case in corpus.cases {
        assert_eq!(
            classify_response(corpus.dialect, &case.line),
            case.expected,
            "response line: {:?}",
            case.line
        );
    }
}

#[test]
fn marlin_response_corpus_is_stable() {
    assert_corpus(include_str!("fixtures/marlin_responses.json"));
}

#[test]
fn smoothieware_response_corpus_is_stable() {
    assert_corpus(include_str!("fixtures/smoothieware_responses.json"));
}
