//! Independently authored emulator tests of the published USB contract.
//! These are software tests, not captured hardware-validation claims.
use beambench_xtool::*;
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

struct Machine {
    version: String,
    module: String,
    busy: bool,
    input: VecDeque<u8>,
    writes: Vec<String>,
    fragment: usize,
    lost: bool,
    ack: bool,
    ack_tail: bool,
    query_ack: bool,
    terminal: bool,
    stop_idle: bool,
    interrupt: bool,
    reverse_query: bool,
}
#[derive(Clone)]
struct Io(Arc<Mutex<Machine>>);
impl Default for Io {
    fn default() -> Self {
        Self(Arc::new(Mutex::new(Machine {
            version: "V40.31.006.01 B2".into(),
            module: "X0 Y20".into(),
            busy: false,
            input: VecDeque::new(),
            writes: Vec::new(),
            fragment: usize::MAX,
            lost: false,
            ack: true,
            ack_tail: true,
            query_ack: true,
            terminal: false,
            stop_idle: true,
            interrupt: false,
            reverse_query: false,
        })))
    }
}
impl Io {
    fn event(&self, text: &str) {
        self.0
            .lock()
            .unwrap()
            .input
            .extend(format!("{text}\n").bytes());
    }
    fn start(&self) {
        self.0.lock().unwrap().busy = true;
        self.event("start:workingOnline");
    }
    fn finish(&self) {
        self.0.lock().unwrap().busy = false;
        self.event("[MSG:Pgm End]\nok:IDLE");
    }
}
impl D1SerialIo for Io {
    fn write(&mut self, bytes: &[u8]) -> Result<(), String> {
        assert!(bytes.ends_with(b"\n"));
        let line = std::str::from_utf8(bytes).unwrap().trim();
        let mut m = self.0.lock().unwrap();
        if m.lost {
            return Err("USB unplugged".into());
        }
        m.writes.push(line.to_owned());
        let response = match line {
            "M99" => format!("M99 {}", m.version),
            "M116" => format!("M116 {}", m.module),
            "M96" => format!("M96 N{}", u8::from(m.busy)),
            "M8 N11" => "M8 N11".into(),
            "M8 N1" => "M8 N1".into(),
            "M108" => {
                if m.stop_idle {
                    m.busy = false;
                }
                "WORK_STOPED".into()
            }
            "M22 S1" => "ok:PAUSING".into(),
            "M18" if m.terminal => {
                m.busy = false;
                "ok:IDLE".into()
            }
            _ => String::new(),
        };
        let query = matches!(line, "M99" | "M116" | "M96");
        let ack = if query {
            m.query_ack
        } else {
            m.ack && (line != "M18" || m.ack_tail)
        };
        let reply = match (ack, m.reverse_query && query) {
            (true, true) => format!("ok\n{response}\n"),
            (true, false) => format!("{response}\nok\n"),
            _ => format!("{response}\n"),
        };
        m.input.extend(reply.bytes());
        Ok(())
    }
    fn read_available(&mut self) -> Result<Vec<u8>, String> {
        let mut m = self.0.lock().unwrap();
        if m.lost {
            return Err("USB unplugged".into());
        }
        let n = m.input.len().min(m.fragment);
        Ok(m.input.drain(..n).collect())
    }
    fn stop_requested(&self) -> bool {
        self.0.lock().unwrap().interrupt
    }
}
fn connected() -> (D1SerialRuntime<Io>, Io) {
    let io = Io::default();
    let mut r = D1SerialRuntime::new(io.clone());
    r.connect().unwrap();
    (r, io)
}
fn job(frame: bool) -> D1CompiledJob {
    let gcode = format!(
        "M17 S1\nM106 S0\nM101\nG92 X0 Y0\nG90\nG1 X10 Y20 S{} F1000\nG1 S0\nM18\n",
        if frame { 0 } else { 250 }
    );
    D1CompiledJob {
        line_count: gcode.lines().count(),
        gcode,
        frame,
    }
}
fn send_all(r: &mut D1SerialRuntime<Io>) {
    for _ in 0..100 {
        r.tick().unwrap();
    }
}

#[test]
fn native_identity_accepts_both_firmware_families_in_either_ack_order() {
    for (version, model) in [
        ("V40.30.010.01 B2", D1Model::D1),
        ("V40.31.006.01 B2", D1Model::D1Pro),
    ] {
        for reverse in [false, true] {
            let io = Io::default();
            {
                let mut m = io.0.lock().unwrap();
                m.version = version.into();
                m.module = "X0 Y10".into();
                m.reverse_query = reverse;
                m.fragment = 3;
            }
            let mut r = D1SerialRuntime::new(io);
            assert_eq!(r.connect().unwrap().model, model);
        }
    }
}
#[test]
fn unknown_families_modules_and_busy_connections_are_rejected_without_mode_writes() {
    for (version, module, busy) in [
        ("V40.32.015", "X0 Y10", false),
        ("V40.31.006", "X1 Y2", false),
        ("V40.30.010", "X0 Y40", false),
        ("V40.31.006", "X0 Y20", true),
    ] {
        let io = Io::default();
        {
            let mut m = io.0.lock().unwrap();
            m.version = version.into();
            m.module = module.into();
            m.busy = busy;
        }
        let mut r = D1SerialRuntime::new(io.clone());
        assert!(r.connect().is_err());
        assert!(
            io.0.lock()
                .unwrap()
                .writes
                .iter()
                .all(|s| matches!(s.as_str(), "M99" | "M116" | "M96"))
        );
    }
}
#[test]
fn physical_start_is_required_and_acks_do_not_complete_a_job() {
    let (mut r, io) = connected();
    let job = job(false);
    r.arm(&job).unwrap();
    send_all(&mut r);
    assert!(
        !io.0
            .lock()
            .unwrap()
            .writes
            .iter()
            .any(|s| s.starts_with("G1"))
    );
    io.start();
    send_all(&mut r);
    assert_eq!(r.snapshot().phase, D1RuntimePhase::Running);
    assert_eq!(r.snapshot().acknowledged_lines, job.line_count + 1);
    io.finish();
    r.tick().unwrap();
    assert_eq!(r.snapshot().phase, D1RuntimePhase::Completed);
    assert!(!r.has_pending_job());
}
#[test]
fn frame_and_processing_send_their_native_mode_with_no_speculative_commands() {
    for frame in [true, false] {
        let (mut r, io) = connected();
        r.arm(&job(frame)).unwrap();
        io.start();
        send_all(&mut r);
        io.finish();
        r.tick().unwrap();
        let m = io.0.lock().unwrap();
        assert!(m.writes.contains(&format!("M207 S{}", u8::from(!frame))));
        assert!(
            !m.writes
                .iter()
                .any(|s| s.contains('$') || s == "?" || s == "M22 S0" || s.contains('\u{18}'))
        );
        if frame {
            assert!(
                m.writes
                    .iter()
                    .filter(|s| s.starts_with("G1"))
                    .all(|s| !s.contains("S250"))
            );
        }
    }
}
#[test]
fn completion_waits_for_tail_ack_and_fresh_idle() {
    let (mut r, io) = connected();
    io.0.lock().unwrap().terminal = true;
    r.arm(&job(false)).unwrap();
    io.start();
    send_all(&mut r);
    assert_eq!(r.snapshot().phase, D1RuntimePhase::Completed);
    let m = io.0.lock().unwrap();
    assert_eq!(m.writes.last().unwrap(), "M96");
}
#[test]
fn early_idle_and_unowned_start_cannot_complete() {
    for event in ["ok:IDLE", "[MSG:Pgm End]", "ok:WORKING_OFFLINE"] {
        let (mut r, io) = connected();
        r.arm(&job(false)).unwrap();
        io.event(event);
        assert!(r.tick().is_err());
        assert_ne!(r.snapshot().phase, D1RuntimePhase::Completed);
    }
    let (mut r, io) = connected();
    io.start();
    assert!(r.tick().is_err());
}
#[test]
fn pause_stops_streaming_and_resume_waits_for_physical_button() {
    let (mut r, io) = connected();
    r.arm(&job(false)).unwrap();
    io.start();
    r.tick().unwrap();
    r.pause().unwrap();
    r.tick().unwrap();
    r.tick().unwrap();
    assert_eq!(r.snapshot().phase, D1RuntimePhase::Paused);
    let sent = r.snapshot().sent_lines;
    send_all(&mut r);
    assert_eq!(r.snapshot().sent_lines, sent);
    r.resume().unwrap();
    send_all(&mut r);
    assert_eq!(r.snapshot().phase, D1RuntimePhase::ResumeRequested);
    io.event("ok:WORKING_ONLINE");
    send_all(&mut r);
    assert_eq!(r.snapshot().phase, D1RuntimePhase::Running);
    assert!(!io.0.lock().unwrap().writes.iter().any(|s| s == "M22 S0"));
}
#[test]
fn cancellation_and_alarms_never_become_successful_completion() {
    for e in [
        "WORK_STOPED",
        "WORK_STOPPED",
        "M314 N3",
        "err:limitCheck",
        "err:flameCheck",
        "ok:TMP_ERROR",
        "ok:NEW_STATE",
        "okok",
    ] {
        let (mut r, io) = connected();
        r.arm(&job(false)).unwrap();
        io.start();
        send_all(&mut r);
        io.event(e);
        io.finish();
        assert!(r.tick().is_err());
        assert_ne!(r.snapshot().phase, D1RuntimePhase::Completed);
    }
}
#[test]
fn confirmed_stop_drops_unsent_lines_and_requires_a_new_arm() {
    let (mut r, io) = connected();
    r.arm(&job(false)).unwrap();
    io.start();
    r.tick().unwrap();
    r.stop().unwrap();
    let writes = io.0.lock().unwrap().writes.len();
    send_all(&mut r);
    assert_eq!(io.0.lock().unwrap().writes.len(), writes);
    assert_eq!(r.snapshot().phase, D1RuntimePhase::Stopped);
    assert!(!r.has_pending_job());
    r.arm(&job(true)).unwrap();
    assert_eq!(r.snapshot().sent_lines, 0);
}
#[test]
fn stop_attempts_output_off_even_after_write_failure_and_never_claims_idle() {
    let (mut r, io) = connected();
    r.arm(&job(false)).unwrap();
    io.0.lock().unwrap().lost = true;
    assert!(r.stop().is_err());
    assert_eq!(r.snapshot().phase, D1RuntimePhase::RecoveryRequired);
}
#[test]
fn lost_serial_link_and_oversized_or_malformed_input_fail_closed() {
    for text in ["x".repeat(4097), "x".repeat(70000)] {
        let (mut r, io) = connected();
        io.event(&text);
        assert!(r.tick().is_err());
    }
    let (mut r, io) = connected();
    io.0.lock().unwrap().input.extend([0xff, b'\n']);
    assert!(r.tick().is_err());
    let (mut r, io) = connected();
    io.0.lock().unwrap().lost = true;
    assert!(r.tick().is_err());
}
#[test]
fn emergency_signal_prevents_the_next_program_write() {
    let (mut r, io) = connected();
    r.arm(&job(false)).unwrap();
    io.start();
    io.0.lock().unwrap().interrupt = true;
    assert!(r.tick().is_err());
    assert_eq!(r.snapshot().sent_lines, 0);
    r.stop().unwrap();
    assert_eq!(r.snapshot().phase, D1RuntimePhase::Stopped);
}
#[test]
fn malformed_or_partial_program_is_rejected_before_arming() {
    let (mut r, io) = connected();
    let count = io.0.lock().unwrap().writes.len();
    let mut job = job(false);
    job.gcode.push_str("G1 X10 S1000\n");
    assert!(r.arm(&job).is_err());
    assert_eq!(io.0.lock().unwrap().writes.len(), count);
}

#[test]
fn physical_resume_start_event_never_rewinds_the_program() {
    let (mut r, io) = connected();
    r.arm(&job(false)).unwrap();
    io.start();
    r.tick().unwrap();
    r.pause().unwrap();
    r.tick().unwrap();
    r.tick().unwrap();
    let before = r.snapshot().sent_lines;
    r.resume().unwrap();
    io.event("start:workingOnline");
    send_all(&mut r);
    assert!(r.snapshot().sent_lines > before);
    assert_eq!(
        io.0.lock()
            .unwrap()
            .writes
            .iter()
            .filter(|s| s.as_str() == "M17 S1")
            .count(),
        1
    );
    io.finish();
    r.tick().unwrap();
    assert_eq!(r.snapshot().phase, D1RuntimePhase::Completed);
}
#[test]
fn merged_terminal_after_all_acks_is_not_an_extra_program_ack() {
    let (mut r, io) = connected();
    r.arm(&job(false)).unwrap();
    io.start();
    send_all(&mut r);
    io.0.lock().unwrap().busy = false;
    io.event("ok[MSG:Pgm End]");
    r.tick().unwrap();
    assert_eq!(r.snapshot().phase, D1RuntimePhase::Completed);
}

#[test]
fn named_query_without_acknowledgement_never_establishes_readiness() {
    let io = Io::default();
    io.0.lock().unwrap().query_ack = false;
    let mut r = D1SerialRuntime::new(io.clone());
    assert!(r.connect().is_err());
    assert_eq!(r.snapshot().phase, D1RuntimePhase::RecoveryRequired);
    assert_eq!(io.0.lock().unwrap().writes, ["M99"]);
}
#[test]
fn terminal_event_without_tail_ack_does_not_complete() {
    let (mut r, io) = connected();
    r.arm(&job(false)).unwrap();
    io.start();
    io.0.lock().unwrap().ack_tail = false;
    send_all(&mut r);
    let sent = r.snapshot().sent_lines;
    assert_eq!(r.snapshot().acknowledged_lines, sent - 1);
    io.finish();
    r.tick().unwrap();
    assert_eq!(r.snapshot().phase, D1RuntimePhase::Running);
    io.event("ok");
    r.tick().unwrap();
    assert_eq!(r.snapshot().phase, D1RuntimePhase::Completed);
}
#[test]
fn terminal_event_with_non_idle_query_is_rejected() {
    let (mut r, io) = connected();
    r.arm(&job(false)).unwrap();
    io.start();
    send_all(&mut r);
    io.event("ok:IDLE");
    assert!(r.tick().is_err());
    assert_ne!(r.snapshot().phase, D1RuntimePhase::Completed);
}
