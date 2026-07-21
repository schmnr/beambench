use beambench_gcode::classify_marlin_response;
use beambench_marlin::{
    MarlinIdentity, MarlinIdentityDetector, MarlinIdentityProbeOutcome, MarlinIdentityProbeSequence,
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct IdentityCorpus {
    schema_version: u16,
    cases: Vec<IdentityCase>,
}

#[derive(Debug, Deserialize)]
struct IdentityCase {
    name: String,
    lines: Vec<String>,
    expected: MarlinIdentity,
}

#[test]
fn marlin_identity_transcripts_are_order_independent_and_fail_closed() {
    let corpus: IdentityCorpus =
        serde_json::from_str(include_str!("fixtures/identity_transcripts.json")).unwrap();
    assert_eq!(corpus.schema_version, 2);

    for case in corpus.cases {
        let mut detector = MarlinIdentityDetector::default();
        let mut probe = MarlinIdentityProbeSequence::default();
        assert_eq!(probe.begin(), Some("M115"), "case: {}", case.name);

        for line in &case.lines {
            detector.observe_line(line);
            probe.observe(&classify_marlin_response(line));
        }

        let identity = detector.identity();
        assert_eq!(identity, case.expected, "case: {}", case.name);
        let result = probe.finish(identity).expect("transcript must end in ok");
        assert_eq!(
            result.outcome,
            MarlinIdentityProbeOutcome::Succeeded,
            "case: {}",
            case.name
        );
    }
}
