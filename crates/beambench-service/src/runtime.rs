use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use std::time::Instant;

use beambench_common::ConsoleEntry;
use beambench_common::controller_choice::{
    ControllerConnectionEndpoint, ControllerDriverId, ControllerSelection,
    ExplicitControllerSelection, PositiveControllerIdentity, ResolvedControllerChoice,
};
use beambench_common::machine::{
    ControllerEvidenceState, ControllerFamily, ControllerModel, ControllerProductTier,
    DeviceCapabilities, DeviceIdentity, JobProgress, JobState, MachineRunState, MachineStatus,
    SessionState, TransportKind,
};
use beambench_dsp::{DspJob, DspSession};
use beambench_galvo::{GalvoJob, GalvoSession};
use beambench_grbl::{
    FluidNcNetworkAdapter, FluidNcSerialAdapter, GrblFamilyAdapter, GrblFamilyIdentityProbeResult,
    GrblHalNetworkAdapter, GrblHalSerialAdapter, GrblSession,
};
use beambench_marlin::{
    MarlinIdentityProbeResult, MarlinSerialAdapter, MarlinSerialSession, MarlinSessionState,
    SnapmakerSerialAdapter,
};
use beambench_smoothieware::{
    SmoothiewareIdentityProbeResult, SmoothiewareSerialAdapter, SmoothiewareSerialSession,
    SmoothiewareSessionState,
};
use beambench_streamer::JobController;

use crate::{
    lihuiyu_runtime::{LihuiyuRuntimeJob, LihuiyuRuntimeSession},
    ruida_runtime::{RuidaRuntimeJob, RuidaRuntimeSession},
};

fn grbl_capabilities() -> DeviceCapabilities {
    DeviceCapabilities::legacy_grbl()
}

fn dsp_capabilities() -> DeviceCapabilities {
    DeviceCapabilities::default()
}

fn galvo_capabilities() -> DeviceCapabilities {
    DeviceCapabilities::default()
}

/// Runtime policy carried alongside the shared GRBL protocol session.
///
/// This keeps the adapter implementation separate from the identity and
/// capability claims made for the connected controller. An explicit
/// Experimental fallback can therefore use GRBL-compatible wire commands
/// without being mislabeled as a stock GRBL controller.
pub struct GrblRuntimeSession {
    session: GrblSession,
    driver: ControllerDriverId,
    selection: ExplicitControllerSelection,
    detected_identity: Option<PositiveControllerIdentity>,
    controller_model: ControllerModel,
    capabilities: DeviceCapabilities,
    product_tier: Option<ControllerProductTier>,
    evidence_state: Option<ControllerEvidenceState>,
    experimental_mode: bool,
    transport_kind: TransportKind,
}

impl GrblRuntimeSession {
    pub fn legacy(session: GrblSession) -> Self {
        Self {
            session,
            driver: ControllerDriverId::Grbl,
            selection: ExplicitControllerSelection::KnownDriver {
                driver: ControllerDriverId::Grbl,
            },
            detected_identity: None,
            controller_model: ControllerModel::Grbl,
            capabilities: grbl_capabilities(),
            product_tier: None,
            evidence_state: None,
            experimental_mode: false,
            transport_kind: TransportKind::Serial,
        }
    }

    pub fn from_choice(session: GrblSession, choice: &ResolvedControllerChoice) -> Self {
        Self::from_choice_with_transport(session, choice, TransportKind::Serial)
    }

    pub fn from_choice_with_transport(
        session: GrblSession,
        choice: &ResolvedControllerChoice,
        transport_kind: TransportKind,
    ) -> Self {
        let controller_model = choice
            .detected_identity
            .as_ref()
            .map(|identity| identity.model)
            .unwrap_or_else(|| {
                if choice.selection.is_generic() {
                    return ControllerModel::Unknown;
                }
                match choice.driver {
                    ControllerDriverId::Grbl => ControllerModel::Grbl,
                    ControllerDriverId::FluidNc => ControllerModel::FluidNc,
                    ControllerDriverId::GrblHal => ControllerModel::GrblHal,
                    ControllerDriverId::LaserPecker => ControllerModel::LaserPecker,
                    ControllerDriverId::Marlin => ControllerModel::Marlin,
                    ControllerDriverId::Snapmaker => ControllerModel::Snapmaker,
                    ControllerDriverId::Smoothieware => ControllerModel::Smoothieware,
                    ControllerDriverId::Ruida => ControllerModel::Ruida,
                    ControllerDriverId::Lihuiyu => ControllerModel::LihuiyuM2Nano,
                    ControllerDriverId::Unknown => ControllerModel::Unknown,
                }
            });
        let adapter_descriptor = match (choice.driver, transport_kind) {
            (ControllerDriverId::FluidNc, TransportKind::Serial) => {
                Some(FluidNcSerialAdapter::new().descriptor())
            }
            (ControllerDriverId::FluidNc, TransportKind::Tcp) => {
                Some(FluidNcNetworkAdapter::new().descriptor())
            }
            (ControllerDriverId::GrblHal, TransportKind::Serial) => {
                Some(GrblHalSerialAdapter::new().descriptor())
            }
            (ControllerDriverId::GrblHal, TransportKind::Tcp) => {
                Some(GrblHalNetworkAdapter::new().descriptor())
            }
            _ => None,
        };
        let experimental_mode = choice.requires_experimental_mode
            || adapter_descriptor.as_ref().is_some_and(|descriptor| {
                descriptor.product_tier == ControllerProductTier::Experimental
            });
        let capabilities = match choice.driver {
            ControllerDriverId::FluidNc | ControllerDriverId::GrblHal
                if choice.requires_experimental_compatibility_handshake =>
            {
                DeviceCapabilities::experimental_grbl_compatible()
            }
            ControllerDriverId::FluidNc | ControllerDriverId::GrblHal => adapter_descriptor
                .as_ref()
                .map(|descriptor| descriptor.capabilities.clone())
                .unwrap_or_else(DeviceCapabilities::disabled),
            ControllerDriverId::Grbl if experimental_mode => {
                DeviceCapabilities::experimental_grbl_compatible()
            }
            ControllerDriverId::LaserPecker => DeviceCapabilities::experimental_named_grbl_family(),
            ControllerDriverId::Grbl => grbl_capabilities(),
            ControllerDriverId::Marlin
            | ControllerDriverId::Snapmaker
            | ControllerDriverId::Smoothieware
            | ControllerDriverId::Ruida
            | ControllerDriverId::Lihuiyu
            | ControllerDriverId::Unknown => DeviceCapabilities::disabled(),
        };
        let (product_tier, evidence_state) = if let Some(descriptor) = adapter_descriptor {
            (
                Some(descriptor.product_tier),
                Some(descriptor.evidence_state),
            )
        } else if experimental_mode {
            (
                Some(ControllerProductTier::Experimental),
                Some(ControllerEvidenceState::Emulated),
            )
        } else {
            (None, None)
        };
        Self {
            session,
            driver: choice.driver,
            selection: choice.selection.clone(),
            detected_identity: choice.detected_identity.clone(),
            controller_model,
            capabilities,
            product_tier,
            evidence_state,
            experimental_mode,
            transport_kind,
        }
    }

    pub const fn driver(&self) -> ControllerDriverId {
        self.driver
    }

    pub fn selection(&self) -> ExplicitControllerSelection {
        self.selection.clone()
    }

    pub fn detected_identity(&self) -> Option<PositiveControllerIdentity> {
        self.detected_identity.clone()
    }

    pub const fn controller_model(&self) -> ControllerModel {
        self.controller_model
    }

    pub fn capabilities(&self) -> DeviceCapabilities {
        self.capabilities.clone()
    }

    pub const fn product_tier(&self) -> Option<ControllerProductTier> {
        self.product_tier
    }

    pub const fn evidence_state(&self) -> Option<ControllerEvidenceState> {
        self.evidence_state
    }

    pub const fn experimental_mode(&self) -> bool {
        self.experimental_mode
    }

    pub const fn transport_kind(&self) -> TransportKind {
        self.transport_kind
    }
}

/// Runtime metadata and coarse machine state for a Smoothieware session.
pub struct SmoothiewareRuntimeSession {
    session: SmoothiewareSerialSession,
    selection: ExplicitControllerSelection,
    detected_identity: Option<PositiveControllerIdentity>,
    capabilities: DeviceCapabilities,
    product_tier: ControllerProductTier,
    evidence_state: ControllerEvidenceState,
    machine_status: MachineStatus,
}

impl SmoothiewareRuntimeSession {
    pub fn from_choice(
        session: SmoothiewareSerialSession,
        choice: &ResolvedControllerChoice,
    ) -> Self {
        let descriptor = SmoothiewareSerialAdapter::new().descriptor();
        Self {
            session,
            selection: choice.selection.clone(),
            detected_identity: choice.detected_identity.clone(),
            capabilities: descriptor.capabilities,
            product_tier: descriptor.product_tier,
            evidence_state: descriptor.evidence_state,
            machine_status: MachineStatus::default(),
        }
    }

    pub const fn driver(&self) -> ControllerDriverId {
        ControllerDriverId::Smoothieware
    }

    pub const fn controller_model(&self) -> ControllerModel {
        ControllerModel::Smoothieware
    }

    pub fn capabilities(&self) -> DeviceCapabilities {
        self.capabilities.clone()
    }

    pub const fn product_tier(&self) -> ControllerProductTier {
        self.product_tier
    }

    pub const fn evidence_state(&self) -> ControllerEvidenceState {
        self.evidence_state
    }

    pub fn selection(&self) -> ExplicitControllerSelection {
        self.selection.clone()
    }

    pub fn detected_identity(&self) -> Option<PositiveControllerIdentity> {
        self.detected_identity.clone()
    }

    pub fn session_state(&self) -> SessionState {
        match self.session.state() {
            SmoothiewareSessionState::Disconnected => SessionState::Disconnected,
            SmoothiewareSessionState::Validating => SessionState::Validating,
            SmoothiewareSessionState::Ready => SessionState::Ready,
            SmoothiewareSessionState::Running => SessionState::Running,
            SmoothiewareSessionState::RecoveryRequired | SmoothiewareSessionState::Error => {
                SessionState::Error
            }
        }
    }

    pub fn controller_info(&self) -> HashMap<String, String> {
        let identity = self.session.identity();
        let mut info = HashMap::new();
        if let Some(value) = identity.firmware_identity {
            info.insert("firmware_identity".to_string(), value);
        }
        if let Some(value) = identity.firmware_version {
            info.insert("firmware_version".to_string(), value);
        }
        if let Some(value) = identity.protocol_version {
            info.insert("protocol_version".to_string(), value);
        }
        if let Some(value) = identity.grbl_mode {
            info.insert("grbl_mode".to_string(), value.to_string());
        }
        if let Some(value) = identity.laser_module_enabled {
            info.insert("laser_module_enabled".to_string(), value.to_string());
        }
        if let Some(value) = identity.laser_maximum_s_value {
            info.insert("laser_maximum_s_value".to_string(), value.to_string());
        }
        if let Some(value) = identity.laser_proportional_power {
            info.insert("laser_proportional_power".to_string(), value.to_string());
        }
        info
    }

    pub fn laser_maximum_s_value(&self) -> Option<f64> {
        self.session.identity().laser_maximum_s_value
    }

    pub fn port_name(&self) -> &str {
        self.session.port_name()
    }

    pub fn machine_status(&self) -> &MachineStatus {
        &self.machine_status
    }

    pub fn start_job(&mut self, commands: Vec<String>) -> Result<(), String> {
        self.session
            .start_job(commands)
            .map_err(|error| error.to_string())?;
        self.machine_status.run_state = MachineRunState::Run;
        Ok(())
    }

    pub fn tick_job(&mut self) -> Result<(), String> {
        self.session
            .tick(Instant::now())
            .map_err(|error| error.to_string())?;
        if self.session.state() == SmoothiewareSessionState::Ready {
            self.machine_status.run_state = MachineRunState::Idle;
        }
        Ok(())
    }

    pub fn cancel_job(&mut self) -> Result<(), String> {
        let result = self.session.cancel_job().map_err(|error| error.to_string());
        self.machine_status.run_state = MachineRunState::Alarm;
        result
    }

    pub fn emergency_shutdown(&mut self) -> Result<(), String> {
        let result = self
            .session
            .emergency_shutdown()
            .map_err(|error| error.to_string());
        self.machine_status.run_state = MachineRunState::Alarm;
        result
    }

    pub fn job_progress(&self) -> Option<beambench_gcode::AckFlowProgress> {
        self.session.job_progress()
    }

    pub fn disconnect(&mut self) -> Result<(), String> {
        self.session.disconnect().map_err(|error| error.to_string())
    }
}

/// Service-facing progress wrapper for a Smoothieware job owned by its session.
pub struct SmoothiewareRuntimeJob {
    total_lines: usize,
    started_at: Instant,
    state: JobState,
    sent_lines: usize,
    acknowledged_lines: usize,
    in_flight: bool,
    error_message: Option<String>,
}

impl SmoothiewareRuntimeJob {
    pub fn start(
        commands: Vec<String>,
        session: &mut SmoothiewareRuntimeSession,
    ) -> Result<Self, String> {
        let total_lines = commands.len();
        session.start_job(commands)?;
        Ok(Self {
            total_lines,
            started_at: Instant::now(),
            state: JobState::Running,
            sent_lines: 0,
            acknowledged_lines: 0,
            in_flight: false,
            error_message: None,
        })
    }

    pub fn tick(&mut self, session: &mut SmoothiewareRuntimeSession) -> JobProgress {
        if self.state != JobState::Running {
            return self.progress();
        }
        if let Err(error) = session.tick_job() {
            self.state = JobState::Failed;
            self.error_message = Some(error);
        } else if session.session_state() == SessionState::Ready {
            self.state = JobState::Completed;
        }
        self.capture_flow(session);
        self.progress()
    }

    pub fn cancel(
        &mut self,
        session: &mut SmoothiewareRuntimeSession,
    ) -> Result<JobProgress, String> {
        session.cancel_job()?;
        self.state = JobState::Cancelled;
        self.capture_flow(session);
        Ok(self.progress())
    }

    fn capture_flow(&mut self, session: &SmoothiewareRuntimeSession) {
        if let Some(flow) = session.job_progress() {
            self.sent_lines = flow.sent_lines;
            self.acknowledged_lines = flow.acknowledged_lines;
            self.in_flight = flow.in_flight_index.is_some();
        }
    }

    pub fn progress(&self) -> JobProgress {
        let elapsed_secs = self.started_at.elapsed().as_secs_f64();
        let estimated_remaining_secs = if self.acknowledged_lines > 0
            && self.acknowledged_lines < self.total_lines
            && self.state == JobState::Running
        {
            elapsed_secs * (self.total_lines - self.acknowledged_lines) as f64
                / self.acknowledged_lines as f64
        } else {
            0.0
        };
        JobProgress {
            state: self.state,
            total_lines: self.total_lines,
            queued_lines: self.total_lines,
            sent_lines: self.sent_lines,
            acknowledged_lines: self.acknowledged_lines,
            elapsed_secs,
            estimated_remaining_secs,
            buffer_fill_bytes: usize::from(self.in_flight),
            error_message: self.error_message.clone(),
            buckets: Vec::new(),
        }
    }
}

impl From<GrblSession> for GrblRuntimeSession {
    fn from(session: GrblSession) -> Self {
        Self::legacy(session)
    }
}

impl Deref for GrblRuntimeSession {
    type Target = GrblSession;

    fn deref(&self) -> &Self::Target {
        &self.session
    }
}

impl DerefMut for GrblRuntimeSession {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.session
    }
}

/// Runtime metadata and coarse machine state for a standard-Marlin session.
pub struct MarlinRuntimeSession {
    session: MarlinSerialSession,
    selection: ExplicitControllerSelection,
    detected_identity: Option<PositiveControllerIdentity>,
    driver: ControllerDriverId,
    controller_model: ControllerModel,
    capabilities: DeviceCapabilities,
    product_tier: ControllerProductTier,
    evidence_state: ControllerEvidenceState,
    machine_status: MachineStatus,
}

impl MarlinRuntimeSession {
    pub fn from_choice(session: MarlinSerialSession, choice: &ResolvedControllerChoice) -> Self {
        let descriptor = match choice.driver {
            ControllerDriverId::Snapmaker => SnapmakerSerialAdapter::new().descriptor(),
            _ => MarlinSerialAdapter::new().descriptor(),
        };
        Self {
            session,
            selection: choice.selection.clone(),
            detected_identity: choice.detected_identity.clone(),
            driver: descriptor.driver,
            controller_model: descriptor.controller_model,
            capabilities: descriptor.capabilities,
            product_tier: descriptor.product_tier,
            evidence_state: descriptor.evidence_state,
            machine_status: MachineStatus::default(),
        }
    }

    pub const fn driver(&self) -> ControllerDriverId {
        self.driver
    }

    pub const fn controller_model(&self) -> ControllerModel {
        self.controller_model
    }

    pub fn capabilities(&self) -> DeviceCapabilities {
        self.capabilities.clone()
    }

    pub const fn product_tier(&self) -> ControllerProductTier {
        self.product_tier
    }

    pub const fn evidence_state(&self) -> ControllerEvidenceState {
        self.evidence_state
    }

    pub fn selection(&self) -> ExplicitControllerSelection {
        self.selection.clone()
    }

    pub fn detected_identity(&self) -> Option<PositiveControllerIdentity> {
        self.detected_identity.clone()
    }

    pub fn session_state(&self) -> SessionState {
        match self.session.state() {
            MarlinSessionState::Disconnected => SessionState::Disconnected,
            MarlinSessionState::Validating => SessionState::Validating,
            MarlinSessionState::Ready => SessionState::Ready,
            MarlinSessionState::Running => SessionState::Running,
            MarlinSessionState::RecoveryRequired | MarlinSessionState::Error => SessionState::Error,
        }
    }

    pub fn controller_info(&self) -> HashMap<String, String> {
        let identity = self.session.identity();
        let mut info = HashMap::new();
        if let Some(value) = identity.firmware_identity {
            info.insert("firmware_identity".to_string(), value);
        }
        if let Some(value) = identity.firmware_version {
            info.insert("firmware_version".to_string(), value);
        }
        if let Some(value) = identity.machine_type {
            info.insert("machine_type".to_string(), value);
        }
        if let Some(value) = identity.protocol_version {
            info.insert("protocol_version".to_string(), value);
        }
        for (name, enabled) in identity.capabilities {
            info.insert(format!("capability.{name}"), enabled.to_string());
        }
        info
    }

    pub fn port_name(&self) -> &str {
        self.session.port_name()
    }

    pub fn machine_status(&self) -> &MachineStatus {
        &self.machine_status
    }

    pub fn start_job(&mut self, commands: Vec<String>) -> Result<(), String> {
        self.session
            .start_job(commands)
            .map_err(|error| error.to_string())?;
        self.machine_status.run_state = MachineRunState::Run;
        Ok(())
    }

    pub fn tick_job(&mut self) -> Result<(), String> {
        self.session
            .tick(Instant::now())
            .map_err(|error| error.to_string())?;
        if self.session.state() == MarlinSessionState::Ready {
            self.machine_status.run_state = MachineRunState::Idle;
        }
        Ok(())
    }

    pub fn cancel_job(&mut self) -> Result<(), String> {
        let result = self.session.cancel_job().map_err(|error| error.to_string());
        self.machine_status.run_state = MachineRunState::Alarm;
        result
    }

    pub fn emergency_shutdown(&mut self) -> Result<(), String> {
        let result = self
            .session
            .emergency_shutdown()
            .map_err(|error| error.to_string());
        self.machine_status.run_state = MachineRunState::Alarm;
        result
    }

    pub fn job_progress(&self) -> Option<beambench_gcode::AckFlowProgress> {
        self.session.job_progress()
    }

    pub fn disconnect(&mut self) -> Result<(), String> {
        self.session.disconnect().map_err(|error| error.to_string())
    }
}

/// Service-facing progress wrapper for a Marlin job owned by its session.
pub struct MarlinRuntimeJob {
    total_lines: usize,
    started_at: Instant,
    state: JobState,
    sent_lines: usize,
    acknowledged_lines: usize,
    in_flight: bool,
    error_message: Option<String>,
}

impl MarlinRuntimeJob {
    pub fn start(
        commands: Vec<String>,
        session: &mut MarlinRuntimeSession,
    ) -> Result<Self, String> {
        let total_lines = commands.len();
        session.start_job(commands)?;
        Ok(Self {
            total_lines,
            started_at: Instant::now(),
            state: JobState::Running,
            sent_lines: 0,
            acknowledged_lines: 0,
            in_flight: false,
            error_message: None,
        })
    }

    pub fn tick(&mut self, session: &mut MarlinRuntimeSession) -> JobProgress {
        if self.state != JobState::Running {
            return self.progress();
        }
        if let Err(error) = session.tick_job() {
            self.state = JobState::Failed;
            self.error_message = Some(error);
        } else if session.session_state() == SessionState::Ready {
            self.state = JobState::Completed;
        }
        self.capture_flow(session);
        self.progress()
    }

    pub fn cancel(&mut self, session: &mut MarlinRuntimeSession) -> Result<JobProgress, String> {
        session.cancel_job()?;
        self.state = JobState::Cancelled;
        self.capture_flow(session);
        Ok(self.progress())
    }

    fn capture_flow(&mut self, session: &MarlinRuntimeSession) {
        if let Some(flow) = session.job_progress() {
            self.sent_lines = flow.sent_lines;
            self.acknowledged_lines = flow.acknowledged_lines;
            self.in_flight = flow.in_flight_index.is_some();
        }
    }

    pub fn progress(&self) -> JobProgress {
        let elapsed_secs = self.started_at.elapsed().as_secs_f64();
        let estimated_remaining_secs = if self.acknowledged_lines > 0
            && self.acknowledged_lines < self.total_lines
            && self.state == JobState::Running
        {
            elapsed_secs * (self.total_lines - self.acknowledged_lines) as f64
                / self.acknowledged_lines as f64
        } else {
            0.0
        };
        JobProgress {
            state: self.state,
            total_lines: self.total_lines,
            queued_lines: self.total_lines,
            sent_lines: self.sent_lines,
            acknowledged_lines: self.acknowledged_lines,
            elapsed_secs,
            estimated_remaining_secs,
            buffer_fill_bytes: usize::from(self.in_flight),
            error_message: self.error_message.clone(),
            buckets: Vec::new(),
        }
    }
}

pub enum PendingControllerSession {
    Grbl {
        probe: GrblFamilyIdentityProbeResult,
        session: Box<GrblSession>,
    },
    Marlin {
        probe: MarlinIdentityProbeResult,
        session: MarlinSerialSession,
    },
    Smoothieware {
        probe: SmoothiewareIdentityProbeResult,
        session: SmoothiewareSerialSession,
    },
}

impl PendingControllerSession {
    pub fn disconnect(&mut self) {
        match self {
            Self::Grbl { session, .. } => {
                let _ = session.disconnect();
            }
            Self::Marlin { session, .. } => {
                let _ = session.disconnect();
            }
            Self::Smoothieware { session, .. } => {
                let _ = session.disconnect();
            }
        }
    }

    pub fn detected_identity(&self) -> Option<PositiveControllerIdentity> {
        match self {
            Self::Grbl { probe, .. } => probe.identity.positive_identity(),
            Self::Marlin { probe, .. } => probe.identity.positive_identity(),
            Self::Smoothieware { probe, .. } => probe.identity.positive_identity(),
        }
    }
}

/// Open controller session awaiting a backend-owned desktop choice decision.
pub struct PendingControllerConnection {
    pub attempt_id: String,
    pub endpoint: ControllerConnectionEndpoint,
    pub device_identity: DeviceIdentity,
    pub selection: ControllerSelection,
    pub session: PendingControllerSession,
    pub created_at: Instant,
}

pub enum MachineSessionHandle {
    Grbl(GrblRuntimeSession),
    Marlin(MarlinRuntimeSession),
    Smoothieware(SmoothiewareRuntimeSession),
    Ruida(RuidaRuntimeSession),
    Lihuiyu(LihuiyuRuntimeSession),
    Dsp(DspSession),
    Galvo(GalvoSession),
}

impl MachineSessionHandle {
    pub fn controller_family(&self) -> ControllerFamily {
        match self {
            Self::Grbl(_) | Self::Marlin(_) | Self::Smoothieware(_) => ControllerFamily::Gcode,
            Self::Ruida(_) | Self::Lihuiyu(_) | Self::Dsp(_) => ControllerFamily::Dsp,
            Self::Galvo(_) => ControllerFamily::Galvo,
        }
    }

    pub fn controller_model(&self) -> ControllerModel {
        match self {
            Self::Grbl(session) => session.controller_model(),
            Self::Marlin(session) => session.controller_model(),
            Self::Smoothieware(session) => session.controller_model(),
            Self::Ruida(session) => session.controller_model(),
            Self::Lihuiyu(session) => session.controller_model(),
            Self::Dsp(session) => session.model,
            Self::Galvo(session) => session.model,
        }
    }

    pub fn transport_kind(&self) -> TransportKind {
        match self {
            Self::Grbl(session) => session.transport_kind(),
            Self::Marlin(_) | Self::Smoothieware(_) => TransportKind::Serial,
            Self::Ruida(_) => TransportKind::Udp,
            Self::Lihuiyu(_) => TransportKind::UsbPacket,
            Self::Dsp(_) => TransportKind::Tcp,
            Self::Galvo(_) => TransportKind::UsbPacket,
        }
    }

    pub fn capabilities(&self) -> DeviceCapabilities {
        match self {
            Self::Grbl(session) => session.capabilities(),
            Self::Marlin(session) => session.capabilities(),
            Self::Smoothieware(session) => session.capabilities(),
            Self::Ruida(session) => session.capabilities(),
            Self::Lihuiyu(session) => session.capabilities(),
            Self::Dsp(_) => dsp_capabilities(),
            Self::Galvo(_) => galvo_capabilities(),
        }
    }

    pub fn controller_driver(&self) -> Option<ControllerDriverId> {
        match self {
            Self::Grbl(session) => Some(session.driver()),
            Self::Marlin(session) => Some(session.driver()),
            Self::Smoothieware(session) => Some(session.driver()),
            Self::Ruida(session) => Some(session.driver()),
            Self::Lihuiyu(session) => Some(session.driver()),
            Self::Dsp(_) | Self::Galvo(_) => None,
        }
    }

    pub fn experimental_mode(&self) -> bool {
        match self {
            Self::Grbl(session) => session.experimental_mode(),
            Self::Marlin(_) => true,
            Self::Smoothieware(_) => true,
            Self::Ruida(_) | Self::Lihuiyu(_) => true,
            Self::Dsp(_) | Self::Galvo(_) => false,
        }
    }

    pub fn controller_product_tier(&self) -> Option<ControllerProductTier> {
        match self {
            Self::Grbl(session) => session.product_tier(),
            Self::Marlin(session) => Some(session.product_tier()),
            Self::Smoothieware(session) => Some(session.product_tier()),
            Self::Ruida(session) => Some(session.product_tier()),
            Self::Lihuiyu(session) => Some(session.product_tier()),
            Self::Dsp(_) | Self::Galvo(_) => Some(ControllerProductTier::Unavailable),
        }
    }

    pub fn controller_evidence_state(&self) -> Option<ControllerEvidenceState> {
        match self {
            Self::Grbl(session) => session.evidence_state(),
            Self::Marlin(session) => Some(session.evidence_state()),
            Self::Smoothieware(session) => Some(session.evidence_state()),
            Self::Ruida(session) => Some(session.evidence_state()),
            Self::Lihuiyu(session) => Some(session.evidence_state()),
            Self::Dsp(_) | Self::Galvo(_) => Some(ControllerEvidenceState::Emulated),
        }
    }

    pub fn controller_selection(&self) -> Option<ExplicitControllerSelection> {
        match self {
            Self::Grbl(session) => Some(session.selection()),
            Self::Marlin(session) => Some(session.selection()),
            Self::Smoothieware(session) => Some(session.selection()),
            Self::Ruida(session) => Some(ExplicitControllerSelection::KnownDriver {
                driver: session.driver(),
            }),
            Self::Lihuiyu(session) => Some(ExplicitControllerSelection::KnownDriver {
                driver: session.driver(),
            }),
            Self::Dsp(_) | Self::Galvo(_) => None,
        }
    }

    pub fn detected_controller_identity(&self) -> Option<PositiveControllerIdentity> {
        match self {
            Self::Grbl(session) => session.detected_identity(),
            Self::Marlin(session) => session.detected_identity(),
            Self::Smoothieware(session) => session.detected_identity(),
            Self::Ruida(session) => Some(session.detected_identity()),
            Self::Lihuiyu(session) => Some(session.detected_identity()),
            Self::Dsp(_) | Self::Galvo(_) => None,
        }
    }

    pub fn session_state(&self) -> SessionState {
        match self {
            Self::Grbl(session) => session.session_state(),
            Self::Marlin(session) => session.session_state(),
            Self::Smoothieware(session) => session.session_state(),
            Self::Ruida(session) => session.session_state(),
            Self::Lihuiyu(session) => session.session_state(),
            Self::Dsp(session) => session.session_state,
            Self::Galvo(session) => session.session_state,
        }
    }

    pub fn machine_status(&self) -> MachineStatus {
        match self {
            Self::Grbl(session) => session.last_status().clone(),
            Self::Marlin(session) => session.machine_status().clone(),
            Self::Smoothieware(session) => session.machine_status().clone(),
            Self::Ruida(session) => session.machine_status().clone(),
            Self::Lihuiyu(session) => session.machine_status().clone(),
            Self::Dsp(session) => session.status(),
            Self::Galvo(session) => session.status(),
        }
    }

    pub fn controller_info(&self) -> Option<HashMap<String, String>> {
        match self {
            Self::Grbl(session) => Some(session.controller_info().clone()),
            Self::Marlin(session) => Some(session.controller_info()),
            Self::Smoothieware(session) => Some(session.controller_info()),
            Self::Ruida(session) => Some(session.controller_info()),
            Self::Lihuiyu(session) => Some(session.controller_info()),
            Self::Dsp(_) | Self::Galvo(_) => None,
        }
    }

    pub fn port_name(&self) -> Option<String> {
        match self {
            Self::Grbl(session) => Some(session.port_name().to_owned()),
            Self::Marlin(session) => Some(session.port_name().to_owned()),
            Self::Smoothieware(session) => Some(session.port_name().to_owned()),
            Self::Ruida(_) | Self::Lihuiyu(_) => None,
            Self::Dsp(_) | Self::Galvo(_) => None,
        }
    }

    pub fn controller_settings(&self) -> Option<HashMap<String, String>> {
        match self {
            Self::Grbl(session) => Some(session.settings().as_string_map()),
            Self::Marlin(_) => None,
            Self::Smoothieware(_) => None,
            Self::Ruida(_) | Self::Lihuiyu(_) => None,
            Self::Dsp(_) | Self::Galvo(_) => None,
        }
    }

    pub fn poll(&mut self) {
        match self {
            Self::Grbl(session) => {
                let _ = session.poll();
            }
            Self::Ruida(session) => {
                let _ = session.poll();
            }
            Self::Lihuiyu(session) => {
                let _ = session.poll();
            }
            _ => {}
        }
    }

    /// Send a `?` status query to the controller so the next `poll()` picks up
    /// the real-time machine state.  No-op for non-GRBL sessions.
    pub fn query_status(&mut self) {
        if let Self::Grbl(session) = self {
            let _ = session.poll_status();
        }
    }

    pub fn console_entries(&self, limit: usize) -> Vec<ConsoleEntry> {
        match self {
            Self::Grbl(session) => session.get_console_log(limit),
            Self::Marlin(_) => Vec::new(),
            Self::Smoothieware(_) => Vec::new(),
            Self::Ruida(_) | Self::Lihuiyu(_) => Vec::new(),
            Self::Dsp(_) | Self::Galvo(_) => Vec::new(),
        }
    }

    pub fn clear_console_entries(&mut self) {
        if let Self::Grbl(session) = self {
            session.clear_console_log();
        }
    }

    pub fn disconnect(&mut self) -> Result<(), String> {
        match self {
            Self::Grbl(session) => session.disconnect().map_err(|e| e.to_string()),
            Self::Marlin(session) => session.disconnect(),
            Self::Smoothieware(session) => session.disconnect(),
            Self::Ruida(session) => session.disconnect(),
            Self::Lihuiyu(session) => session.disconnect(),
            Self::Dsp(session) => {
                session.disconnect();
                Ok(())
            }
            Self::Galvo(session) => {
                session.disconnect();
                Ok(())
            }
        }
    }

    pub fn supports_home(&self) -> bool {
        self.capabilities().can_home
    }

    pub fn supports_jog(&self) -> bool {
        self.capabilities().can_jog
    }

    pub fn supports_unlock(&self) -> bool {
        self.capabilities().can_unlock
    }

    pub fn supports_origin(&self) -> bool {
        self.capabilities().can_set_origin
    }

    pub fn set_running(&mut self) {
        match self {
            Self::Grbl(_) => {}
            Self::Marlin(session) => session.machine_status.run_state = MachineRunState::Run,
            Self::Smoothieware(session) => session.machine_status.run_state = MachineRunState::Run,
            Self::Ruida(_) | Self::Lihuiyu(_) => {}
            Self::Dsp(session) => session.machine_status.run_state = MachineRunState::Run,
            Self::Galvo(session) => session.machine_status.run_state = MachineRunState::Run,
        }
    }
}

pub enum ActiveJobHandle {
    Grbl(JobController),
    Marlin(MarlinRuntimeJob),
    Smoothieware(SmoothiewareRuntimeJob),
    Ruida(RuidaRuntimeJob),
    Lihuiyu(LihuiyuRuntimeJob),
    Dsp(DspJob),
    Galvo(GalvoJob),
}

impl ActiveJobHandle {
    pub fn progress(&self) -> JobProgress {
        match self {
            Self::Grbl(job) => job.progress(),
            Self::Marlin(job) => job.progress(),
            Self::Smoothieware(job) => job.progress(),
            Self::Ruida(job) => job.progress(),
            Self::Lihuiyu(job) => job.progress(),
            Self::Dsp(job) => job.progress(),
            Self::Galvo(job) => job.progress(),
        }
    }

    pub fn cancel(&mut self, session: &mut MachineSessionHandle) -> Result<JobProgress, String> {
        match (self, session) {
            (Self::Grbl(job), MachineSessionHandle::Grbl(session)) => {
                job.cancel(session).map_err(|e| e.to_string())?;
                Ok(job.progress())
            }
            (Self::Marlin(job), MachineSessionHandle::Marlin(session)) => job.cancel(session),
            (Self::Smoothieware(job), MachineSessionHandle::Smoothieware(session)) => {
                job.cancel(session)
            }
            (Self::Ruida(job), MachineSessionHandle::Ruida(session)) => job.cancel(session),
            (Self::Lihuiyu(job), MachineSessionHandle::Lihuiyu(session)) => job.cancel(session),
            (Self::Dsp(job), MachineSessionHandle::Dsp(session)) => {
                session.machine_status.run_state = MachineRunState::Idle;
                Ok(job.cancel())
            }
            (Self::Galvo(job), MachineSessionHandle::Galvo(session)) => {
                session.machine_status.run_state = MachineRunState::Idle;
                Ok(job.cancel())
            }
            _ => Err("Active job does not match connected controller".to_string()),
        }
    }

    pub fn tick(
        &mut self,
        session: Option<&mut MachineSessionHandle>,
    ) -> Result<JobProgress, String> {
        match (self, session) {
            (Self::Grbl(job), Some(MachineSessionHandle::Grbl(session))) => {
                if let Err(err) = job.tick(session) {
                    let progress = job.progress();
                    if matches!(progress.state, beambench_common::machine::JobState::Failed) {
                        return Ok(progress);
                    }
                    return Err(err.to_string());
                }
                Ok(job.progress())
            }
            (Self::Marlin(job), Some(MachineSessionHandle::Marlin(session))) => {
                Ok(job.tick(session))
            }
            (Self::Smoothieware(job), Some(MachineSessionHandle::Smoothieware(session))) => {
                Ok(job.tick(session))
            }
            (Self::Ruida(job), Some(MachineSessionHandle::Ruida(session))) => Ok(job.tick(session)),
            (Self::Lihuiyu(job), Some(MachineSessionHandle::Lihuiyu(session))) => {
                Ok(job.tick(session))
            }
            (Self::Dsp(job), Some(MachineSessionHandle::Dsp(session))) => {
                let progress = job.tick();
                if matches!(
                    progress.state,
                    beambench_common::machine::JobState::Completed
                ) {
                    session.machine_status.run_state = MachineRunState::Idle;
                }
                Ok(progress)
            }
            (Self::Galvo(job), Some(MachineSessionHandle::Galvo(session))) => {
                let progress = job.tick();
                if matches!(
                    progress.state,
                    beambench_common::machine::JobState::Completed
                ) {
                    session.machine_status.run_state = MachineRunState::Idle;
                }
                Ok(progress)
            }
            (Self::Dsp(job), None) => Ok(job.progress()),
            (Self::Galvo(job), None) => Ok(job.progress()),
            (Self::Grbl(job), None) => Ok(job.progress()),
            (Self::Marlin(job), None) => Ok(job.progress()),
            (Self::Smoothieware(job), None) => Ok(job.progress()),
            (Self::Ruida(job), None) => Ok(job.progress()),
            (Self::Lihuiyu(job), None) => Ok(job.progress()),
            _ => Err("Active job does not match connected controller".to_string()),
        }
    }

    pub fn is_complete(&self) -> bool {
        matches!(
            self.progress().state,
            beambench_common::machine::JobState::Completed
                | beambench_common::machine::JobState::Failed
                | beambench_common::machine::JobState::Cancelled
        )
    }

    pub fn supports_pause_resume(&self) -> bool {
        matches!(self, Self::Grbl(_) | Self::Ruida(_) | Self::Lihuiyu(_))
    }

    pub fn console_entries(&self, limit: usize) -> Vec<ConsoleEntry> {
        match self {
            Self::Grbl(job) => job.get_console_entries(limit),
            Self::Marlin(_) => Vec::new(),
            Self::Smoothieware(_) => Vec::new(),
            Self::Ruida(_) | Self::Lihuiyu(_) => Vec::new(),
            Self::Dsp(_) | Self::Galvo(_) => Vec::new(),
        }
    }

    pub fn pause(&mut self, session: &mut MachineSessionHandle) -> Result<JobProgress, String> {
        match (self, session) {
            (Self::Grbl(job), MachineSessionHandle::Grbl(session)) => {
                job.pause(session).map_err(|e| e.to_string())?;
                Ok(job.progress())
            }
            (Self::Ruida(job), MachineSessionHandle::Ruida(session)) => job.pause(session),
            (Self::Lihuiyu(job), MachineSessionHandle::Lihuiyu(session)) => job.pause(session),
            _ => Err("Pause is not supported by the connected controller".to_string()),
        }
    }

    pub fn resume(&mut self, session: &mut MachineSessionHandle) -> Result<JobProgress, String> {
        match (self, session) {
            (Self::Grbl(job), MachineSessionHandle::Grbl(session)) => {
                job.resume(session).map_err(|e| e.to_string())?;
                Ok(job.progress())
            }
            (Self::Ruida(job), MachineSessionHandle::Ruida(session)) => job.resume(session),
            (Self::Lihuiyu(job), MachineSessionHandle::Lihuiyu(session)) => job.resume(session),
            _ => Err("Resume is not supported by the connected controller".to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use beambench_common::geometry::{Bounds, Point2D};
    use beambench_common::machine::JobState;
    use beambench_grbl::GcodeConfig;
    use beambench_planner::{ExecutionPlan, PlanSegment};
    use beambench_serial::{SerialError, SerialTransport};
    use chrono::Utc;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};
    use uuid::Uuid;

    #[test]
    fn runtime_capabilities_preserve_grbl_and_disable_placeholders() {
        assert_eq!(grbl_capabilities(), DeviceCapabilities::legacy_grbl());
        assert_eq!(dsp_capabilities(), DeviceCapabilities::default());
        assert_eq!(galvo_capabilities(), DeviceCapabilities::default());
    }

    struct SharedMockTransport {
        open: bool,
        rx: Arc<Mutex<VecDeque<String>>>,
    }

    impl SharedMockTransport {
        fn new(rx: Arc<Mutex<VecDeque<String>>>) -> Self {
            Self { open: false, rx }
        }
    }

    impl SerialTransport for SharedMockTransport {
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
            Ok(data.len())
        }

        fn write_line(&mut self, _line: &str) -> Result<(), SerialError> {
            Ok(())
        }

        fn read_available(&mut self) -> Result<Vec<u8>, SerialError> {
            Ok(Vec::new())
        }

        fn read_line(&mut self) -> Result<Option<String>, SerialError> {
            Ok(self.rx.lock().unwrap().pop_front())
        }

        fn flush(&mut self) -> Result<(), SerialError> {
            Ok(())
        }

        fn port_name(&self) -> &str {
            "shared-mock"
        }
    }

    #[test]
    fn experimental_grbl_runtime_preserves_detected_model_and_capability_overlay() {
        use beambench_common::controller_choice::{
            ControllerChoiceSource, ControllerOverrideScope, ExplicitControllerSelection,
            PositiveControllerIdentity,
        };

        let transport = SharedMockTransport::new(Arc::new(Mutex::new(VecDeque::new())));
        let session = GrblSession::new(Box::new(transport));
        let choice = ResolvedControllerChoice {
            selection: ExplicitControllerSelection::KnownDriver {
                driver: ControllerDriverId::Grbl,
            },
            driver: ControllerDriverId::Grbl,
            source: ControllerChoiceSource::UserExperimentalOverride,
            detected_identity: Some(PositiveControllerIdentity {
                family: ControllerFamily::Gcode,
                model: ControllerModel::FluidNc,
                firmware_identity: Some("FluidNC".to_string()),
                firmware_version: Some("4.0.3".to_string()),
                evidence: vec!["Controller information response".to_string()],
            }),
            requires_experimental_mode: true,
            mismatch: true,
            override_scope: Some(ControllerOverrideScope::SessionOnly),
            requires_experimental_compatibility_handshake: true,
        };

        let handle = MachineSessionHandle::Grbl(GrblRuntimeSession::from_choice(session, &choice));
        assert_eq!(handle.controller_model(), ControllerModel::FluidNc);
        assert_eq!(handle.controller_driver(), Some(ControllerDriverId::Grbl));
        assert_eq!(
            handle.controller_selection(),
            Some(ExplicitControllerSelection::KnownDriver {
                driver: ControllerDriverId::Grbl,
            })
        );
        assert_eq!(
            handle
                .detected_controller_identity()
                .map(|identity| identity.model),
            Some(ControllerModel::FluidNc)
        );
        assert!(handle.experimental_mode());
        assert_eq!(
            handle.controller_product_tier(),
            Some(ControllerProductTier::Experimental)
        );
        assert_eq!(
            handle.controller_evidence_state(),
            Some(ControllerEvidenceState::Emulated)
        );
        assert!(!handle.capabilities().can_home);
        assert!(handle.capabilities().can_run_job);
    }

    #[test]
    fn fluidnc_runtime_uses_named_adapter_metadata_and_shared_capabilities() {
        use beambench_common::controller_choice::{
            ControllerChoiceSource, ExplicitControllerSelection, PositiveControllerIdentity,
        };

        let transport = SharedMockTransport::new(Arc::new(Mutex::new(VecDeque::new())));
        let session = GrblSession::new(Box::new(transport));
        let choice = ResolvedControllerChoice {
            selection: ExplicitControllerSelection::KnownDriver {
                driver: ControllerDriverId::FluidNc,
            },
            driver: ControllerDriverId::FluidNc,
            source: ControllerChoiceSource::AutoDetected,
            detected_identity: Some(PositiveControllerIdentity {
                family: ControllerFamily::Gcode,
                model: ControllerModel::FluidNc,
                firmware_identity: Some("FluidNC".to_string()),
                firmware_version: Some("4.0.3".to_string()),
                evidence: vec!["Controller information response".to_string()],
            }),
            requires_experimental_mode: true,
            mismatch: false,
            override_scope: None,
            requires_experimental_compatibility_handshake: false,
        };

        let handle = MachineSessionHandle::Grbl(GrblRuntimeSession::from_choice(session, &choice));
        assert_eq!(handle.controller_model(), ControllerModel::FluidNc);
        assert_eq!(
            handle.controller_driver(),
            Some(ControllerDriverId::FluidNc)
        );
        assert!(handle.experimental_mode());
        assert_eq!(
            handle.controller_product_tier(),
            Some(ControllerProductTier::Experimental)
        );
        assert_eq!(
            handle.controller_evidence_state(),
            Some(ControllerEvidenceState::Emulated)
        );
        assert!(handle.capabilities().can_home);
        assert!(handle.capabilities().can_jog);
        assert!(handle.capabilities().can_run_job);
    }

    #[test]
    fn grblhal_runtime_uses_named_adapter_metadata_and_shared_capabilities() {
        use beambench_common::controller_choice::{
            ControllerChoiceSource, ExplicitControllerSelection, PositiveControllerIdentity,
        };

        let transport = SharedMockTransport::new(Arc::new(Mutex::new(VecDeque::new())));
        let session = GrblSession::new(Box::new(transport));
        let choice = ResolvedControllerChoice {
            selection: ExplicitControllerSelection::KnownDriver {
                driver: ControllerDriverId::GrblHal,
            },
            driver: ControllerDriverId::GrblHal,
            source: ControllerChoiceSource::AutoDetected,
            detected_identity: Some(PositiveControllerIdentity {
                family: ControllerFamily::Gcode,
                model: ControllerModel::GrblHal,
                firmware_identity: Some("grblHAL".to_string()),
                firmware_version: Some("1.1f.20260712".to_string()),
                evidence: vec!["Firmware identity response".to_string()],
            }),
            requires_experimental_mode: true,
            mismatch: false,
            override_scope: None,
            requires_experimental_compatibility_handshake: false,
        };

        let handle = MachineSessionHandle::Grbl(GrblRuntimeSession::from_choice(session, &choice));
        assert_eq!(handle.controller_model(), ControllerModel::GrblHal);
        assert_eq!(
            handle.controller_driver(),
            Some(ControllerDriverId::GrblHal)
        );
        assert!(handle.experimental_mode());
        assert_eq!(
            handle.controller_product_tier(),
            Some(ControllerProductTier::Experimental)
        );
        assert_eq!(
            handle.controller_evidence_state(),
            Some(ControllerEvidenceState::Emulated)
        );
        assert!(handle.capabilities().can_home);
        assert!(handle.capabilities().can_jog);
        assert!(handle.capabilities().can_run_job);
    }

    fn make_plan() -> ExecutionPlan {
        ExecutionPlan {
            id: Uuid::new_v4(),
            project_id: Uuid::new_v4(),
            revision_hash: "test".to_string(),
            created_at: Utc::now(),
            bounds: Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(20.0, 0.0)),
            total_distance_mm: 20.0,
            estimated_duration_secs: 1.0,
            segments: vec![PlanSegment::Vector {
                polyline: vec![Point2D::new(0.0, 0.0), Point2D::new(20.0, 0.0)],
                closed: false,
                power_percent: 50.0,
                speed_mm_min: 1000.0,
                layer_id: "l1".to_string(),
                cut_entry_id: "entry-1".to_string(),
                perforation_enabled: false,
                perforation_on_ms: 0.0,
                perforation_off_ms: 0.0,
                source_object_id: None,
                source_subpath_index: None,
            }],
            layer_order: vec!["l1".to_string()],
            failed_entries: vec![],
            warnings: vec![],
        }
    }

    #[test]
    fn grbl_tick_returns_failed_progress_on_controller_error() {
        let rx = Arc::new(Mutex::new(VecDeque::from(["Grbl 1.1h".to_string()])));
        let transport = SharedMockTransport::new(Arc::clone(&rx));
        let mut session = GrblSession::new(Box::new(transport));
        session.connect().unwrap();
        session.poll().unwrap();
        session.mark_ready().unwrap();
        rx.lock().unwrap().push_back("error:2".to_string());

        let mut job = JobController::prepare(&make_plan(), &GcodeConfig::default()).unwrap();
        job.start(&mut session).unwrap();
        let mut active = ActiveJobHandle::Grbl(job);
        let mut handle = MachineSessionHandle::Grbl(session.into());

        let progress = active.tick(Some(&mut handle)).unwrap();

        assert_eq!(progress.state, JobState::Failed);
    }
}
