//! Independently authored protocol replay tests. Wire facts and pinned public
//! sources are recorded in docs/machines/xtool-d1-network-protocol.md.
use beambench_xtool::*;
use std::{cell::RefCell, rc::Rc};

#[derive(Default)]
struct Machine {
    model: Option<&'static str>,
    power: Option<u16>,
    working: u8,
    events: Vec<String>,
    file: Vec<u8>,
    requests: Vec<String>,
    posts: usize,
    bad_readback: bool,
    status_lost: bool,
    reject_control: bool,
    start_during_upload: bool,
}
#[derive(Clone, Default)]
struct Io(Rc<RefCell<Machine>>);
fn response(body: serde_json::Value) -> XToolHttpResponse {
    XToolHttpResponse {
        status: 200,
        body: serde_json::to_vec(&body).unwrap(),
    }
}
impl XToolHttpIo for Io {
    fn get(&mut self, path: &str) -> Result<XToolHttpResponse, String> {
        let mut m = self.0.borrow_mut();
        m.requests.push(path.to_owned());
        let ack = serde_json::json!({"result":"ok"});
        Ok(match path {
            "/ping" => response(ack),
            "/getmachinetype" => {
                response(serde_json::json!({"result":"ok","type":m.model.unwrap_or("xTool D1Pro")}))
            }
            "/getlaserpowerinfo" => {
                response(serde_json::json!({"result":"ok","type":0,"power":m.power.unwrap_or(10)}))
            }
            "/system?action=version" => {
                response(serde_json::json!({"result":"ok","version":"V40.31.006.01 B2"}))
            }
            "/peripherystatus" => {
                response(serde_json::json!({"result":"ok","sdCard":1,"status":"normal"}))
            }
            "/system?action=get_working_sta" => {
                response(serde_json::json!({"result":"ok","working":m.working.to_string()}))
            }
            "/progress" => response(serde_json::json!({"progress":100.0,"working":56197,"line":0})),
            "/cnc/data?action=pause" | "/cnc/data?action=resume" => response(if m.reject_control {
                serde_json::json!({"result":"fail"})
            } else {
                ack
            }),
            "/cnc/data?action=stop" => {
                m.working = 0;
                response(ack)
            }
            "/cmd?cmd=M9+S0+N0" => response(ack),
            "/tmp.gcode" => XToolHttpResponse {
                status: 200,
                body: if m.bad_readback {
                    b"old file".to_vec()
                } else {
                    m.file.clone()
                },
            },
            "/delete?file=/tmp.gcode" => {
                m.file.clear();
                response(ack)
            }
            _ => panic!("Unexpected or unsafe request {path}"),
        })
    }
    fn post(
        &mut self,
        path: &str,
        body: &[u8],
        content_type: &str,
    ) -> Result<XToolHttpResponse, String> {
        assert_eq!(path, D1_JOB_UPLOAD_PATH);
        let boundary = content_type.split("boundary=").nth(1).unwrap();
        let payload = String::from_utf8(body.to_vec()).unwrap();
        assert!(payload.contains("name=\"file\"; filename=\"tmp.gcode\""));
        let file = payload
            .split_once("\r\n\r\n")
            .unwrap()
            .1
            .strip_suffix(&format!("\r\n--{boundary}--\r\n"))
            .unwrap();
        let mut m = self.0.borrow_mut();
        m.posts += 1;
        m.file = file.as_bytes().to_vec();
        if m.start_during_upload {
            m.working = 2;
            m.events.push("ok:WORKING_OFFLINE".into());
        }
        Ok(response(serde_json::json!({"result":"ok"})))
    }
}
impl D1NetworkIo for Io {
    fn poll_status_events(&mut self) -> Result<Vec<String>, String> {
        let mut m = self.0.borrow_mut();
        if m.status_lost {
            return Err("socket closed".into());
        }
        Ok(std::mem::take(&mut m.events))
    }
    fn synchronize_status(&mut self) -> Result<Vec<String>, String> {
        self.poll_status_events()
    }
}
fn connected() -> (D1Runtime<Io>, Io) {
    let io = Io::default();
    let mut runtime = D1Runtime::new(io.clone());
    runtime.connect().unwrap();
    (runtime, io)
}
fn job() -> D1CompiledJob {
    D1CompiledJob {
        gcode: "M17 S1\nG1 S0\nG1 X10 Y20 S250\nM18\n".into(),
        line_count: 4,
        frame: false,
    }
}
fn event(io: &Io, text: &str, working: u8) {
    let mut m = io.0.borrow_mut();
    m.events.push(text.into());
    m.working = working;
}

#[test]
fn identity_is_exact_and_original_stock_modules_are_bounded() {
    for model in ["xTool M1", "xTool D1 Pro 2.0", "My D1", "D1"] {
        assert!(
            parse_d1_model(
                200,
                &serde_json::to_vec(&serde_json::json!({"result":"ok","type":model})).unwrap()
            )
            .is_err()
        );
    }
    assert_eq!(
        parse_d1_model(200, br#"{"result":"ok","type":"xTool D1"}"#).unwrap(),
        D1Model::D1
    );
    for (model, power, bed) in [
        (D1Model::D1, 10, (432.0, 406.0)),
        (D1Model::D1Pro, 10, (430.0, 400.0)),
        (D1Model::D1Pro, 20, (430.0, 390.0)),
        (D1Model::D1Pro, 40, (430.0, 348.0)),
    ] {
        assert_eq!(
            D1Identity {
                model,
                power_watts: power,
                firmware_version: None
            }
            .stock_workspace_mm(),
            bed
        );
    }
    let io = Io::default();
    io.0.borrow_mut().power = Some(2);
    assert!(D1Runtime::new(io).connect().is_err());
}
#[test]
fn parsers_reject_unknown_state_invalid_ack_and_stale_progress_is_only_telemetry() {
    for body in [
        b"".as_slice(),
        br#"{}"#,
        br#"{"result":"fail"}"#,
        br#"{"nested":{"result":"ok"}}"#,
    ] {
        assert!(require_d1_ack(200, body).is_err());
    }
    assert!(require_d1_ack(500, br#"{"result":"ok"}"#).is_err());
    assert!(parse_d1_working_state(200, br#"{"result":"ok","working":"3"}"#).is_err());
    assert_eq!(
        parse_d1_status_event("ok:IDLE").unwrap(),
        D1StatusEvent::Idle
    );
    assert_eq!(
        parse_d1_status_event("ok:IDLE\r\n").unwrap(),
        D1StatusEvent::Idle
    );
    assert!(parse_d1_status_event("ok:IDL").is_err());
    assert_eq!(
        parse_d1_progress(200, br#"{"progress":100.00,"working":56197,"line":0}"#)
            .unwrap()
            .elapsed_ms,
        56197
    );
    assert!(parse_d1_progress(200, br#"{"progress":101,"working":0,"line":0}"#).is_err());
    assert!(encode_d1_upload("abc", "abc").is_err());
    assert!(encode_d1_upload("G1", "bad\r\n").is_err());
}
#[test]
fn false_upload_ack_is_never_retried_or_armed() {
    let (mut rt, io) = connected();
    io.0.borrow_mut().bad_readback = true;
    assert!(rt.upload(&job(), "test-boundary").is_err());
    assert_eq!(io.0.borrow().posts, 1);
    assert_eq!(rt.phase(), D1RuntimePhase::RecoveryRequired);
    assert!(rt.upload(&job(), "another-boundary").is_err());
    assert_eq!(io.0.borrow().posts, 1);
}
#[test]
fn physical_button_and_ordered_idle_are_required_for_completion() {
    let (mut rt, io) = connected();
    rt.upload(&job(), "test-boundary").unwrap();
    for _ in 0..3 {
        assert_eq!(rt.poll().unwrap(), D1RuntimePhase::ReadyToRun);
    }
    event(&io, "ok:IDLE", 0);
    assert_eq!(rt.poll().unwrap(), D1RuntimePhase::ReadyToRun);
    event(&io, "ok:WORKING_OFFLINE", 2);
    assert_eq!(rt.poll().unwrap(), D1RuntimePhase::Running);
    assert_eq!(rt.progress().unwrap().percent, 100.0); // stale 100 does not finish
    event(&io, "ok:IDLE", 0);
    assert_eq!(rt.poll().unwrap(), D1RuntimePhase::Completed);
    assert_eq!(rt.poll().unwrap(), D1RuntimePhase::Completed); // terminal remains observable
    event(&io, "ok:WORKING_OFFLINE", 2);
    assert_eq!(rt.poll().unwrap(), D1RuntimePhase::BusyUnowned); // repeat isn't owned
}
#[test]
fn ordered_stop_and_alarm_cannot_complete_a_job() {
    for terminal in ["WORK_STOPPED", "err:flameCheck", "err:limitCheck"] {
        let (mut rt, io) = connected();
        rt.upload(&job(), "test-boundary").unwrap();
        event(&io, "ok:WORKING_OFFLINE", 2);
        rt.poll().unwrap();
        event(&io, terminal, 0);
        event(&io, "ok:IDLE", 0);
        let _ = rt.poll();
        assert_ne!(rt.phase(), D1RuntimePhase::Completed);
    }
}
#[test]
fn status_loss_or_unexplained_http_idle_requires_explicit_recovery() {
    for lose_socket in [false, true] {
        let (mut rt, io) = connected();
        rt.upload(&job(), "test-boundary").unwrap();
        event(&io, "ok:WORKING_OFFLINE", 2);
        rt.poll().unwrap();
        {
            let mut m = io.0.borrow_mut();
            m.status_lost = lose_socket;
            m.working = 0;
        }
        if !lose_socket {
            assert_eq!(rt.poll().unwrap(), D1RuntimePhase::Running);
            std::thread::sleep(std::time::Duration::from_millis(5010));
        }
        assert!(rt.poll().is_err());
        assert_eq!(rt.phase(), D1RuntimePhase::RecoveryRequired);
        io.0.borrow_mut().status_lost = false;
        event(&io, "ok:WORKING_OFFLINE", 2);
        event(&io, "ok:IDLE", 0);
        rt.poll().unwrap();
        assert_eq!(rt.phase(), D1RuntimePhase::RecoveryRequired);
        rt.stop().unwrap();
        assert_eq!(rt.phase(), D1RuntimePhase::Stopped);
        assert!(io.0.borrow().file.is_empty());
    }
}
#[test]
fn pause_and_resume_wait_for_status_and_ambiguous_control_is_not_retryable() {
    let (mut rt, io) = connected();
    rt.upload(&job(), "test-boundary").unwrap();
    event(&io, "ok:WORKING_OFFLINE", 2);
    rt.poll().unwrap();
    rt.pause().unwrap();
    assert_eq!(rt.poll().unwrap(), D1RuntimePhase::PauseRequested);
    event(&io, "ok:PAUSING", 2);
    assert_eq!(rt.poll().unwrap(), D1RuntimePhase::Paused);
    rt.resume().unwrap();
    assert_eq!(rt.poll().unwrap(), D1RuntimePhase::ResumeRequested);
    event(&io, "ok:WORKING_OFFLINE", 2);
    assert_eq!(rt.poll().unwrap(), D1RuntimePhase::Running);
    io.0.borrow_mut().reject_control = true;
    assert!(rt.pause().is_err());
    assert_eq!(rt.phase(), D1RuntimePhase::RecoveryRequired);
}
#[test]
fn cancellation_deletes_only_the_exact_pending_file() {
    for overwritten in [false, true] {
        let (mut rt, io) = connected();
        rt.upload(&job(), "test-boundary").unwrap();
        if overwritten {
            io.0.borrow_mut().file = b"another app's job".to_vec();
        }
        rt.stop().unwrap();
        let m = io.0.borrow();
        assert_eq!(
            m.requests.iter().any(|path| path.starts_with("/delete?")),
            !overwritten
        );
        assert!(m.requests.iter().any(|path| path == "/cmd?cmd=M9+S0+N0"));
    }
    let (mut rt, io) = connected();
    rt.stop().unwrap();
    assert!(
        !io.0
            .borrow()
            .requests
            .iter()
            .any(|path| path.starts_with("/delete?"))
    );
}
#[test]
fn unowned_busy_controller_is_read_only_until_explicit_stop() {
    let (mut rt, io) = connected();
    event(&io, "ok:WORKING_OFFLINE", 2);
    rt.poll().unwrap();
    assert!(rt.upload(&job(), "test-boundary").is_err());
    assert!(rt.pause().is_err());
    assert_eq!(io.0.borrow().posts, 0);
}

#[test]
fn http_may_lead_ordered_events_without_proving_start_or_completion() {
    let (mut rt, io) = connected();
    rt.upload(&job(), "test-boundary").unwrap();
    io.0.borrow_mut().working = 2;
    assert_eq!(rt.poll().unwrap(), D1RuntimePhase::ReadyToRun);
    event(&io, "ok:WORKING_OFFLINE", 2);
    assert_eq!(rt.poll().unwrap(), D1RuntimePhase::Running);
    io.0.borrow_mut().working = 0;
    assert_eq!(rt.poll().unwrap(), D1RuntimePhase::Running);
    event(&io, "ok:IDLE", 0);
    assert_eq!(rt.poll().unwrap(), D1RuntimePhase::Completed);
}

#[test]
fn motion_during_upload_is_not_claimed_as_the_new_owned_job() {
    let (mut rt, io) = connected();
    io.0.borrow_mut().start_during_upload = true;
    assert!(rt.upload(&job(), "test-boundary").is_err());
    assert_eq!(rt.phase(), D1RuntimePhase::RecoveryRequired);
    assert!(rt.has_pending_file());
    rt.stop().unwrap();
    assert!(!rt.has_pending_file());
    assert!(io.0.borrow().file.is_empty());
}
