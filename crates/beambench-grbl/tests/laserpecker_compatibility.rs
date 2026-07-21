use std::collections::BTreeSet;

use beambench_common::{ConsoleDirection, MachineRunState, SessionState};
use beambench_grbl::{GrblResponse, GrblSession, parse_response};
use beambench_serial::MockSerialTransport;
use serde::Deserialize;

const CORPUS_JSON: &str = include_str!("fixtures/laserpecker_grbl_v1.json");

#[derive(Debug, Deserialize)]
struct LaserPeckerCorpus {
    schema_version: u16,
    description: String,
    sources: Vec<LaserPeckerProfile>,
    idle_status: String,
    job_body: Vec<String>,
    acknowledgement: String,
}

#[derive(Debug, Deserialize)]
struct LaserPeckerProfile {
    model: String,
    url: String,
    transport: String,
    baud_rate: u32,
    s_value_max: u32,
    width_mm: f64,
    height_mm: f64,
    job_header: Vec<String>,
}

fn corpus() -> LaserPeckerCorpus {
    serde_json::from_str(CORPUS_JSON).expect("LaserPecker corpus must be valid JSON")
}

#[test]
fn official_profile_facts_are_versioned_and_cover_the_documented_models() {
    let corpus = corpus();
    assert_eq!(corpus.schema_version, 1);
    assert!(corpus.description.contains("no hardware-validation claim"));

    let models = corpus
        .sources
        .iter()
        .map(|source| source.model.as_str())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        models,
        BTreeSet::from(["LP2 Plus", "LP4", "LP5", "LX1", "LX1 Max", "LX2"])
    );

    for source in &corpus.sources {
        assert!(source.url.starts_with("https://support.laserpecker.net/"));
        assert!(source.width_mm > 0.0 && source.height_mm > 0.0);
        assert!(matches!(source.s_value_max, 255 | 1000));
        if source.model == "LX2" {
            assert_eq!(source.transport, "tcp:8888");
            assert_eq!(source.job_header, ["START_PRINT"]);
        } else {
            assert_eq!(source.transport, "serial");
            assert_eq!(source.baud_rate, 460_800);
        }
    }
}

#[test]
fn status_and_job_acknowledgement_replay_use_the_shared_grbl_session() {
    let corpus = corpus();
    let mut transport = MockSerialTransport::new("laserpecker-fixture");
    transport.enqueue_response(&corpus.idle_status);
    let handle = transport.handle();
    let mut session = GrblSession::new(Box::new(transport));

    session.connect().unwrap();
    let previous_status_count = session.status_report_count();
    session.poll().unwrap();
    session
        .begin_validation_from_fresh_status(previous_status_count)
        .unwrap();
    session.mark_ready().unwrap();

    assert_eq!(session.session_state(), SessionState::Ready);
    assert_eq!(session.last_status().run_state, MachineRunState::Idle);
    assert_eq!(session.last_status().spindle_speed, 0.0);

    let lx2 = corpus
        .sources
        .iter()
        .find(|source| source.model == "LX2")
        .unwrap();
    for line in lx2.job_header.iter().chain(corpus.job_body.iter()) {
        session.send_command(line).unwrap();
        handle.enqueue_response(&corpus.acknowledgement);
    }
    let responses = session.poll().unwrap();
    assert_eq!(
        responses
            .iter()
            .filter(|response| **response == GrblResponse::Ok)
            .count(),
        lx2.job_header.len() + corpus.job_body.len()
    );
    assert_eq!(parse_response(&corpus.acknowledgement), GrblResponse::Ok);
    assert!(session.get_console_log(30).iter().any(|entry| {
        entry.direction == ConsoleDirection::Sent && entry.content == "START_PRINT"
    }));
}
