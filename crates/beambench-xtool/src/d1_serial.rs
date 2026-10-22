//! Native D-series USB, independently implemented from the public Studio
//! protocol definitions. See docs/machines/xtool-d1-usb-protocol.md.
use std::time::{Duration, Instant};

use crate::{D1_MAX_JOB_BYTES, D1CompiledJob, D1Identity, D1Model, D1RuntimePhase};

pub const D1_SERIAL_BAUD: u32 = 230_400;
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(5);
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(1);

pub trait D1SerialIo: Send {
    fn write(&mut self, bytes: &[u8]) -> Result<(), String>;
    /// Nonblocking. An unplug, invalid handle or read failure must be an error.
    fn read_available(&mut self) -> Result<Vec<u8>, String>;
    fn stop_requested(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone)]
pub struct D1SerialSnapshot {
    pub phase: D1RuntimePhase,
    pub sent_lines: usize,
    pub acknowledged_lines: usize,
    pub error: Option<String>,
}

pub struct D1SerialRuntime<I> {
    io: I,
    identity: Option<D1Identity>,
    snapshot: D1SerialSnapshot,
    buffer: Vec<u8>,
    lines: Vec<String>,
    pending_since: Option<Instant>,
    control_since: Option<Instant>,
    last_heartbeat: Instant,
    terminal: bool,
    cancelled: bool,
    owned: bool,
    armed_echo: bool,
    stopping: bool,
    pause_to_send: bool,
    stop_mode_echo: bool,
    query_active: bool,
    query_ack: bool,
    mode_wait: bool,
    mode_ack: bool,
}

impl<I: D1SerialIo> D1SerialRuntime<I> {
    pub fn new(io: I) -> Self {
        Self {
            io,
            identity: None,
            snapshot: D1SerialSnapshot {
                phase: D1RuntimePhase::Disconnected,
                sent_lines: 0,
                acknowledged_lines: 0,
                error: None,
            },
            buffer: Vec::new(),
            lines: Vec::new(),
            pending_since: None,
            control_since: None,
            last_heartbeat: Instant::now(),
            terminal: false,
            cancelled: false,
            owned: false,
            armed_echo: false,
            stopping: false,
            pause_to_send: false,
            stop_mode_echo: false,
            query_active: false,
            query_ack: false,
            mode_wait: false,
            mode_ack: false,
        }
    }

    pub fn identity(&self) -> Option<&D1Identity> {
        self.identity.as_ref()
    }
    pub fn snapshot(&self) -> D1SerialSnapshot {
        self.snapshot.clone()
    }
    pub fn has_pending_job(&self) -> bool {
        self.owned
    }

    fn fail<T>(&mut self, message: impl Into<String>) -> Result<T, String> {
        let message = message.into();
        self.snapshot.phase = D1RuntimePhase::RecoveryRequired;
        self.snapshot.error = Some(message.clone());
        Err(message)
    }

    fn write(&mut self, line: &str) -> Result<(), String> {
        if !self.stopping && self.io.stop_requested() {
            return self.fail("D1 USB operation interrupted by Stop");
        }
        if line.len() > 4095 || line.contains(['\r', '\n']) || !line.is_ascii() {
            return self.fail("Invalid D1 USB command");
        }
        if let Err(e) = self.io.write(format!("{line}\n").as_bytes()) {
            return self.fail(e);
        }
        Ok(())
    }

    fn receive(&mut self) -> Result<Vec<String>, String> {
        let bytes = match self.io.read_available() {
            Ok(b) => b,
            Err(e) => return self.fail(e),
        };
        if bytes.len() > 65536 || self.buffer.len() + bytes.len() > 65536 {
            return self.fail("D1 USB input exceeded its bounded buffer");
        }
        self.buffer.extend(bytes);
        let mut lines = Vec::new();
        while let Some(end) = self.buffer.iter().position(|b| *b == b'\n') {
            if end > 4095 {
                return self.fail("D1 USB sent an oversized line");
            }
            let bytes: Vec<_> = self.buffer.drain(..=end).collect();
            let line = match std::str::from_utf8(&bytes) {
                Ok(s) if s.is_ascii() => s.trim().to_owned(),
                _ => return self.fail("D1 USB sent invalid text"),
            };
            if !line.is_empty() {
                lines.push(line);
            }
        }
        if self.buffer.len() > 4095 {
            return self.fail("D1 USB sent an unterminated line");
        }
        Ok(lines)
    }

    fn event(&mut self, line: &str) -> Result<(), String> {
        use D1RuntimePhase::*;
        match line {
            "ok" | "ok512" => {
                if self.query_active {
                    self.query_ack = true;
                    return Ok(());
                }
                if self.mode_wait {
                    self.mode_ack = true;
                    return Ok(());
                }
                // Queries/mode controls have their own named replies. Only
                // one program line may be in flight, so extra ACKs fail closed.
                if self.pending_since.take().is_some() {
                    self.snapshot.acknowledged_lines += 1;
                } else if self.owned
                    && self.snapshot.sent_lines > 0
                    && self.snapshot.phase == Running
                    && !self.stopping
                {
                    return self.fail("Unexpected D1 USB acknowledgement");
                }
            }
            "M8 N11" => self.armed_echo = true,
            "M8 N1" => {
                if self.owned && !self.stopping {
                    return self.fail("D1 USB left job mode unexpectedly");
                }
                self.stop_mode_echo = true;
            }
            "start:workingOnline" => {
                if self.stopping {
                    return Ok(());
                }
                if self.owned
                    && matches!(self.snapshot.phase, ReadyToRun | Paused | ResumeRequested)
                    && self.armed_echo
                {
                    self.snapshot.phase = Running;
                    self.control_since = None;
                    if self.pending_since.is_some() {
                        self.pending_since = Some(Instant::now());
                    }
                } else {
                    return self.fail("D1 USB started outside the armed job");
                }
            }
            "ok:WORKING_ONLINE" => {
                if self.stopping {
                    return Ok(());
                }
                if self.owned
                    && matches!(
                        self.snapshot.phase,
                        Paused | ResumeRequested | PauseRequested | Running
                    )
                {
                    self.snapshot.phase = Running;
                    self.control_since = None;
                    if self.pending_since.is_some() {
                        self.pending_since = Some(Instant::now());
                    }
                } else {
                    return self.fail("Unexpected D1 USB running event");
                }
            }
            "ok:PAUSING" => {
                if self.stopping {
                    return Ok(());
                }
                if self.owned && matches!(self.snapshot.phase, Running | PauseRequested) {
                    self.snapshot.phase = Paused;
                    self.control_since = None;
                } else {
                    return self.fail("Unexpected D1 USB pause event");
                }
            }
            "WORK_STOPED" | "WORK_STOPPED" | "M314 N3" => {
                self.cancelled = true;
                if !self.stopping {
                    self.snapshot.phase = RecoveryRequired;
                }
            }
            "ok:IDLE" | "MSG:Pgm End" | "[MSG:Pgm End]" | "ok[MSG:Pgm End]" => {
                if line == "ok[MSG:Pgm End]" && self.pending_since.is_some() {
                    self.event("ok")?;
                }
                if self.owned && !self.stopping && !self.cancelled {
                    if self.snapshot.sent_lines != self.lines.len()
                        || self.snapshot.phase != Running
                    {
                        return self.fail("D1 USB ended before the complete program was sent");
                    }
                    self.terminal = true;
                }
            }
            "M28" => {} // Also emitted after homing. Never proof of this job's completion.
            s if s.starts_with("err:")
                || s.starts_with("error:")
                || s.starts_with("Error:")
                || s.starts_with("ALARM")
                || matches!(
                    s,
                    "ok:ERROR" | "ok:TMP_ERROR" | "ok:KEY_LOCK_ERROR" | "M66 S1"
                ) =>
            {
                return self.fail(format!("D1 USB alarm: {s}"));
            }
            s if s.starts_with("ok:") || s.starts_with("start:") || s.starts_with("okok") => {
                return self.fail(format!("Unknown D1 USB state: {s}"));
            }
            // Named query replies are consumed by query(). Position-only
            // reports and diagnostics cannot establish readiness/completion.
            _ => {}
        }
        Ok(())
    }

    fn drain(&mut self) -> Result<(), String> {
        for line in self.receive()? {
            self.event(&line)?;
        }
        Ok(())
    }

    fn query(&mut self, command: &str) -> Result<String, String> {
        self.drain()?;
        self.query_active = true;
        self.query_ack = false;
        self.write(command)?;
        let start = Instant::now();
        let mut reply = None;
        loop {
            if !self.stopping && self.io.stop_requested() {
                return self.fail("D1 USB query interrupted by Stop");
            }
            for line in self.receive()? {
                if let Some(value) = line.strip_prefix(&format!("{command} ")) {
                    if reply.replace(value.to_owned()).is_some() {
                        return self.fail("Duplicate D1 USB query reply");
                    }
                } else {
                    self.event(&line)?;
                }
            }
            if self.query_ack
                && let Some(value) = reply.take()
            {
                self.query_active = false;
                self.last_heartbeat = Instant::now();
                return Ok(value);
            }
            if start.elapsed() >= RESPONSE_TIMEOUT {
                return self.fail(format!("D1 USB query {command} timed out"));
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn idle(&mut self) -> Result<bool, String> {
        let reply = self.query("M96")?;
        if reply == "N0" {
            return Ok(true);
        }
        if reply
            .strip_prefix('N')
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        {
            return Ok(false);
        }
        self.fail("Unknown D1 USB working-state reply")
    }

    pub fn connect(&mut self) -> Result<D1Identity, String> {
        let version = self.query("M99")?;
        // These are firmware family identifiers, not the editable device name.
        let model = if version.starts_with("V40.30.") {
            D1Model::D1
        } else if version.starts_with("V40.31.") {
            D1Model::D1Pro
        } else {
            return self.fail("USB firmware is not a supported D1 / D1 Pro family");
        };
        let power = self.query("M116")?;
        let parts: Vec<_> = power.split_ascii_whitespace().collect();
        let watts = parts
            .get(1)
            .and_then(|p| p.strip_prefix('Y'))
            .and_then(|p| p.parse::<u16>().ok());
        let supported = match model {
            D1Model::D1 => matches!(watts, Some(5 | 10)),
            D1Model::D1Pro => matches!(watts, Some(5 | 10 | 20 | 40)),
        };
        if parts.len() != 2 || parts[0] != "X0" || !supported {
            return self.fail("Unsupported D1 USB laser module; stock blue-diode modules only");
        }
        if !self.idle()? {
            return self
                .fail("D1 USB is busy; finish or stop work in its current application first");
        }
        // Native mode entry is a documented command, never a GRBL reset.
        self.write("M112 N0")?;
        self.write("M9 S0 N0")?;
        if !self.idle()? {
            return self.fail("D1 USB did not remain idle during connection");
        }
        let identity = D1Identity {
            model,
            power_watts: watts.unwrap(),
            firmware_version: Some(version),
        };
        self.identity = Some(identity.clone());
        self.snapshot.phase = D1RuntimePhase::Ready;
        Ok(identity)
    }

    pub fn arm(&mut self, job: &D1CompiledJob) -> Result<(), String> {
        if !matches!(
            self.snapshot.phase,
            D1RuntimePhase::Ready | D1RuntimePhase::Completed | D1RuntimePhase::Stopped
        ) {
            return Err("D1 USB is not ready for a new job".into());
        }
        if job.gcode.len() > D1_MAX_JOB_BYTES || job.gcode.is_empty() {
            return Err("Invalid D1 USB program size".into());
        }
        let mut lines: Vec<_> = job.gcode.lines().map(str::to_owned).collect();
        if lines.len() != job.line_count
            || lines
                .iter()
                .any(|l| l.is_empty() || !l.is_ascii() || l.len() > 4095)
            || lines.first().map(String::as_str) != Some("M17 S1")
            || lines.last().map(String::as_str) != Some("M18")
        {
            return Err("D1 USB requires a complete native compiled program".into());
        }
        // Same native head as Studio: processing S1, laser-off framing S0.
        lines.insert(1, format!("M207 S{}", u8::from(!job.frame)));
        if !self.idle()? {
            return self.fail("D1 USB became busy before arming");
        }
        self.lines = lines;
        self.snapshot.sent_lines = 0;
        self.snapshot.acknowledged_lines = 0;
        self.terminal = false;
        self.cancelled = false;
        self.armed_echo = false;
        self.owned = true;
        self.snapshot.phase = D1RuntimePhase::ReadyToRun;
        self.mode_wait = true;
        self.mode_ack = false;
        self.write("M8 N11")?;
        let since = Instant::now();
        while !self.armed_echo || !self.mode_ack {
            if self.io.stop_requested() {
                return self.fail("D1 USB arming interrupted by Stop");
            }
            self.drain()?;
            if since.elapsed() > RESPONSE_TIMEOUT {
                return self.fail("D1 USB did not confirm job mode");
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        self.mode_wait = false;
        // No program bytes go out until the physical start event is observed.
        Ok(())
    }

    pub fn tick(&mut self) -> Result<(), String> {
        if self.io.stop_requested() {
            return self.fail("D1 USB streaming interrupted by Stop");
        }
        self.drain()?;
        if self.snapshot.phase == D1RuntimePhase::RecoveryRequired {
            return Err(self
                .snapshot
                .error
                .clone()
                .unwrap_or_else(|| "D1 USB stopped outside Beam Bench".into()));
        }
        if self
            .control_since
            .is_some_and(|t| t.elapsed() > RESPONSE_TIMEOUT)
        {
            return self.fail("D1 USB did not confirm pause");
        }
        if !matches!(
            self.snapshot.phase,
            D1RuntimePhase::Paused | D1RuntimePhase::ResumeRequested
        ) && self
            .pending_since
            .is_some_and(|t| t.elapsed() > RESPONSE_TIMEOUT)
        {
            return self.fail("D1 USB program acknowledgement timed out");
        }
        if self.pause_to_send && self.pending_since.is_none() {
            self.pause_to_send = false;
            self.write("M22 S1")?;
        }
        if self.terminal
            && self.pending_since.is_none()
            && self.snapshot.acknowledged_lines == self.lines.len()
        {
            if !self.idle()? {
                return self.fail("D1 USB completion was not confirmed idle");
            }
            self.owned = false;
            self.lines.clear();
            self.snapshot.phase = D1RuntimePhase::Completed;
            return Ok(());
        }
        if self.snapshot.phase == D1RuntimePhase::Running
            && self.pending_since.is_none()
            && self.snapshot.sent_lines < self.lines.len()
        {
            let line = self.lines[self.snapshot.sent_lines].clone();
            self.write(&line)?;
            self.snapshot.sent_lines += 1;
            self.pending_since = Some(Instant::now());
        } else if self.pending_since.is_none()
            && self.last_heartbeat.elapsed() >= HEARTBEAT_INTERVAL
        {
            let idle = self.idle()?;
            if self.snapshot.phase == D1RuntimePhase::Running && idle && !self.terminal {
                return self.fail("D1 USB became idle without a completion event");
            }
            if matches!(
                self.snapshot.phase,
                D1RuntimePhase::Ready | D1RuntimePhase::Completed | D1RuntimePhase::Stopped
            ) && !idle
            {
                return self.fail("D1 USB started unowned work");
            }
        }
        Ok(())
    }

    pub fn pause(&mut self) -> Result<(), String> {
        if self.snapshot.phase != D1RuntimePhase::Running {
            return Err("D1 USB is not running".into());
        }
        self.snapshot.phase = D1RuntimePhase::PauseRequested;
        self.control_since = Some(Instant::now());
        self.pause_to_send = true;
        Ok(())
    }

    /// Studio resumes D1 USB through the physical button. M22 S0 is only
    /// established for other protocol families, so do not send it here.
    pub fn resume(&mut self) -> Result<(), String> {
        if self.snapshot.phase != D1RuntimePhase::Paused {
            return Err("D1 USB is not paused".into());
        }
        self.snapshot.phase = D1RuntimePhase::ResumeRequested;
        Ok(())
    }

    pub fn stop(&mut self) -> Result<(), String> {
        // Stop sending immediately. Never retry a program line, reset, reopen,
        // or resume after an ambiguous write or acknowledgement.
        self.stopping = true;
        self.query_active = false;
        self.mode_wait = false;
        self.owned = false;
        self.lines.clear();
        self.pending_since = None;
        self.control_since = None;
        self.pause_to_send = false;
        // Output-off is attempted even if cancel fails.
        let stop = self.write("M108");
        let off = self.write("M9 S0 N0");
        self.stop_mode_echo = false;
        let mode = self.write("M8 N1");
        stop.and(off).and(mode)?;
        let since = Instant::now();
        while !self.stop_mode_echo {
            self.drain()?;
            if since.elapsed() >= RESPONSE_TIMEOUT {
                return self
                    .fail("D1 USB did not confirm exit from job mode; use the physical stop");
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        if !self.idle()? {
            return self.fail("D1 USB stop could not be confirmed idle; use the physical stop");
        }
        self.stopping = false;
        self.snapshot.phase = D1RuntimePhase::Stopped;
        self.snapshot.error = None;
        Ok(())
    }
}
