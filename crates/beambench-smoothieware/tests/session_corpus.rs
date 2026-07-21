use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use beambench_gcode::AckFlowConfig;
use beambench_serial::{SerialError, SerialTransport};
use beambench_smoothieware::{
    SMOOTHIEWARE_CANCEL_COMMAND, SMOOTHIEWARE_LASER_ENABLE_QUERY, SMOOTHIEWARE_MAXIMUM_S_QUERY,
    SMOOTHIEWARE_PROPORTIONAL_POWER_QUERY, SmoothiewareJobOutcome, SmoothiewareSerialSession,
    SmoothiewareSerialSessionConfig, SmoothiewareSessionState,
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Corpus {
    schema_version: u16,
    provenance: Vec<String>,
    identity_lines: Vec<String>,
    configuration: Configuration,
    cases: Vec<Case>,
}

#[derive(Debug, Deserialize)]
struct Configuration {
    laser_module_enable: String,
    laser_module_maximum_s_value: String,
    laser_module_proportional_power: String,
}

#[derive(Debug, Deserialize)]
struct Case {
    name: String,
    auto_acknowledge: bool,
    commands: Vec<String>,
    cancel_after_first_tick: bool,
    expected_state: SmoothiewareSessionState,
    expected_outcome: SmoothiewareJobOutcome,
    expected_sent_lines: Vec<String>,
}

#[derive(Debug, Default)]
struct ScriptState {
    rx: VecDeque<String>,
    tx: Vec<String>,
}

struct CorpusTransport {
    open: bool,
    state: Arc<Mutex<ScriptState>>,
    identity_lines: Vec<String>,
    configuration: Configuration,
    auto_acknowledge: bool,
}

impl SerialTransport for CorpusTransport {
    fn open(&mut self) -> Result<(), SerialError> {
        self.open = true;
        Ok(())
    }

    fn close(&mut self) -> Result<(), SerialError> {
        self.open = false;
        Ok(())
    }

    fn is_open(&self) -> bool {
        self.open
    }

    fn write_bytes(&mut self, data: &[u8]) -> Result<usize, SerialError> {
        if !self.open {
            return Err(SerialError::NotOpen);
        }
        Ok(data.len())
    }

    fn write_line(&mut self, line: &str) -> Result<(), SerialError> {
        if !self.open {
            return Err(SerialError::NotOpen);
        }
        let mut state = self.state.lock().unwrap();
        state.tx.push(line.to_string());
        match line {
            "M115" => state.rx.extend(self.identity_lines.iter().cloned()),
            SMOOTHIEWARE_LASER_ENABLE_QUERY => state.rx.extend([
                self.configuration.laser_module_enable.clone(),
                "ok".to_string(),
            ]),
            SMOOTHIEWARE_MAXIMUM_S_QUERY => state.rx.extend([
                self.configuration.laser_module_maximum_s_value.clone(),
                "ok".to_string(),
            ]),
            SMOOTHIEWARE_PROPORTIONAL_POWER_QUERY => state.rx.extend([
                self.configuration.laser_module_proportional_power.clone(),
                "ok".to_string(),
            ]),
            SMOOTHIEWARE_CANCEL_COMMAND => {}
            _ if self.auto_acknowledge => state.rx.push_back("ok".to_string()),
            _ => {}
        }
        Ok(())
    }

    fn read_available(&mut self) -> Result<Vec<u8>, SerialError> {
        if !self.open {
            return Err(SerialError::NotOpen);
        }
        let mut state = self.state.lock().unwrap();
        let mut bytes = Vec::new();
        while let Some(line) = state.rx.pop_front() {
            bytes.extend_from_slice(line.as_bytes());
            bytes.push(b'\n');
        }
        Ok(bytes)
    }

    fn read_line(&mut self) -> Result<Option<String>, SerialError> {
        if !self.open {
            return Err(SerialError::NotOpen);
        }
        Ok(self.state.lock().unwrap().rx.pop_front())
    }

    fn flush(&mut self) -> Result<(), SerialError> {
        if !self.open {
            return Err(SerialError::NotOpen);
        }
        Ok(())
    }

    fn port_name(&self) -> &str {
        "smoothieware-session-corpus"
    }
}

#[test]
fn versioned_smoothieware_live_session_transcripts_are_stable() {
    let corpus: Corpus =
        serde_json::from_str(include_str!("fixtures/session_transcripts.json")).unwrap();
    assert_eq!(corpus.schema_version, 1);
    assert_eq!(corpus.provenance.len(), 4);

    for case in corpus.cases {
        let state = Arc::new(Mutex::new(ScriptState::default()));
        let transport = CorpusTransport {
            open: false,
            state: Arc::clone(&state),
            identity_lines: corpus.identity_lines.clone(),
            configuration: Configuration {
                laser_module_enable: corpus.configuration.laser_module_enable.clone(),
                laser_module_maximum_s_value: corpus
                    .configuration
                    .laser_module_maximum_s_value
                    .clone(),
                laser_module_proportional_power: corpus
                    .configuration
                    .laser_module_proportional_power
                    .clone(),
            },
            auto_acknowledge: case.auto_acknowledge,
        };
        let config = SmoothiewareSerialSessionConfig {
            acknowledgement_flow: AckFlowConfig {
                max_line_bytes: 256,
                acknowledgement_timeout: Duration::from_secs(1),
            },
            probe_timeout: Duration::from_millis(50),
            poll_interval: Duration::ZERO,
        };
        let mut session = SmoothiewareSerialSession::new(Box::new(transport), config);
        session
            .connect()
            .unwrap_or_else(|error| panic!("{}: {error}", case.name));
        session
            .probe_identity()
            .unwrap_or_else(|error| panic!("{}: {error}", case.name));
        session
            .activate()
            .unwrap_or_else(|error| panic!("{}: {error}", case.name));
        session
            .start_job(case.commands.clone())
            .unwrap_or_else(|error| panic!("{}: {error}", case.name));

        let started = Instant::now();
        if case.cancel_after_first_tick {
            session.tick(started).unwrap();
            session.cancel_job().unwrap();
        } else {
            // The ack-gated pump may finish the whole job in one tick when
            // acknowledgements arrive synchronously; tick only while running.
            for index in 0..case.commands.len() {
                if session.state() != SmoothiewareSessionState::Running {
                    break;
                }
                session
                    .tick(started + Duration::from_millis(index as u64))
                    .unwrap_or_else(|error| panic!("{}: {error}", case.name));
            }
        }

        assert_eq!(session.state(), case.expected_state, "case: {}", case.name);
        assert_eq!(
            session.job_outcome(),
            Some(case.expected_outcome),
            "case: {}",
            case.name
        );
        assert_eq!(
            state.lock().unwrap().tx.clone(),
            case.expected_sent_lines,
            "case: {}",
            case.name
        );
    }
}
