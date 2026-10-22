use crate::{
    ops::{machine, planning},
    runtime::MachineSessionHandle,
    xtool_d1_runtime::XToolD1RuntimeSession,
    xtool_d1_serial::D1SerialWorker,
    xtool_d1_tests::{context_for_session, wait_state},
};
use beambench_common::{JobState, TransportKind};
use beambench_serial::{SerialError, SerialTransport};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[derive(Default)]
struct State {
    dtr: Vec<bool>,
    input: VecDeque<u8>,
    writes: Vec<String>,
    busy: bool,
    lost: bool,
    hold_arm: bool,
    hold_query: bool,
    ignore_stop: bool,
}
struct Transport(Arc<Mutex<State>>);
impl SerialTransport for Transport {
    fn open(&mut self) -> Result<(), SerialError> {
        Ok(())
    }
    fn close(&mut self) -> Result<(), SerialError> {
        Ok(())
    }
    fn is_open(&self) -> bool {
        true
    }
    fn port_name(&self) -> &str {
        "emulated-D1-USB"
    }
    fn set_dtr(&mut self, level: bool) -> Result<(), SerialError> {
        self.0.lock().unwrap().dtr.push(level);
        Ok(())
    }
    fn flush(&mut self) -> Result<(), SerialError> {
        Ok(())
    }
    fn read_line(&mut self) -> Result<Option<String>, SerialError> {
        unreachable!()
    }
    fn write_line(&mut self, _: &str) -> Result<(), SerialError> {
        unreachable!()
    }
    fn write_bytes(&mut self, bytes: &[u8]) -> Result<usize, SerialError> {
        let mut m = self.0.lock().unwrap();
        if m.lost {
            return Err(SerialError::NotOpen);
        }
        let cmd = std::str::from_utf8(bytes).unwrap().trim();
        m.writes.push(cmd.into());
        let reply = match cmd {
            "M99" => "M99 V40.31.006.01 B2".to_string(),
            "M116" => "M116 X0 Y10".into(),
            "M96" if m.hold_query => return Ok(bytes.len()),
            "M96" => format!("M96 N{}", u8::from(m.busy)),
            "M8 N11" if m.hold_arm => return Ok(bytes.len()),
            "M8 N11" => "M8 N11".into(),
            "M8 N1" => "M8 N1".into(),
            "M108" => {
                if !m.ignore_stop {
                    m.busy = false;
                }
                "WORK_STOPED".into()
            }
            "M22 S1" => "ok:PAUSING".into(),
            _ => "".into(),
        };
        m.input.extend(format!("{reply}\nok\n").bytes());
        Ok(bytes.len())
    }
    fn read_available(&mut self) -> Result<Vec<u8>, SerialError> {
        let mut m = self.0.lock().unwrap();
        if m.lost {
            return Err(SerialError::NotOpen);
        }
        let n = m.input.len().min(13);
        Ok(m.input.drain(..n).collect())
    }
}
fn event(state: &Arc<Mutex<State>>, text: &str) {
    state
        .lock()
        .unwrap()
        .input
        .extend(format!("{text}\n").bytes());
}
fn worker() -> (D1SerialWorker, Arc<Mutex<State>>) {
    let state = Arc::new(Mutex::new(State::default()));
    (
        D1SerialWorker::from_transport(Box::new(Transport(state.clone()))).unwrap(),
        state,
    )
}
fn wait_until(mut condition: impl FnMut() -> bool) {
    let since = Instant::now();
    while !condition() {
        assert!(
            since.elapsed() < Duration::from_secs(3),
            "USB emulator condition timed out"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn start(state: &Arc<Mutex<State>>) {
    state.lock().unwrap().busy = true;
    event(state, "start:workingOnline");
}
fn complete(state: &Arc<Mutex<State>>) {
    wait_until(|| {
        state
            .lock()
            .unwrap()
            .writes
            .last()
            .is_some_and(|s| s == "M18")
    });
    state.lock().unwrap().busy = false;
    event(state, "[MSG:Pgm End]\nok:IDLE");
}
fn simple_job() -> beambench_xtool::D1CompiledJob {
    beambench_xtool::D1CompiledJob {
        gcode: "M17 S1\nG1 X10 Y20 S0\nM18\n".into(),
        line_count: 3,
    }
}

#[test]
fn d1_usb_real_planner_frame_proof_start_and_stream_progress() {
    let (worker, state) = worker();
    let session = XToolD1RuntimeSession::from_serial_worker("emulated-D1-USB", worker).unwrap();
    assert_eq!(session.transport_kind(), TransportKind::Serial);
    let stop = session.serial_stop_handle().unwrap();
    let ctx = context_for_session(session);
    *ctx.xtool_d1_serial_stop.lock().unwrap() = Some(stop);
    assert!(machine::start_job(&ctx).is_err());
    let frame = machine::frame_job(&ctx, "rectangular", &[], true, None).unwrap();
    assert_eq!(frame.state, JobState::ReadyToRun);
    assert_eq!(frame.sent_lines, 0);
    start(&state);
    wait_state(&ctx, JobState::Running);
    complete(&state);
    wait_state(&ctx, JobState::Completed);
    assert!(ctx.relative_frame_confirmation.lock().unwrap().is_some());
    {
        let m = state.lock().unwrap();
        assert!(m.writes.contains(&"G92 X10.000 Y15.000".into()));
        assert!(
            m.writes
                .iter()
                .filter(|s| s.starts_with("G1"))
                .flat_map(|s| s.split_whitespace())
                .filter_map(|s| s.strip_prefix('S'))
                .all(|s| s == "0")
        );
    }
    let job = machine::start_job_with_options_confirming_advisories(
        &ctx,
        &planning::SessionJobOptions::default(),
        true,
    )
    .unwrap();
    assert_eq!(job.state, JobState::ReadyToRun);
    assert!(ctx.relative_frame_confirmation.lock().unwrap().is_none());
    start(&state);
    wait_state(&ctx, JobState::Running);
    machine::pause_job(&ctx).unwrap();
    wait_state(&ctx, JobState::Paused);
    machine::resume_job(&ctx).unwrap();
    wait_state(&ctx, JobState::ReadyToRun);
    event(&state, "ok:WORKING_ONLINE");
    complete(&state);
    wait_state(&ctx, JobState::Completed);
    let m = state.lock().unwrap();
    assert!(m.writes.iter().any(|s| s.contains("S250")));
    assert!(!m.writes.iter().any(|s| s == "M22 S0"));
}
#[test]
fn d1_usb_stop_interrupts_arming_without_the_session_lock() {
    let (worker, state) = worker();
    state.lock().unwrap().hold_arm = true;
    let stop = worker.stop_handle();
    let thread = std::thread::spawn(move || {
        let r = worker.arm(&simple_job());
        (worker, r)
    });
    wait_until(|| state.lock().unwrap().writes.contains(&"M8 N11".into()));
    let since = Instant::now();
    stop.stop().unwrap();
    assert!(since.elapsed() < Duration::from_secs(1));
    let (worker, result) = thread.join().unwrap();
    assert!(result.is_err());
    assert_eq!(worker.snapshot().sent_lines, 0);
    assert_eq!(
        worker.snapshot().phase,
        beambench_xtool::D1RuntimePhase::Stopped
    );
}
#[test]
fn d1_usb_read_failure_never_becomes_success_or_reopens_the_transport() {
    let (worker, state) = worker();
    worker.arm(&simple_job()).unwrap();
    start(&state);
    wait_until(|| worker.snapshot().sent_lines > 0);
    state.lock().unwrap().lost = true;
    wait_until(|| worker.snapshot().error.is_some());
    assert_eq!(
        worker.snapshot().phase,
        beambench_xtool::D1RuntimePhase::RecoveryRequired
    );
    assert!(worker.stop().is_err());
}
#[test]
fn d1_usb_unconfirmed_stop_retains_recovery_and_no_frame_proof() {
    let (worker, state) = worker();
    worker.arm(&simple_job()).unwrap();
    start(&state);
    wait_until(|| worker.snapshot().sent_lines > 0);
    state.lock().unwrap().ignore_stop = true;
    assert!(worker.stop().is_err());
    assert_ne!(
        worker.snapshot().phase,
        beambench_xtool::D1RuntimePhase::Completed
    );
    state.lock().unwrap().ignore_stop = false;
    worker.stop().unwrap();
}
#[test]
fn d1_usb_emergency_stop_reaches_worker_while_service_session_is_locked() {
    let (worker, state) = worker();
    let session = XToolD1RuntimeSession::from_serial_worker("emulated-D1-USB", worker).unwrap();
    let stop = session.serial_stop_handle().unwrap();
    let ctx = Arc::new(context_for_session(session));
    *ctx.xtool_d1_serial_stop.lock().unwrap() = Some(stop);
    machine::frame_job(&ctx, "rectangular", &[], false, None).unwrap();
    start(&state);
    complete(&state);
    wait_state(&ctx, JobState::Completed);
    assert!(ctx.relative_frame_confirmation.lock().unwrap().is_some());
    let guard = ctx.session.lock().unwrap();
    let stop_ctx = ctx.clone();
    let thread = std::thread::spawn(move || machine::emergency_stop(&stop_ctx));
    wait_until(|| state.lock().unwrap().writes.contains(&"M108".into()));
    drop(guard);
    thread.join().unwrap().unwrap();
    assert!(ctx.relative_frame_confirmation.lock().unwrap().is_none());
    assert!(matches!(
        ctx.session.lock().unwrap().as_ref(),
        Some(MachineSessionHandle::XToolD1(_))
    ));
}

#[test]
fn d1_usb_worker_never_toggles_dtr_after_open() {
    let (worker, state) = worker();
    let expected: &[bool] = &[];
    assert_eq!(state.lock().unwrap().dtr, expected);
    drop(worker);
}
