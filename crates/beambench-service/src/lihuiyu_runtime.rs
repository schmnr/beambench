use std::collections::HashMap;
use std::time::Instant;

use beambench_common::{
    ControllerDriverId, ControllerEvidenceState, ControllerFamily, ControllerModel,
    ControllerProductTier, DeviceCapabilities, JobProgress, JobState, MachinePosition,
    MachineRunState, MachineStatus, PositiveControllerIdentity, SessionState,
};
use beambench_lihuiyu::{
    LIHUIYU_M2_NANO_TARGET, LihuiyuCompiledJob, LihuiyuRuntime, LihuiyuRuntimeConfig,
    LihuiyuRuntimePhase, LihuiyuRuntimeSnapshot, LihuiyuTransferConfig, LihuiyuUsbDeviceInfo,
    LihuiyuUsbIo, M2_MILLIMETRES_PER_STEP,
};

type RuntimeIo = Box<dyn LihuiyuUsbIo + Send>;

pub struct LihuiyuRuntimeSession {
    runtime: LihuiyuRuntime<RuntimeIo>,
    device: LihuiyuUsbDeviceInfo,
    capabilities: DeviceCapabilities,
    machine_status: MachineStatus,
    connected: bool,
}

impl LihuiyuRuntimeSession {
    pub fn connect(
        io: impl LihuiyuUsbIo + Send + 'static,
        device: LihuiyuUsbDeviceInfo,
    ) -> Result<Self, String> {
        let mut runtime = LihuiyuRuntime::new(
            Box::new(io) as RuntimeIo,
            LihuiyuTransferConfig::default(),
            LihuiyuRuntimeConfig::default(),
        )
        .map_err(|error| error.to_string())?;
        let snapshot = runtime.connect().map_err(|error| error.to_string())?;
        if snapshot.phase != LihuiyuRuntimePhase::Ready {
            return Err(format!(
                "Lihuiyu controller is not idle after identity validation ({:?})",
                snapshot.phase
            ));
        }
        let machine_status = machine_status_from_snapshot(&snapshot);
        Ok(Self {
            runtime,
            device,
            capabilities: LIHUIYU_M2_NANO_TARGET.capabilities.clone(),
            machine_status,
            connected: true,
        })
    }

    pub const fn driver(&self) -> ControllerDriverId {
        ControllerDriverId::Lihuiyu
    }

    pub const fn controller_model(&self) -> ControllerModel {
        ControllerModel::LihuiyuM2Nano
    }

    pub const fn product_tier(&self) -> ControllerProductTier {
        LIHUIYU_M2_NANO_TARGET.product_tier
    }

    pub const fn evidence_state(&self) -> ControllerEvidenceState {
        LIHUIYU_M2_NANO_TARGET.evidence_state
    }

    pub fn capabilities(&self) -> DeviceCapabilities {
        self.capabilities.clone()
    }

    pub fn detected_identity(&self) -> PositiveControllerIdentity {
        let status = self
            .runtime
            .snapshot()
            .last_status
            .map(|status| format!("{status:?}"))
            .unwrap_or_else(|| "recognized".to_string());
        PositiveControllerIdentity {
            family: ControllerFamily::Dsp,
            model: ControllerModel::LihuiyuM2Nano,
            firmware_identity: Some("Lihuiyu M2-compatible controller".to_string()),
            firmware_version: None,
            evidence: vec![format!(
                "CH341 {:04x}:{:04x} completed EPP 1.9 initialization and returned {status} status",
                self.device.vendor_id, self.device.product_id
            )],
        }
    }

    pub fn session_state(&self) -> SessionState {
        if !self.connected {
            return SessionState::Disconnected;
        }
        match self.runtime.phase() {
            LihuiyuRuntimePhase::Ready
            | LihuiyuRuntimePhase::Completed
            | LihuiyuRuntimePhase::Stopped => SessionState::Ready,
            LihuiyuRuntimePhase::Paused => SessionState::Paused,
            LihuiyuRuntimePhase::Disconnected => SessionState::Disconnected,
            LihuiyuRuntimePhase::RecoveryRequired | LihuiyuRuntimePhase::PowerFault => {
                SessionState::Error
            }
            LihuiyuRuntimePhase::BusyUnowned => SessionState::Validating,
            LihuiyuRuntimePhase::Transferring | LihuiyuRuntimePhase::Running => {
                SessionState::Running
            }
        }
    }

    pub fn machine_status(&self) -> &MachineStatus {
        &self.machine_status
    }

    pub fn controller_info(&self) -> HashMap<String, String> {
        let mut info = HashMap::from([
            (
                "Controller".to_string(),
                LIHUIYU_M2_NANO_TARGET.name.to_string(),
            ),
            (
                "USB ID".to_string(),
                format!(
                    "{:04x}:{:04x}",
                    self.device.vendor_id, self.device.product_id
                ),
            ),
            ("Transport".to_string(), "CH341 USB / EPP 1.9".to_string()),
            ("USB path".to_string(), self.device.stable_id()),
            ("Support tier".to_string(), "Experimental".to_string()),
            (
                "Power control".to_string(),
                "Binary beam; power set on machine panel".to_string(),
            ),
        ]);
        if let Some(manufacturer) = &self.device.manufacturer {
            info.insert("USB manufacturer".to_string(), manufacturer.clone());
        }
        if let Some(product) = &self.device.product {
            info.insert("USB product".to_string(), product.clone());
        }
        if let Some(driver) = &self.device.driver {
            info.insert("USB driver".to_string(), driver.clone());
        }
        if let Some(status) = self.runtime.snapshot().last_status {
            info.insert("Controller status".to_string(), format!("{status:?}"));
        }
        info
    }

    pub fn poll(&mut self) -> Result<LihuiyuRuntimeSnapshot, String> {
        let snapshot = self.runtime.poll().map_err(|error| error.to_string())?;
        self.machine_status = machine_status_from_snapshot(&snapshot);
        Ok(snapshot)
    }

    pub fn home(&mut self) -> Result<(), String> {
        let snapshot = self.runtime.home().map_err(|error| error.to_string())?;
        self.machine_status = machine_status_from_snapshot(&snapshot);
        Ok(())
    }

    pub fn unlock(&mut self) -> Result<(), String> {
        let snapshot = self.runtime.unlock().map_err(|error| error.to_string())?;
        self.machine_status = machine_status_from_snapshot(&snapshot);
        Ok(())
    }

    pub fn jog(&mut self, x_mm: f64, y_mm: f64) -> Result<(), String> {
        let snapshot = self
            .runtime
            .jog(x_mm, y_mm)
            .map_err(|error| error.to_string())?;
        self.machine_status = machine_status_from_snapshot(&snapshot);
        Ok(())
    }

    /// Stage the job for tick-driven transfer. Nothing is uploaded here: the
    /// job tick loop advances the transfer in bounded steps so emergency
    /// stop, cancel, and status stay reachable between ticks.
    pub fn start_job(
        &mut self,
        compiled: &LihuiyuCompiledJob,
        frame: bool,
    ) -> Result<LihuiyuRuntimeJob, String> {
        let snapshot = self
            .runtime
            .begin_job_transfer(compiled)
            .map_err(|error| error.to_string())?;
        self.machine_status = machine_status_from_snapshot(&snapshot);
        Ok(LihuiyuRuntimeJob {
            // Preparing keeps Stop available but Pause disabled in the UI:
            // the runtime cannot pause a staged transfer, only a running job.
            state: JobState::Preparing,
            started_at: Instant::now(),
            acknowledged_packets: 0,
            total_packets: compiled.packets.len(),
            error_message: None,
            frame,
        })
    }

    pub fn is_transferring(&self) -> bool {
        self.runtime.phase() == LihuiyuRuntimePhase::Transferring
    }

    pub fn advance_transfer(&mut self) -> Result<LihuiyuRuntimeSnapshot, String> {
        let snapshot = self
            .runtime
            .advance_transfer()
            .map_err(|error| error.to_string())?;
        self.machine_status = machine_status_from_snapshot(&snapshot);
        Ok(snapshot)
    }

    pub fn pause_job(&mut self) -> Result<(), String> {
        let snapshot = self.runtime.pause().map_err(|error| error.to_string())?;
        self.machine_status = machine_status_from_snapshot(&snapshot);
        Ok(())
    }

    pub fn resume_job(&mut self) -> Result<(), String> {
        let snapshot = self.runtime.resume().map_err(|error| error.to_string())?;
        self.machine_status = machine_status_from_snapshot(&snapshot);
        Ok(())
    }

    pub fn cancel_job(&mut self) -> Result<(), String> {
        let snapshot = if self.is_transferring() {
            self.runtime.abort_transfer()
        } else {
            self.runtime.cancel()
        }
        .map_err(|error| error.to_string())?;
        self.machine_status = machine_status_from_snapshot(&snapshot);
        Ok(())
    }

    pub fn emergency_stop(&mut self) -> Result<(), String> {
        let snapshot = match self.runtime.phase() {
            LihuiyuRuntimePhase::Running | LihuiyuRuntimePhase::Paused => {
                self.runtime.cancel()
            }
            LihuiyuRuntimePhase::Ready
            | LihuiyuRuntimePhase::Completed
            | LihuiyuRuntimePhase::Stopped => self.runtime.output_off(),
            LihuiyuRuntimePhase::Disconnected => return Ok(()),
            LihuiyuRuntimePhase::BusyUnowned => {
                return Err(
                    "Lihuiyu reports controller activity that Beam Bench did not start; use the machine's physical stop"
                        .to_string(),
                );
            }
            // A tick-driven transfer is ours to abort; the runtime then
            // confirms the stop through the owned-job cancel path.
            LihuiyuRuntimePhase::Transferring => self.runtime.abort_transfer(),
            LihuiyuRuntimePhase::RecoveryRequired => {
                return Err(
                    "Lihuiyu could not confirm a software stop; use the machine's physical stop"
                        .to_string(),
                );
            }
            LihuiyuRuntimePhase::PowerFault => {
                return Err(
                    "Lihuiyu reports a power fault; use the machine's physical stop and reconnect"
                        .to_string(),
                );
            }
        }
        .map_err(|error| error.to_string())?;
        self.machine_status = machine_status_from_snapshot(&snapshot);
        Ok(())
    }

    pub fn disconnect(&mut self) -> Result<(), String> {
        self.emergency_stop()?;
        self.connected = false;
        self.machine_status = MachineStatus::default();
        Ok(())
    }
}

pub struct LihuiyuRuntimeJob {
    state: JobState,
    started_at: Instant,
    acknowledged_packets: usize,
    total_packets: usize,
    error_message: Option<String>,
    frame: bool,
}

impl LihuiyuRuntimeJob {
    pub fn tick(&mut self, session: &mut LihuiyuRuntimeSession) -> JobProgress {
        if !matches!(
            self.state,
            JobState::Preparing | JobState::Running | JobState::Paused
        ) {
            return self.progress();
        }
        // While the transfer is staged, each tick advances it by one bounded
        // step; once every packet is acknowledged the runtime enters Running
        // and ticks poll for execution completion as before.
        let result = if session.is_transferring() {
            session.advance_transfer()
        } else {
            session.poll()
        };
        match result {
            Ok(snapshot) => {
                self.acknowledged_packets = snapshot.packets_acknowledged;
                match snapshot.phase {
                    LihuiyuRuntimePhase::Completed => self.state = JobState::Completed,
                    LihuiyuRuntimePhase::Paused => self.state = JobState::Paused,
                    LihuiyuRuntimePhase::Transferring => {
                        self.state = JobState::Preparing;
                    }
                    LihuiyuRuntimePhase::Running => {
                        self.state = JobState::Running;
                    }
                    LihuiyuRuntimePhase::RecoveryRequired | LihuiyuRuntimePhase::PowerFault => {
                        self.state = JobState::Failed;
                        self.error_message = snapshot
                            .recovery_reason
                            .map(|reason| format!("Lihuiyu recovery required: {reason:?}"))
                            .or_else(|| Some(format!("Lihuiyu job entered {:?}", snapshot.phase)));
                    }
                    phase => {
                        self.state = JobState::Failed;
                        self.error_message = Some(format!(
                            "Lihuiyu job entered unexpected runtime phase {phase:?}"
                        ));
                    }
                }
            }
            Err(error) => {
                self.state = JobState::Failed;
                self.error_message = Some(error);
            }
        }
        self.progress()
    }

    pub fn pause(&mut self, session: &mut LihuiyuRuntimeSession) -> Result<JobProgress, String> {
        if session.is_transferring() {
            return Err(
                "The job is still transferring to the controller; stop the job instead of pausing"
                    .to_string(),
            );
        }
        session.pause_job()?;
        self.state = JobState::Paused;
        Ok(self.progress())
    }

    pub fn resume(&mut self, session: &mut LihuiyuRuntimeSession) -> Result<JobProgress, String> {
        session.resume_job()?;
        self.state = JobState::Running;
        Ok(self.progress())
    }

    pub fn cancel(&mut self, session: &mut LihuiyuRuntimeSession) -> Result<JobProgress, String> {
        session.cancel_job()?;
        self.state = JobState::Cancelled;
        Ok(self.progress())
    }

    pub fn progress(&self) -> JobProgress {
        JobProgress {
            state: self.state,
            total_lines: self.total_packets,
            queued_lines: self.total_packets.saturating_sub(self.acknowledged_packets),
            sent_lines: self.acknowledged_packets,
            acknowledged_lines: self.acknowledged_packets,
            elapsed_secs: self.started_at.elapsed().as_secs_f64(),
            estimated_remaining_secs: 0.0,
            buffer_fill_bytes: 0,
            error_message: self.error_message.clone(),
            buckets: Vec::new(),
        }
    }

    pub const fn is_frame(&self) -> bool {
        self.frame
    }
}

fn machine_status_from_snapshot(snapshot: &LihuiyuRuntimeSnapshot) -> MachineStatus {
    let position = MachinePosition {
        x: f64::from(snapshot.commanded_position_steps[0]) * M2_MILLIMETRES_PER_STEP,
        y: f64::from(snapshot.commanded_position_steps[1]) * M2_MILLIMETRES_PER_STEP,
        z: 0.0,
    };
    let run_state = match snapshot.phase {
        LihuiyuRuntimePhase::Ready
        | LihuiyuRuntimePhase::Completed
        | LihuiyuRuntimePhase::Stopped => MachineRunState::Idle,
        LihuiyuRuntimePhase::Paused => MachineRunState::Hold,
        LihuiyuRuntimePhase::RecoveryRequired | LihuiyuRuntimePhase::PowerFault => {
            MachineRunState::Alarm
        }
        LihuiyuRuntimePhase::BusyUnowned => MachineRunState::Unknown,
        LihuiyuRuntimePhase::Disconnected => MachineRunState::Sleep,
        LihuiyuRuntimePhase::Transferring | LihuiyuRuntimePhase::Running => MachineRunState::Run,
    };
    MachineStatus {
        run_state,
        machine_position: position,
        work_position: position,
        feed_rate: 0.0,
        spindle_speed: if snapshot.output_may_be_active {
            1.0
        } else {
            0.0
        },
        feed_override: 100,
        spindle_override: 100,
        rapid_override: 100,
        pin_states: String::new(),
    }
}

#[cfg(test)]
mod tests {
    use beambench_common::{Bounds, Point2D};
    use beambench_lihuiyu::{
        LihuiyuCompilationConfig, LihuiyuVirtualUsbIo, USB_PRODUCT_ID, USB_VENDOR_ID,
        compile_lihuiyu_job,
    };
    use beambench_planner::{ExecutionPlan, PlanSegment};

    use super::*;

    fn device() -> LihuiyuUsbDeviceInfo {
        LihuiyuUsbDeviceInfo {
            bus_id: "virtual".to_string(),
            device_address: 1,
            port_numbers: vec![2, 3],
            vendor_id: USB_VENDOR_ID,
            product_id: USB_PRODUCT_ID,
            manufacturer: Some("Lihuiyu Studio Labs".to_string()),
            product: Some("M2 Nano".to_string()),
            serial_number: None,
            has_required_bulk_endpoints: Some(true),
            driver: Some("virtual-usb".to_string()),
        }
    }

    fn compiled_job() -> LihuiyuCompiledJob {
        let plan = ExecutionPlan {
            id: Default::default(),
            project_id: Default::default(),
            revision_hash: "service-lihuiyu-runtime".to_string(),
            created_at: Default::default(),
            bounds: Bounds::new(Point2D::zero(), Point2D::new(10.0, 10.0)),
            total_distance_mm: 8.0,
            estimated_duration_secs: 1.0,
            segments: vec![PlanSegment::Vector {
                polyline: vec![Point2D::new(1.0, 1.0), Point2D::new(9.0, 1.0)],
                closed: false,
                power_percent: 100.0,
                speed_mm_min: 900.0,
                layer_id: "layer".to_string(),
                cut_entry_id: "entry".to_string(),
                perforation_enabled: false,
                perforation_on_ms: 0.0,
                perforation_off_ms: 0.0,
                source_object_id: None,
                source_subpath_index: None,
            }],
            layer_order: vec!["layer".to_string()],
            warnings: Vec::new(),
            failed_entries: Vec::new(),
        };
        compile_lihuiyu_job(&plan, &LihuiyuCompilationConfig::default()).unwrap()
    }

    #[test]
    fn service_session_exposes_exact_identity_capabilities_and_usb_diagnostics() {
        let session =
            LihuiyuRuntimeSession::connect(LihuiyuVirtualUsbIo::m2_nano(), device()).unwrap();
        assert_eq!(session.session_state(), SessionState::Ready);
        assert_eq!(session.driver(), ControllerDriverId::Lihuiyu);
        assert_eq!(session.controller_model(), ControllerModel::LihuiyuM2Nano);
        assert!(session.capabilities().can_run_job);
        assert!(session.capabilities().can_pause_resume);
        assert!(!session.capabilities().can_set_origin);
        assert_eq!(
            session.detected_identity().model,
            ControllerModel::LihuiyuM2Nano
        );
        let info = session.controller_info();
        assert_eq!(info["USB ID"], "1a86:5512");
        assert_eq!(info["Support tier"], "Experimental");
        assert_eq!(info["USB driver"], "virtual-usb");
    }

    #[test]
    fn service_session_runs_the_lifecycle_and_tracks_finite_position() {
        let mut session =
            LihuiyuRuntimeSession::connect(LihuiyuVirtualUsbIo::m2_nano(), device()).unwrap();
        session.home().unwrap();
        session.unlock().unwrap();
        session.jog(2.54, -1.27).unwrap();
        assert_eq!(session.machine_status().machine_position.x, 2.54);
        assert_eq!(session.machine_status().machine_position.y, -1.27);

        let mut job = session.start_job(&compiled_job(), false).unwrap();
        assert_eq!(job.progress().state, JobState::Preparing);
        assert!(!job.is_frame());
        // The transfer is tick-driven: pump until every packet is acknowledged.
        let mut guard = 0;
        while session.is_transferring() {
            job.tick(&mut session);
            guard += 1;
            assert!(guard < 10_000, "transfer did not complete");
        }
        assert_eq!(job.progress().queued_lines, 0);
        assert_eq!(job.pause(&mut session).unwrap().state, JobState::Paused);
        assert_eq!(job.resume(&mut session).unwrap().state, JobState::Running);
        assert_eq!(job.cancel(&mut session).unwrap().state, JobState::Cancelled);
        assert_eq!(session.session_state(), SessionState::Ready);
        assert_eq!(session.machine_status().spindle_speed, 0.0);
    }
}
