use std::collections::BTreeSet;
use std::time::Duration;

use beambench_common::{
    ConsoleDirection, GrblFamilyDialect, GrblFamilyIdentityStatus, MachineRunState, SessionState,
};
use beambench_grbl::{
    FluidNcSerialAdapter, GrblFamilyIdentityProbeConfig, GrblFamilyIdentityProbeOutcome,
    GrblFamilySerialAdapter, GrblResponse, GrblSession,
};
use beambench_serial::MockSerialTransport;
use serde::Deserialize;

const CORPUS_JSON: &str = include_str!("fixtures/fluidnc_serial_v1.json");

#[derive(Debug, Deserialize)]
struct FluidNcCorpus {
    schema_version: u16,
    description: String,
    provenance: Vec<FixtureProvenance>,
    startup_lines: Vec<String>,
    controller_info_lines: Vec<String>,
    settings_lines: Vec<String>,
    status_cases: Vec<StatusCase>,
    alarm_line: String,
    recovery_status_line: String,
}

#[derive(Debug, Deserialize)]
struct FixtureProvenance {
    kind: String,
    repository: String,
    commit: String,
    url: String,
    note: String,
}

#[derive(Debug, Deserialize)]
struct StatusCase {
    id: String,
    line: String,
    expected_state: String,
    expected_machine_position: [f64; 3],
    expected_work_position: [f64; 3],
}

fn load_corpus() -> FluidNcCorpus {
    serde_json::from_str(CORPUS_JSON).expect("FluidNC serial corpus must be valid JSON")
}

fn expected_run_state(value: &str) -> MachineRunState {
    match value {
        "idle" => MachineRunState::Idle,
        "run" => MachineRunState::Run,
        "hold" => MachineRunState::Hold,
        "alarm" => MachineRunState::Alarm,
        other => panic!("unknown fixture state {other:?}"),
    }
}

fn assert_position(actual: [f64; 3], expected: [f64; 3], case_id: &str) {
    assert_eq!(actual, expected, "fixture {case_id}");
}

#[test]
fn fixture_is_versioned_unique_and_pinned() {
    let corpus = load_corpus();
    assert_eq!(corpus.schema_version, 1);
    assert!(!corpus.description.trim().is_empty());
    assert!(corpus.provenance.len() >= 3);

    for source in &corpus.provenance {
        assert!(!source.kind.trim().is_empty());
        assert_eq!(source.repository, "bdring/FluidNC");
        assert_eq!(source.commit.len(), 40);
        assert!(source.commit.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert!(source.url.contains(&format!("/blob/{}/", source.commit)));
        assert!(!source.note.trim().is_empty());
    }

    let mut ids = BTreeSet::new();
    for case in &corpus.status_cases {
        assert!(ids.insert(&case.id), "duplicate status fixture {}", case.id);
    }
}

#[test]
fn exact_source_derived_probe_activates_fluidnc_adapter() {
    let corpus = load_corpus();
    let mut transport = MockSerialTransport::new("fixture-fluidnc");
    for line in &corpus.startup_lines {
        transport.enqueue_response(line);
    }
    let handle = transport.handle();
    let mut session = GrblSession::new(Box::new(transport));

    session.connect().unwrap();
    session.poll().unwrap();
    assert_eq!(session.session_state(), SessionState::Validating);
    assert_eq!(
        session.grbl_family_identity().status,
        GrblFamilyIdentityStatus::Provisional
    );

    for line in &corpus.controller_info_lines {
        handle.enqueue_response(line);
    }
    let probe = session
        .probe_grbl_family_identity(GrblFamilyIdentityProbeConfig {
            command_timeout: Duration::from_millis(10),
            poll_interval: Duration::ZERO,
        })
        .unwrap();

    assert_eq!(probe.identity.dialect, GrblFamilyDialect::FluidNc);
    assert_eq!(probe.identity.status, GrblFamilyIdentityStatus::Identified);
    assert_eq!(
        probe.controller_info,
        GrblFamilyIdentityProbeOutcome::Succeeded
    );
    assert_eq!(
        probe.extended_controller_info,
        GrblFamilyIdentityProbeOutcome::NotNeeded
    );
    FluidNcSerialAdapter::new().validate_probe(&probe).unwrap();
    assert_eq!(probe.identity.firmware_identity.as_deref(), Some("FluidNC"));
    assert_eq!(
        probe.identity.firmware_version.as_deref(),
        Some("4.0.3 (main-94e8adb) (esp32-wifi)")
    );
}

#[test]
fn lifecycle_replay_covers_settings_status_fault_and_reconnect() {
    let corpus = load_corpus();
    let mut transport = MockSerialTransport::new("fixture-fluidnc");
    for line in &corpus.startup_lines {
        transport.enqueue_response(line);
    }
    let handle = transport.handle();
    let mut session = GrblSession::new(Box::new(transport));

    session.connect().unwrap();
    session.poll().unwrap();
    for line in &corpus.controller_info_lines {
        handle.enqueue_response(line);
    }
    let probe = session
        .probe_grbl_family_identity(GrblFamilyIdentityProbeConfig {
            command_timeout: Duration::from_millis(10),
            poll_interval: Duration::ZERO,
        })
        .unwrap();
    FluidNcSerialAdapter::new().validate_probe(&probe).unwrap();
    session.mark_ready().unwrap();

    for line in &corpus.settings_lines {
        handle.enqueue_response(line);
    }
    let settings_responses = session.poll().unwrap();
    assert!(settings_responses.contains(&GrblResponse::Setting(376, 3.0)));
    assert!(settings_responses.iter().any(
        |response| matches!(response, GrblResponse::Unknown(line) if line == "$Report/Status=3")
    ));
    assert_eq!(session.settings().get(0), Some(10.0));
    assert_eq!(session.settings().get(32), Some(1.0));
    assert_eq!(session.settings().get(376), Some(3.0));

    for case in &corpus.status_cases {
        if case.expected_state == "run" && session.session_state() == SessionState::Ready {
            session.start_running().unwrap();
        } else if case.expected_state == "hold" && session.session_state() == SessionState::Running
        {
            session.pause().unwrap();
        }
        handle.enqueue_response(&case.line);
        let responses = session.poll().unwrap();
        let status = responses
            .iter()
            .find_map(|response| match response {
                GrblResponse::Status(status) => Some(status),
                _ => None,
            })
            .unwrap_or_else(|| panic!("fixture {} did not parse as status", case.id));
        assert_eq!(
            status.run_state,
            expected_run_state(&case.expected_state),
            "fixture {}",
            case.id
        );
        assert_position(
            [
                status.machine_position.x,
                status.machine_position.y,
                status.machine_position.z,
            ],
            case.expected_machine_position,
            &case.id,
        );
        assert_position(
            [
                status.work_position.x,
                status.work_position.y,
                status.work_position.z,
            ],
            case.expected_work_position,
            &case.id,
        );
    }
    assert_eq!(session.session_state(), SessionState::Alarm);

    handle.enqueue_response(&corpus.alarm_line);
    let alarm_responses = session.poll().unwrap();
    assert!(alarm_responses.contains(&GrblResponse::Alarm(1)));
    assert_eq!(session.session_state(), SessionState::Alarm);

    handle.enqueue_response(&corpus.recovery_status_line);
    session.poll().unwrap();
    assert_eq!(session.session_state(), SessionState::Ready);

    session.send_command("G90").unwrap();
    assert!(
        session
            .get_console_log(20)
            .iter()
            .any(|entry| { entry.direction == ConsoleDirection::Sent && entry.content == "G90" })
    );

    session.disconnect().unwrap();
    assert_eq!(session.session_state(), SessionState::Disconnected);
    assert_eq!(
        session.grbl_family_identity().status,
        GrblFamilyIdentityStatus::Unknown
    );

    for line in &corpus.startup_lines {
        handle.enqueue_response(line);
    }
    session.connect().unwrap();
    session.poll().unwrap();
    assert_eq!(session.session_state(), SessionState::Validating);
    assert_eq!(
        session.grbl_family_identity().dialect,
        GrblFamilyDialect::FluidNc
    );
    assert_eq!(
        session.grbl_family_identity().status,
        GrblFamilyIdentityStatus::Provisional
    );
}
