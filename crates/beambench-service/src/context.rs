use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use beambench_common::feedback::{
    DiagnosticConnectionEvent, DiagnosticPanic, DiagnosticTerminalJob,
};
use beambench_common::machine::{DiscoveryScanState, JobProgress};
use beambench_common::{
    CameraDeviceInfo, CameraFrameHandle, CameraOverlayRenderResult, CameraOverlayRuntimeState,
    ConsoleEntry,
};
use beambench_core::{
    AppSettings, ArtLibraryDocument, ArtLibraryItem, ArtLibraryItemKind, CutEntry, LayerId,
    MachineProfileId, MacroDefinition, MaterialPreset, Project,
};
use beambench_planner::{ExecutionPlan, OptimizationRuntime};
use chrono::{DateTime, Utc};
use image::GrayImage;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::agent::AgentSelectionSnapshot;
use crate::error::ServiceResult;
use crate::events::ServiceEventEnvelope;
use crate::history::{ProjectHistory, UndoState};
use crate::material_apply::{
    MaterialApplyResponse, MaterialApplyWarning, MaterialApplyWarningCode,
};
use crate::runtime::{ActiveJobHandle, MachineSessionHandle, PendingControllerConnection};

type SettingsApplier = dyn Fn(&AppSettings) -> Result<(), String> + Send + Sync;

/// Maximum number of entries in the log buffer.
const LOG_BUFFER_CAP: usize = 500;
/// Maximum number of entries in the active errors list.
const ACTIVE_ERRORS_CAP: usize = 100;
/// Maximum number of connection lifecycle events retained for diagnostics.
const CONNECTION_EVENTS_CAP: usize = 32;
/// Maximum number of entries in the console log.
const CONSOLE_LOG_CAP: usize = 2000;
/// Maximum number of decoded grayscale trace-preview sources to retain.
const TRACE_PREVIEW_SOURCE_CACHE_CAP: usize = 8;

#[derive(Debug, Clone)]
pub struct StoredPanicReport {
    pub report: DiagnosticPanic,
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveErrorEntry {
    pub ts: DateTime<Utc>,
    pub line: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TracePreviewSourceKey {
    pub asset_key: String,
    pub trace_alpha: bool,
}

#[derive(Default)]
pub struct TracePreviewSourceCache {
    order: VecDeque<TracePreviewSourceKey>,
    entries: HashMap<TracePreviewSourceKey, Arc<GrayImage>>,
}

pub struct PendingCameraCaptureRequest {
    pub request_id: String,
    pub tx: tokio::sync::oneshot::Sender<ServiceResult<CameraFrameHandle>>,
}

pub struct PendingCameraOverlayRenderRequest {
    pub request_id: String,
    pub view: beambench_common::CameraOverlayRenderView,
    pub temporary: bool,
    pub tx: tokio::sync::oneshot::Sender<ServiceResult<CameraOverlayRenderResult>>,
}

#[derive(Debug, Clone)]
pub struct LaserFireState {
    pub token: String,
    pub expires_at: Instant,
    pub max_expires_at: Instant,
    pub stop_requested: bool,
}

impl TracePreviewSourceCache {
    pub fn get(&mut self, key: &TracePreviewSourceKey) -> Option<Arc<GrayImage>> {
        let image = self.entries.get(key)?.clone();
        self.touch(key.clone());
        Some(image)
    }

    pub fn put(&mut self, key: TracePreviewSourceKey, image: Arc<GrayImage>) {
        self.entries.insert(key.clone(), image);
        self.touch(key);
    }

    pub fn remove_asset(&mut self, asset_key: &str) {
        self.entries.retain(|key, _| key.asset_key != asset_key);
        self.order.retain(|key| key.asset_key != asset_key);
    }

    fn touch(&mut self, key: TracePreviewSourceKey) {
        self.order.retain(|existing| existing != &key);
        self.order.push_back(key.clone());
        while self.order.len() > TRACE_PREVIEW_SOURCE_CACHE_CAP {
            if let Some(oldest) = self.order.pop_front() {
                self.entries.remove(&oldest);
            }
        }
    }
}

/// Framework-agnostic service context that owns all runtime state.
/// Shared by Tauri (via `Arc`), HTTP API, and CLI.
pub struct ServiceContext {
    pub project: Mutex<Option<Project>>,
    pub project_path: Mutex<Option<PathBuf>>,
    pub settings: Mutex<AppSettings>,
    pub plan_cache: Mutex<Option<ExecutionPlan>>,
    pub history: Mutex<ProjectHistory>,
    pub session: Mutex<Option<MachineSessionHandle>>,
    /// Serializes connect, decision, cancel, and disconnect operations so an
    /// open challenge cannot race a second connection into the same runtime.
    pub controller_connection_gate: Mutex<()>,
    /// Backend-owned open session retained while the desktop resolves a
    /// controller selection or mismatch challenge. Never persisted.
    pub pending_controller_connection: Mutex<Option<PendingControllerConnection>>,
    pub job: Mutex<Option<ActiveJobHandle>>,
    /// True while the backend-owned job ticker is active. Jobs must keep
    /// streaming even if no frontend panel or CLI command polls progress.
    pub job_tick_loop_running: AtomicBool,
    /// True while a backend-issued continuous jog is outstanding.
    pub active_jog: AtomicBool,
    /// True only after the current connection has established machine
    /// coordinates. G53 absolute machine-coordinate moves require this.
    pub machine_coordinates_valid: AtomicBool,
    /// Active manual fire session. Fire is backend-deadman protected and must
    /// be explicitly stopped or renewed before this state expires.
    pub active_laser_fire: Mutex<Option<LaserFireState>>,
    pub discovery_state: Mutex<DiscoveryScanState>,
    pub camera_devices_override: Mutex<Option<Vec<CameraDeviceInfo>>>,
    pub camera_frames: Mutex<HashMap<MachineProfileId, CameraFrameHandle>>,
    /// Frontend/CLI-visible camera overlay display state. This is committed
    /// session state only; frontend pointer drags keep local uncommitted state
    /// and commit on pointer-up.
    pub camera_overlay_runtime: Mutex<HashMap<MachineProfileId, CameraOverlayRuntimeState>>,
    /// Last visual artifacts produced by the agent-facing camera commands.
    pub camera_latest_capture_artifact: Mutex<Option<beambench_common::CameraArtifactInfo>>,
    pub camera_latest_render_artifact: Mutex<Option<beambench_common::CameraArtifactInfo>>,
    /// True while the Tauri frontend has registered the browser/canvas camera bridge.
    pub camera_agent_bridge_connected: AtomicBool,
    /// Single-flight reverse-RPC requests for browser capture and canvas render.
    pub pending_camera_capture_request: Mutex<Option<PendingCameraCaptureRequest>>,
    pub pending_camera_overlay_render_request: Mutex<Option<PendingCameraOverlayRenderRequest>>,
    /// Last emitted job progress snapshot to avoid noisy websocket churn.
    pub last_job_progress: Mutex<Option<JobProgress>>,
    /// Last terminal job evidence retained for feedback after live job/session
    /// state has been cleared.
    pub last_terminal_job: Mutex<Option<DiagnosticTerminalJob>>,
    /// Broadcast channel for pushing events to WebSocket clients.
    pub events: broadcast::Sender<String>,
    /// Monotonic event sequence for websocket consumers.
    pub event_seq: AtomicU64,
    /// Ring-style log buffer (capped at [`LOG_BUFFER_CAP`] entries).
    pub log_buffer: Mutex<VecDeque<String>>,
    /// Recent WARN/ERROR entries (capped at [`ACTIVE_ERRORS_CAP`] entries).
    pub active_errors: Mutex<VecDeque<ActiveErrorEntry>>,
    /// Recent connection lifecycle events for bug reports and diagnostics.
    pub connection_events: Mutex<VecDeque<DiagnosticConnectionEvent>>,
    /// Panic reports loaded from disk at startup. Save-to-file keeps them;
    /// successful server submission deletes the included files.
    pub panic_reports: Mutex<Vec<StoredPanicReport>>,
    /// Optional runtime hook for applying settings side effects (for example
    /// reconfiguring the local API server in the desktop app).
    pub settings_applier: Mutex<Option<Arc<SettingsApplier>>>,
    /// Material presets for cut/engrave operations.
    pub material_presets: Mutex<Vec<MaterialPreset>>,
    /// User-defined G-code macros.
    pub macros: Mutex<Vec<MacroDefinition>>,
    /// Console log entries (sent/received G-code, capped at [`CONSOLE_LOG_CAP`]).
    pub console_log: Mutex<VecDeque<ConsoleEntry>>,
    /// Ephemeral runtime state that lives alongside the persisted
    /// `Project.optimization` block. Today this carries `current_position`
    /// (live machine work position captured at plan time when
    /// `StartFromMode::CurrentPosition` is active); future runtime-only
    /// fields land here. **Never persisted** — survives only for the
    /// lifetime of the machine session.
    pub optimization_runtime: Mutex<OptimizationRuntime>,
    /// Loaded art library documents (reusable design asset collections).
    pub art_libraries: Mutex<Vec<super::persist::LoadedArtLibrary>>,
    /// Pending art-library load/persistence warnings surfaced to the UI on the next fetch.
    pub art_library_warnings: Mutex<Vec<String>>,
    /// Content-addressed cache for processed raster results (planner only).
    pub raster_cache: Arc<beambench_raster::cache::RasterCache>,
    /// Separate preview cache — avoids evicting planner entries with transient slider settings.
    pub preview_cache: Arc<beambench_raster::cache::RasterCache>,
    /// Staged decode+scale cache — shared between preview and planner for slider responsiveness.
    pub scaled_image_cache: Arc<beambench_raster::cache::ScaledImageCache>,
    /// Small LRU of decoded grayscale trace sources used by the trace dialog.
    pub trace_preview_source_cache: Mutex<TracePreviewSourceCache>,
    /// Latest frontend-owned trace preview request id used for cooperative cancellation.
    pub latest_trace_preview_request_id: AtomicU64,
    /// Latest frontend-owned plan/preview request id used for cooperative cancellation.
    pub latest_planning_request_id: Arc<AtomicU64>,
    /// Serializes agent design transactions so two apply/dry-run requests cannot
    /// race through the same project snapshot.
    pub design_transaction_lock: Mutex<()>,
    /// Last accepted frontend-owned selection snapshot. This is advisory context
    /// for agents and never gates normal app or hardware commands.
    pub agent_selection: Mutex<Option<AgentSelectionSnapshot>>,
}

impl ServiceContext {
    pub fn new() -> Self {
        let (tx, _rx) = broadcast::channel(256);
        let art_library_state = super::persist::load_art_libraries();
        Self {
            project: Mutex::new(None),
            project_path: Mutex::new(None),
            settings: Mutex::new(super::persist::load_settings()),
            plan_cache: Mutex::new(None),
            history: Mutex::new(ProjectHistory::default()),
            session: Mutex::new(None),
            controller_connection_gate: Mutex::new(()),
            pending_controller_connection: Mutex::new(None),
            job: Mutex::new(None),
            job_tick_loop_running: AtomicBool::new(false),
            active_jog: AtomicBool::new(false),
            machine_coordinates_valid: AtomicBool::new(false),
            active_laser_fire: Mutex::new(None),
            discovery_state: Mutex::new(DiscoveryScanState::default()),
            camera_devices_override: Mutex::new(None),
            camera_frames: Mutex::new(HashMap::new()),
            camera_overlay_runtime: Mutex::new(HashMap::new()),
            camera_latest_capture_artifact: Mutex::new(None),
            camera_latest_render_artifact: Mutex::new(None),
            camera_agent_bridge_connected: AtomicBool::new(false),
            pending_camera_capture_request: Mutex::new(None),
            pending_camera_overlay_render_request: Mutex::new(None),
            last_job_progress: Mutex::new(None),
            last_terminal_job: Mutex::new(None),
            events: tx,
            event_seq: AtomicU64::new(0),
            log_buffer: Mutex::new(VecDeque::new()),
            active_errors: Mutex::new(VecDeque::new()),
            connection_events: Mutex::new(VecDeque::new()),
            panic_reports: Mutex::new(Vec::new()),
            settings_applier: Mutex::new(None),
            material_presets: Mutex::new(super::persist::load_material_presets()),
            macros: Mutex::new(super::persist::load_macros()),
            console_log: Mutex::new(VecDeque::new()),
            optimization_runtime: Mutex::new(OptimizationRuntime::default()),
            art_libraries: Mutex::new(art_library_state.libraries),
            art_library_warnings: Mutex::new(art_library_state.warnings),
            raster_cache: Arc::new(beambench_raster::cache::RasterCache::new(32)),
            preview_cache: Arc::new(beambench_raster::cache::RasterCache::new(16)),
            scaled_image_cache: Arc::new(beambench_raster::cache::ScaledImageCache::new(16)),
            trace_preview_source_cache: Mutex::new(TracePreviewSourceCache::default()),
            latest_trace_preview_request_id: AtomicU64::new(0),
            latest_planning_request_id: Arc::new(AtomicU64::new(0)),
            design_transaction_lock: Mutex::new(()),
            agent_selection: Mutex::new(None),
        }
    }

    pub fn with_settings(settings: AppSettings) -> Self {
        let (tx, _rx) = broadcast::channel(256);
        Self {
            project: Mutex::new(None),
            project_path: Mutex::new(None),
            settings: Mutex::new(settings),
            plan_cache: Mutex::new(None),
            history: Mutex::new(ProjectHistory::default()),
            session: Mutex::new(None),
            controller_connection_gate: Mutex::new(()),
            pending_controller_connection: Mutex::new(None),
            job: Mutex::new(None),
            job_tick_loop_running: AtomicBool::new(false),
            active_jog: AtomicBool::new(false),
            machine_coordinates_valid: AtomicBool::new(false),
            active_laser_fire: Mutex::new(None),
            discovery_state: Mutex::new(DiscoveryScanState::default()),
            camera_devices_override: Mutex::new(None),
            camera_frames: Mutex::new(HashMap::new()),
            camera_overlay_runtime: Mutex::new(HashMap::new()),
            camera_latest_capture_artifact: Mutex::new(None),
            camera_latest_render_artifact: Mutex::new(None),
            camera_agent_bridge_connected: AtomicBool::new(false),
            pending_camera_capture_request: Mutex::new(None),
            pending_camera_overlay_render_request: Mutex::new(None),
            last_job_progress: Mutex::new(None),
            last_terminal_job: Mutex::new(None),
            events: tx,
            event_seq: AtomicU64::new(0),
            log_buffer: Mutex::new(VecDeque::new()),
            active_errors: Mutex::new(VecDeque::new()),
            connection_events: Mutex::new(VecDeque::new()),
            panic_reports: Mutex::new(Vec::new()),
            settings_applier: Mutex::new(None),
            material_presets: Mutex::new(Vec::new()),
            macros: Mutex::new(Vec::new()),
            console_log: Mutex::new(VecDeque::new()),
            optimization_runtime: Mutex::new(OptimizationRuntime::default()),
            art_libraries: Mutex::new(Vec::new()),
            art_library_warnings: Mutex::new(Vec::new()),
            raster_cache: Arc::new(beambench_raster::cache::RasterCache::new(32)),
            preview_cache: Arc::new(beambench_raster::cache::RasterCache::new(16)),
            scaled_image_cache: Arc::new(beambench_raster::cache::ScaledImageCache::new(16)),
            trace_preview_source_cache: Mutex::new(TracePreviewSourceCache::default()),
            latest_trace_preview_request_id: AtomicU64::new(0),
            latest_planning_request_id: Arc::new(AtomicU64::new(0)),
            design_transaction_lock: Mutex::new(()),
            agent_selection: Mutex::new(None),
        }
    }

    /// Push a formatted log line into the buffer, evicting the oldest entry
    /// when the buffer exceeds [`LOG_BUFFER_CAP`].
    pub fn push_log(&self, line: String) {
        if let Ok(mut buf) = self.log_buffer.lock() {
            buf.push_back(line);
            if buf.len() > LOG_BUFFER_CAP {
                buf.pop_front();
            }
        }
    }

    /// Push an error/warning entry, evicting the oldest when the list
    /// exceeds [`ACTIVE_ERRORS_CAP`].
    pub fn push_error(&self, line: String) {
        self.push_error_at(line, Utc::now());
    }

    /// Push an error/warning entry with an explicit timestamp.
    pub fn push_error_at(&self, line: String, ts: DateTime<Utc>) {
        if let Ok(mut errs) = self.active_errors.lock() {
            errs.push_back(ActiveErrorEntry { ts, line });
            if errs.len() > ACTIVE_ERRORS_CAP {
                errs.pop_front();
            }
        }
    }

    pub fn push_connection_event(
        &self,
        stage: impl Into<String>,
        port_name: Option<String>,
        baud_rate: Option<u32>,
        message: Option<String>,
        error: Option<String>,
    ) {
        if let Ok(mut events) = self.connection_events.lock() {
            events.push_back(DiagnosticConnectionEvent {
                ts: Utc::now().to_rfc3339(),
                stage: stage.into(),
                port_name,
                baud_rate,
                message,
                error,
            });
            while events.len() > CONNECTION_EVENTS_CAP {
                events.pop_front();
            }
        }
    }

    pub fn recent_connection_events(&self) -> Vec<DiagnosticConnectionEvent> {
        self.connection_events
            .lock()
            .map(|events| events.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn set_panic_reports(&self, reports: Vec<StoredPanicReport>) {
        if let Ok(mut guard) = self.panic_reports.lock() {
            *guard = reports;
        }
    }

    pub fn recent_panic_reports(&self) -> Vec<StoredPanicReport> {
        self.panic_reports
            .lock()
            .map(|reports| reports.clone())
            .unwrap_or_default()
    }

    pub fn cleanup_tracked_camera_frame_files(&self) -> usize {
        let Ok(mut frames) = self.camera_frames.lock() else {
            return 0;
        };
        let mut deleted = 0usize;
        for (_, frame) in frames.drain() {
            if frame.file_path.is_empty() {
                continue;
            }
            if std::fs::remove_file(&frame.file_path).is_ok() {
                deleted += 1;
            }
        }
        deleted
    }

    /// Emit a JSON event to all connected WebSocket clients.
    /// Silently ignores send failures (no subscribers).
    pub fn next_event_id(&self) -> u64 {
        self.event_seq.fetch_add(1, Ordering::Relaxed) + 1
    }

    pub fn build_event(
        &self,
        event_type: &str,
        payload: serde_json::Value,
    ) -> ServiceEventEnvelope {
        ServiceEventEnvelope {
            id: self.next_event_id(),
            event_type: event_type.to_string(),
            timestamp: crate::events::timestamp(),
            payload,
        }
    }

    pub fn event_message(&self, event_type: &str, payload: serde_json::Value) -> String {
        self.build_event(event_type, payload).to_json()
    }

    pub fn emit_event(&self, event_type: &str, payload: serde_json::Value) {
        let msg = self.event_message(event_type, payload);
        // Ignore error — means no active subscribers
        let _ = self.events.send(msg);
    }

    /// Capture the current project state before a mutation so undo/redo is
    /// owned by the shared service layer instead of a frontend snapshot stack.
    pub fn push_project_undo_snapshot(&self, project: &Project) -> Result<(), String> {
        let mut history = self
            .history
            .lock()
            .map_err(|e| format!("Failed to lock history: {e}"))?;
        history.push_snapshot(project);
        Ok(())
    }

    pub fn clear_project_history(&self) -> Result<(), String> {
        let mut history = self
            .history
            .lock()
            .map_err(|e| format!("Failed to lock history: {e}"))?;
        history.clear();
        Ok(())
    }

    pub fn undo_state(&self) -> Result<UndoState, String> {
        let history = self
            .history
            .lock()
            .map_err(|e| format!("Failed to lock history: {e}"))?;
        Ok(history.state())
    }

    pub fn set_settings_applier<F>(&self, applier: F) -> Result<(), String>
    where
        F: Fn(&AppSettings) -> Result<(), String> + Send + Sync + 'static,
    {
        let mut guard = self
            .settings_applier
            .lock()
            .map_err(|e| format!("Failed to lock settings_applier: {e}"))?;
        *guard = Some(Arc::new(applier));
        Ok(())
    }

    pub fn apply_settings_side_effects(&self, settings: &AppSettings) -> Result<(), String> {
        let applier = self
            .settings_applier
            .lock()
            .map_err(|e| format!("Failed to lock settings_applier: {e}"))?
            .clone();
        if let Some(applier) = applier {
            applier(settings)?;
        }
        Ok(())
    }

    // --- Material Presets CRUD ---

    pub fn get_materials(&self) -> Result<Vec<MaterialPreset>, String> {
        let presets = self
            .material_presets
            .lock()
            .map_err(|e| format!("Failed to lock material_presets: {e}"))?;
        Ok(presets.clone())
    }

    pub fn save_material(&self, preset: MaterialPreset) -> Result<(), String> {
        let mut presets = self
            .material_presets
            .lock()
            .map_err(|e| format!("Failed to lock material_presets: {e}"))?;

        // Update existing or insert new
        if let Some(existing) = presets.iter_mut().find(|p| p.id == preset.id) {
            *existing = preset;
        } else {
            presets.push(preset);
        }

        Ok(())
    }

    pub fn replace_materials(&self, presets: Vec<MaterialPreset>) -> Result<(), String> {
        let mut stored = self
            .material_presets
            .lock()
            .map_err(|e| format!("Failed to lock material_presets: {e}"))?;
        *stored = presets;
        Ok(())
    }

    pub fn delete_material(&self, id: Uuid) -> Result<(), String> {
        let mut presets = self
            .material_presets
            .lock()
            .map_err(|e| format!("Failed to lock material_presets: {e}"))?;
        presets.retain(|p| p.id != id);
        Ok(())
    }

    // --- Macro Definitions CRUD ---

    pub fn get_macros(&self) -> Result<Vec<MacroDefinition>, String> {
        let macros = self
            .macros
            .lock()
            .map_err(|e| format!("Failed to lock macros: {e}"))?;
        Ok(macros.clone())
    }

    pub fn save_macro(&self, m: MacroDefinition) -> Result<(), String> {
        let mut macros = self
            .macros
            .lock()
            .map_err(|e| format!("Failed to lock macros: {e}"))?;

        // Update existing or insert new
        if let Some(existing) = macros.iter_mut().find(|mac| mac.id == m.id) {
            *existing = m;
        } else {
            macros.push(m);
        }

        Ok(())
    }

    pub fn replace_macros(&self, macros: Vec<MacroDefinition>) -> Result<(), String> {
        let mut stored = self
            .macros
            .lock()
            .map_err(|e| format!("Failed to lock macros: {e}"))?;
        *stored = macros;
        Ok(())
    }

    pub fn delete_macro(&self, id: Uuid) -> Result<(), String> {
        let mut macros = self
            .macros
            .lock()
            .map_err(|e| format!("Failed to lock macros: {e}"))?;
        macros.retain(|m| m.id != id);
        Ok(())
    }

    // --- Console Log ---

    pub fn get_console_log(&self, limit: usize) -> Result<Vec<ConsoleEntry>, String> {
        self.sync_session_console_log()?;
        let log = self
            .console_log
            .lock()
            .map_err(|e| format!("Failed to lock console_log: {e}"))?;

        // Return the last `limit` entries
        let start = log.len().saturating_sub(limit);
        Ok(log.iter().skip(start).cloned().collect())
    }

    fn sync_session_console_log(&self) -> Result<(), String> {
        let job_active = self
            .job
            .lock()
            .map_err(|e| format!("Failed to lock job: {e}"))?
            .is_some();
        let mut session = self
            .session
            .lock()
            .map_err(|e| format!("Failed to lock session: {e}"))?;
        let Some(session) = session.as_mut() else {
            return Ok(());
        };

        // During a job, only the streamer may consume controller responses;
        // polling here would steal acknowledgements from its byte accounting.
        if !job_active {
            session.poll();
        }
        let mut session_entries = session.console_entries(CONSOLE_LOG_CAP);

        // Session logs are newest-first. The desktop console expects chronological
        // order, and the service log retains entries across panel close/reopen.
        session_entries.reverse();
        if session_entries.is_empty() {
            return Ok(());
        }

        let mut log = self
            .console_log
            .lock()
            .map_err(|e| format!("Failed to lock console_log: {e}"))?;
        if log.back() == session_entries.last() {
            return Ok(());
        }

        let first_new = session_entries
            .iter()
            .rposition(|entry| log.iter().any(|existing| existing == entry))
            .map_or(0, |index| index + 1);
        for entry in session_entries.into_iter().skip(first_new) {
            log.push_back(entry);
            if log.len() > CONSOLE_LOG_CAP {
                log.pop_front();
            }
        }
        Ok(())
    }

    pub fn clear_console_log(&self) -> Result<(), String> {
        {
            let mut session = self
                .session
                .lock()
                .map_err(|e| format!("Failed to lock session: {e}"))?;
            if let Some(session) = session.as_mut() {
                session.clear_console_entries();
            }
        }
        self.console_log
            .lock()
            .map_err(|e| format!("Failed to lock console_log: {e}"))?
            .clear();
        Ok(())
    }

    pub fn send_gcode_line(&self, line: &str) -> Result<(), String> {
        // Raw G-code (console, macros, API) must never be injected while a job
        // is streaming: it bypasses the streamer's byte-budget accounting and
        // desyncs ack tracking against GRBL's 128-byte RX buffer.
        {
            let job = self
                .job
                .lock()
                .map_err(|e| format!("Failed to lock job: {e}"))?;
            if job.is_some() {
                return Err(
                    "Cannot send G-code while a job is active. Cancel the job or wait for it to finish first."
                        .to_string(),
                );
            }
        }

        {
            let mut session = self
                .session
                .lock()
                .map_err(|e| format!("Failed to lock session: {e}"))?;
            let Some(session) = session.as_mut() else {
                return Err("No active machine session".to_string());
            };
            match session {
                crate::runtime::MachineSessionHandle::Grbl(grbl) => {
                    grbl.send_command(line)
                        .map_err(|e| format!("Failed to send command: {e}"))?;
                }
                _ => {
                    return Err("Only GRBL sessions support sending arbitrary G-code".to_string());
                }
            }
        }

        // GrblSession records the sent line. Synchronize that authoritative
        // entry instead of creating a second service-side copy.
        self.sync_session_console_log()?;
        Ok(())
    }

    // --- Material Application ---

    pub fn apply_material(
        &self,
        preset_id: Uuid,
        layer_id: LayerId,
        project: &mut Project,
    ) -> Result<MaterialApplyResponse, String> {
        let presets = self
            .material_presets
            .lock()
            .map_err(|e| format!("Failed to lock material_presets: {e}"))?;

        let preset = presets
            .iter()
            .find(|p| p.id == preset_id)
            .ok_or_else(|| format!("Material preset {preset_id} not found"))?
            .clone();
        drop(presets);

        let layer = project
            .layers
            .iter_mut()
            .find(|l| l.id == layer_id)
            .ok_or_else(|| format!("Layer {layer_id} not found"))?;
        if layer.is_tool_layer {
            return Err("Tool layers do not support material presets".to_string());
        }
        let mut warnings = Vec::new();
        if layer.entries.len() > 1 {
            warnings.push(MaterialApplyWarning {
                code: MaterialApplyWarningCode::MultiEntryLayerTargetedPrimary,
                message: "Material preset applied to the primary sub-layer only".to_string(),
            });
        }

        let seed = layer.primary_entry().clone();
        let (updated, entry_warnings) =
            crate::material_apply::apply_material_to_entry(&preset, &seed);
        warnings.extend(entry_warnings);
        let targeted_entry_id = updated.id;
        *layer.primary_entry_mut() = updated;

        Ok(MaterialApplyResponse {
            applied_layer_id: layer_id,
            targeted_entry_id,
            warnings,
        })
    }

    /// Transient preset application — pure path used by quality-test dialogs.
    ///
    /// Looks up the preset by id, applies it to a caller-supplied seed entry, returns the updated
    /// entry plus warnings. Does not touch any project state, plan cache, undo, or events.
    pub fn apply_preset_to_seed(
        &self,
        preset_id: Uuid,
        seed: CutEntry,
    ) -> Result<(CutEntry, Vec<MaterialApplyWarning>), String> {
        let presets = self
            .material_presets
            .lock()
            .map_err(|e| format!("Failed to lock material_presets: {e}"))?;
        let preset = presets
            .iter()
            .find(|p| p.id == preset_id)
            .ok_or_else(|| format!("Material preset {preset_id} not found"))?
            .clone();
        drop(presets);
        Ok(crate::material_apply::apply_material_to_entry(
            &preset, &seed,
        ))
    }

    // --- Art Library CRUD ---

    fn queue_art_library_warning(&self, warning: impl Into<String>) -> Result<(), String> {
        let mut warnings = self
            .art_library_warnings
            .lock()
            .map_err(|e| format!("Failed to lock art_library_warnings: {e}"))?;
        warnings.push(warning.into());
        Ok(())
    }

    fn persist_art_library_manifest(
        &self,
        libraries: &[super::persist::LoadedArtLibrary],
    ) -> Result<(), String> {
        let manifest = super::persist::ArtLibraryManifest {
            format_version: beambench_core::ART_LIBRARY_FORMAT_VERSION.to_string(),
            loaded_paths: libraries
                .iter()
                .map(|library| library.path.clone())
                .collect(),
        };
        super::persist::save_art_library_manifest(&manifest)
    }

    fn persist_loaded_art_library(
        &self,
        library: &mut super::persist::LoadedArtLibrary,
    ) -> Result<(), String> {
        match super::persist::save_art_library_document(&library.path, &library.document) {
            Ok(()) => {
                library.save_error = None;
                Ok(())
            }
            Err(err) => {
                library.save_error = Some(err.clone());
                self.queue_art_library_warning(format!(
                    "Failed to save art library {}: {err}",
                    library.path.display()
                ))?;
                Ok(())
            }
        }
    }

    pub fn get_art_libraries(&self) -> Result<super::persist::ArtLibraryLoadState, String> {
        let libs = self
            .art_libraries
            .lock()
            .map_err(|e| format!("Failed to lock art_libraries: {e}"))?
            .clone();
        let warnings = {
            let mut guard = self
                .art_library_warnings
                .lock()
                .map_err(|e| format!("Failed to lock art_library_warnings: {e}"))?;
            std::mem::take(&mut *guard)
        };
        Ok(super::persist::ArtLibraryLoadState {
            libraries: libs,
            warnings,
        })
    }

    pub fn create_art_library(
        &self,
        path: PathBuf,
        name: &str,
    ) -> Result<super::persist::LoadedArtLibrary, String> {
        let mut libs = self
            .art_libraries
            .lock()
            .map_err(|e| format!("Failed to lock art_libraries: {e}"))?;
        if libs.iter().any(|l| l.path == path) {
            return Err(format!(
                "Library path '{}' is already loaded",
                path.display()
            ));
        }
        let document = ArtLibraryDocument::new(name);
        super::persist::save_art_library_document(&path, &document)?;
        let library = super::persist::LoadedArtLibrary {
            document,
            path,
            save_error: None,
        };
        libs.push(library.clone());
        self.persist_art_library_manifest(&libs)?;
        Ok(library)
    }

    pub fn load_art_library(
        &self,
        path: PathBuf,
    ) -> Result<super::persist::LoadedArtLibrary, String> {
        let mut libs = self
            .art_libraries
            .lock()
            .map_err(|e| format!("Failed to lock art_libraries: {e}"))?;
        if let Some(existing) = libs.iter().find(|library| library.path == path) {
            return Ok(existing.clone());
        }

        let mut document = super::persist::load_art_library_document(&path)?;
        let mut save_error = None;
        if libs
            .iter()
            .any(|library| library.document.library_id == document.library_id)
        {
            document.library_id = Uuid::new_v4();
            match super::persist::save_art_library_document(&path, &document) {
                Ok(()) => self.queue_art_library_warning(format!(
                    "Detected duplicate library id in {}, assigned a new id and saved it.",
                    path.display()
                ))?,
                Err(err) => {
                    save_error = Some(err.clone());
                    self.queue_art_library_warning(format!(
                        "Detected duplicate library id in {} but failed to persist the new id: {err}",
                        path.display()
                    ))?;
                }
            }
        }

        let library = super::persist::LoadedArtLibrary {
            document,
            path,
            save_error,
        };
        libs.push(library.clone());
        self.persist_art_library_manifest(&libs)?;
        Ok(library)
    }

    pub fn unload_art_library(&self, library_id: Uuid) -> Result<(), String> {
        let mut libs = self
            .art_libraries
            .lock()
            .map_err(|e| format!("Failed to lock art_libraries: {e}"))?;
        libs.retain(|library| library.document.library_id != library_id);
        self.persist_art_library_manifest(&libs)
    }

    pub fn save_art_library_as(
        &self,
        library_id: Uuid,
        path: PathBuf,
    ) -> Result<super::persist::LoadedArtLibrary, String> {
        let mut libs = self
            .art_libraries
            .lock()
            .map_err(|e| format!("Failed to lock art_libraries: {e}"))?;
        if libs
            .iter()
            .any(|library| library.path == path && library.document.library_id != library_id)
        {
            return Err(format!(
                "Library path '{}' is already loaded",
                path.display()
            ));
        }
        let idx = libs
            .iter()
            .position(|library| library.document.library_id == library_id)
            .ok_or_else(|| format!("Library '{library_id}' not found"))?;
        let updated = {
            let library = &mut libs[idx];
            super::persist::save_art_library_document(&path, &library.document)?;
            library.path = path;
            library.save_error = None;
            library.clone()
        };
        self.persist_art_library_manifest(&libs)?;
        Ok(updated)
    }

    pub fn delete_art_library(&self, library_id: Uuid) -> Result<(), String> {
        let mut libs = self
            .art_libraries
            .lock()
            .map_err(|e| format!("Failed to lock art_libraries: {e}"))?;
        let Some(idx) = libs
            .iter()
            .position(|library| library.document.library_id == library_id)
        else {
            return Err(format!("Library '{library_id}' not found"));
        };
        let path = libs[idx].path.clone();
        super::persist::delete_art_library_document(&path)?;
        libs.remove(idx);
        self.persist_art_library_manifest(&libs)
    }

    pub fn rename_art_library(
        &self,
        library_id: Uuid,
        name: &str,
    ) -> Result<super::persist::LoadedArtLibrary, String> {
        let mut libs = self
            .art_libraries
            .lock()
            .map_err(|e| format!("Failed to lock art_libraries: {e}"))?;
        let library = libs
            .iter_mut()
            .find(|library| library.document.library_id == library_id)
            .ok_or_else(|| format!("Library '{library_id}' not found"))?;
        library.document.name = name.to_string();
        self.persist_loaded_art_library(library)?;
        Ok(library.clone())
    }

    pub fn add_art_library_item(
        &self,
        library_id: Uuid,
        item: ArtLibraryItem,
    ) -> Result<super::persist::LoadedArtLibrary, String> {
        let mut libs = self
            .art_libraries
            .lock()
            .map_err(|e| format!("Failed to lock art_libraries: {e}"))?;
        let library = libs
            .iter_mut()
            .find(|library| library.document.library_id == library_id)
            .ok_or_else(|| format!("Library '{library_id}' not found"))?;
        library.document.items.push(item);
        self.persist_loaded_art_library(library)?;
        Ok(library.clone())
    }

    pub fn add_art_library_item_dedupe_external(
        &self,
        library_id: Uuid,
        item: ArtLibraryItem,
    ) -> Result<(ArtLibraryItem, bool), String> {
        let mut libs = self
            .art_libraries
            .lock()
            .map_err(|e| format!("Failed to lock art_libraries: {e}"))?;
        let library = libs
            .iter_mut()
            .find(|library| library.document.library_id == library_id)
            .ok_or_else(|| format!("Library '{library_id}' not found"))?;

        if let Some(existing) = library.document.items.iter().find(|existing| {
            // Only external-file items dedupe by stored source bytes. Snapshot
            // payloads are intentionally excluded from this check.
            existing.kind == ArtLibraryItemKind::ExternalFile
                && existing.media_type == item.media_type
                && existing.data == item.data
        }) {
            return Ok((existing.clone(), true));
        }

        library.document.items.push(item.clone());
        self.persist_loaded_art_library(library)?;
        Ok((item, false))
    }

    pub fn find_duplicate_art_library_external_item(
        &self,
        library_id: Uuid,
        media_type: &str,
        data: &str,
    ) -> Result<Option<ArtLibraryItem>, String> {
        let libs = self
            .art_libraries
            .lock()
            .map_err(|e| format!("Failed to lock art_libraries: {e}"))?;
        let library = libs
            .iter()
            .find(|library| library.document.library_id == library_id)
            .ok_or_else(|| format!("Library '{library_id}' not found"))?;
        Ok(library
            .document
            .items
            .iter()
            .find(|item| {
                // Only external-file items dedupe by stored source bytes. Snapshot
                // payloads are intentionally excluded from this check.
                item.kind == ArtLibraryItemKind::ExternalFile
                    && item.media_type == media_type
                    && item.data == data
            })
            .cloned())
    }

    pub fn rename_art_library_item(
        &self,
        library_id: Uuid,
        item_id: Uuid,
        name: &str,
    ) -> Result<super::persist::LoadedArtLibrary, String> {
        let mut libs = self
            .art_libraries
            .lock()
            .map_err(|e| format!("Failed to lock art_libraries: {e}"))?;
        let library = libs
            .iter_mut()
            .find(|library| library.document.library_id == library_id)
            .ok_or_else(|| format!("Library '{library_id}' not found"))?;
        let item = library
            .document
            .items
            .iter_mut()
            .find(|item| item.id == item_id)
            .ok_or_else(|| format!("Item '{item_id}' not found"))?;
        item.name = name.to_string();
        self.persist_loaded_art_library(library)?;
        Ok(library.clone())
    }

    pub fn commit_art_library_thumbnail(
        &self,
        library_id: Uuid,
        item_id: Uuid,
        thumbnail: Option<String>,
    ) -> Result<super::persist::LoadedArtLibrary, String> {
        let mut libs = self
            .art_libraries
            .lock()
            .map_err(|e| format!("Failed to lock art_libraries: {e}"))?;
        let library = libs
            .iter_mut()
            .find(|library| library.document.library_id == library_id)
            .ok_or_else(|| format!("Library '{library_id}' not found"))?;
        let item = library
            .document
            .items
            .iter_mut()
            .find(|item| item.id == item_id)
            .ok_or_else(|| format!("Item '{item_id}' not found"))?;
        item.thumbnail = thumbnail;
        self.persist_loaded_art_library(library)?;
        Ok(library.clone())
    }

    pub fn remove_art_library_item(
        &self,
        library_id: Uuid,
        item_id: uuid::Uuid,
    ) -> Result<super::persist::LoadedArtLibrary, String> {
        let mut libs = self
            .art_libraries
            .lock()
            .map_err(|e| format!("Failed to lock art_libraries: {e}"))?;
        let library = libs
            .iter_mut()
            .find(|library| library.document.library_id == library_id)
            .ok_or_else(|| format!("Library '{library_id}' not found"))?;
        library.document.items.retain(|item| item.id != item_id);
        self.persist_loaded_art_library(library)?;
        Ok(library.clone())
    }

    pub fn copy_art_library_item(
        &self,
        source_library_id: Uuid,
        item_id: Uuid,
        target_library_id: Uuid,
        remove_source: bool,
    ) -> Result<(), String> {
        let mut libs = self
            .art_libraries
            .lock()
            .map_err(|e| format!("Failed to lock art_libraries: {e}"))?;
        let source_idx = libs
            .iter()
            .position(|library| library.document.library_id == source_library_id)
            .ok_or_else(|| format!("Library '{source_library_id}' not found"))?;
        let target_idx = libs
            .iter()
            .position(|library| library.document.library_id == target_library_id)
            .ok_or_else(|| format!("Library '{target_library_id}' not found"))?;
        if source_idx == target_idx {
            return Ok(());
        }

        let source_item = libs[source_idx]
            .document
            .items
            .iter()
            .find(|item| item.id == item_id)
            .cloned()
            .ok_or_else(|| format!("Item '{item_id}' not found"))?;
        let mut copied = source_item;
        copied.id = Uuid::new_v4();
        libs[target_idx].document.items.push(copied);
        self.persist_loaded_art_library(&mut libs[target_idx])?;
        if remove_source {
            libs[source_idx]
                .document
                .items
                .retain(|item| item.id != item_id);
            self.persist_loaded_art_library(&mut libs[source_idx])?;
        }
        Ok(())
    }

    pub fn art_library_item(
        &self,
        library_id: Uuid,
        item_id: Uuid,
    ) -> Result<ArtLibraryItem, String> {
        let libs = self
            .art_libraries
            .lock()
            .map_err(|e| format!("Failed to lock art_libraries: {e}"))?;
        let library = libs
            .iter()
            .find(|library| library.document.library_id == library_id)
            .ok_or_else(|| format!("Library '{library_id}' not found"))?;
        library
            .document
            .items
            .iter()
            .find(|item| item.id == item_id)
            .cloned()
            .ok_or_else(|| format!("Item '{item_id}' not found"))
    }
}

impl Default for ServiceContext {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for ServiceContext {
    fn drop(&mut self) {
        let _ = self.cleanup_tracked_camera_frame_files();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persist;
    use crate::runtime::MachineSessionHandle;
    use beambench_common::ConsoleDirection;
    use beambench_grbl::GrblSession;
    use beambench_serial::MockSerialTransport;

    #[test]
    fn new_context_has_no_project() {
        let ctx = ServiceContext::new();
        let guard = ctx.project.lock().unwrap();
        assert!(guard.is_none());
    }

    #[test]
    fn new_context_has_no_path() {
        let ctx = ServiceContext::new();
        let guard = ctx.project_path.lock().unwrap();
        assert!(guard.is_none());
    }

    #[test]
    fn new_context_has_default_settings() {
        let ctx = ServiceContext::new();
        let guard = ctx.settings.lock().unwrap();
        let expected = super::super::persist::load_settings();
        assert_eq!(guard.autosave_enabled, expected.autosave_enabled);
        assert_eq!(
            guard.autosave_interval_secs,
            expected.autosave_interval_secs
        );
    }

    #[test]
    fn with_settings_stores_custom_settings() {
        let settings = AppSettings {
            autosave_enabled: false,
            ..Default::default()
        };
        let ctx = ServiceContext::with_settings(settings);
        let guard = ctx.settings.lock().unwrap();
        assert!(!guard.autosave_enabled);
    }

    #[test]
    fn new_context_has_no_plan_cache() {
        let ctx = ServiceContext::new();
        let guard = ctx.plan_cache.lock().unwrap();
        assert!(guard.is_none());
    }

    #[test]
    fn new_context_has_empty_history() {
        let ctx = ServiceContext::new();
        let guard = ctx.history.lock().unwrap();
        let state = guard.state();
        assert!(!state.can_undo);
        assert!(!state.can_redo);
    }

    #[test]
    fn new_context_has_no_session() {
        let ctx = ServiceContext::new();
        let guard = ctx.session.lock().unwrap();
        assert!(guard.is_none());
    }

    #[test]
    fn new_context_has_no_job() {
        let ctx = ServiceContext::new();
        let guard = ctx.job.lock().unwrap();
        assert!(guard.is_none());
    }

    #[test]
    fn connection_events_keep_newest_32_entries() {
        let ctx = ServiceContext::new();
        for index in 0..40 {
            ctx.push_connection_event(
                format!("stage_{index}"),
                Some(format!("port_{index}")),
                Some(115200),
                None,
                None,
            );
        }

        let events = ctx.recent_connection_events();
        assert_eq!(events.len(), 32);
        assert_eq!(events[0].stage, "stage_8");
        assert_eq!(events[31].stage, "stage_39");
    }

    #[test]
    fn new_context_has_idle_discovery_state() {
        let ctx = ServiceContext::new();
        let guard = ctx.discovery_state.lock().unwrap();
        assert!(guard.candidates.is_empty());
    }

    #[test]
    fn new_context_has_no_camera_frames() {
        let ctx = ServiceContext::new();
        let guard = ctx.camera_frames.lock().unwrap();
        assert!(guard.is_empty());
    }

    #[test]
    fn default_is_same_as_new() {
        let ctx = ServiceContext::default();
        assert!(ctx.project.lock().unwrap().is_none());
        let expected = super::super::persist::load_settings();
        assert_eq!(
            ctx.settings.lock().unwrap().autosave_enabled,
            expected.autosave_enabled
        );
    }

    #[test]
    fn emit_event_does_not_panic_without_subscribers() {
        let ctx = ServiceContext::new();
        ctx.emit_event("test", serde_json::json!({"key": "value"}));
        // Should not panic even with no subscribers
    }

    #[test]
    fn emit_event_sends_to_subscriber() {
        let ctx = ServiceContext::new();
        let mut rx = ctx.events.subscribe();
        ctx.emit_event("project.opened", serde_json::json!({"name": "demo"}));
        let msg = rx.try_recv().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&msg).unwrap();
        assert_eq!(parsed["id"], 1);
        assert_eq!(parsed["type"], "project.opened");
        assert!(parsed["timestamp"].is_string());
        assert_eq!(parsed["payload"]["name"], "demo");
    }

    #[test]
    fn new_context_has_empty_log_buffer() {
        let ctx = ServiceContext::new();
        let buf = ctx.log_buffer.lock().unwrap();
        assert!(buf.is_empty());
    }

    #[test]
    fn new_context_has_empty_active_errors() {
        let ctx = ServiceContext::new();
        let errs = ctx.active_errors.lock().unwrap();
        assert!(errs.is_empty());
    }

    #[test]
    fn push_log_appends_entry() {
        let ctx = ServiceContext::new();
        ctx.push_log("INFO test: hello".to_string());
        ctx.push_log("DEBUG test: world".to_string());
        let buf = ctx.log_buffer.lock().unwrap();
        assert_eq!(buf.len(), 2);
        assert_eq!(buf[0], "INFO test: hello");
        assert_eq!(buf[1], "DEBUG test: world");
    }

    #[test]
    fn push_log_caps_at_500() {
        let ctx = ServiceContext::new();
        for i in 0..510 {
            ctx.push_log(format!("line {i}"));
        }
        let buf = ctx.log_buffer.lock().unwrap();
        assert_eq!(buf.len(), 500);
        // Oldest entries (0..9) should have been evicted
        assert_eq!(buf[0], "line 10");
        assert_eq!(buf[499], "line 509");
    }

    #[test]
    fn push_error_appends_entry() {
        let ctx = ServiceContext::new();
        ctx.push_error("WARN test: something".to_string());
        let errs = ctx.active_errors.lock().unwrap();
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].line, "WARN test: something");
    }

    #[test]
    fn push_error_caps_at_100() {
        let ctx = ServiceContext::new();
        for i in 0..110 {
            ctx.push_error(format!("err {i}"));
        }
        let errs = ctx.active_errors.lock().unwrap();
        assert_eq!(errs.len(), 100);
        assert_eq!(errs[0].line, "err 10");
        assert_eq!(errs[99].line, "err 109");
    }

    #[test]
    fn push_error_preserves_timestamp() {
        let ctx = ServiceContext::new();
        let ts = DateTime::parse_from_rfc3339("2026-05-14T12:34:56Z")
            .unwrap()
            .with_timezone(&Utc);
        ctx.push_error_at("ERROR test: timed".to_string(), ts);

        let errs = ctx.active_errors.lock().unwrap();
        assert_eq!(errs[0].ts, ts);
        assert_eq!(errs[0].line, "ERROR test: timed");
    }

    // --- Material preset, macro, and console-log tests ---

    #[test]
    fn new_context_has_empty_material_presets() {
        let ctx = ServiceContext::with_settings(AppSettings::default());
        let presets = ctx.material_presets.lock().unwrap();
        assert!(presets.is_empty());
    }

    #[test]
    fn new_context_has_empty_macros() {
        let ctx = ServiceContext::with_settings(AppSettings::default());
        let macros = ctx.macros.lock().unwrap();
        assert!(macros.is_empty());
    }

    #[test]
    fn new_context_has_empty_console_log() {
        let ctx = ServiceContext::new();
        let log = ctx.console_log.lock().unwrap();
        assert!(log.is_empty());
    }

    #[test]
    fn save_material_inserts_new_preset() {
        let ctx = ServiceContext::with_settings(AppSettings::default());
        let preset = MaterialPreset {
            id: Uuid::new_v4(),
            name: "Plywood".to_string(),
            material: "Plywood".to_string(),
            thickness_mm: 3.0,
            operation: beambench_core::OperationType::Cut,
            speed_mm_min: 800.0,
            power_percent: 80.0,
            passes: 2,
            dpi: None,
            raster_mode: None,
            line_interval_mm: None,
            scan_angle: None,
            bidirectional: None,
            overscan_mm: None,
            flood_fill: None,
            angle_passes: None,
            angle_increment_deg: None,
            pass_through: None,
            halftone_cells_per_inch: None,
            halftone_angle_deg: None,
            newsprint_angle_deg: None,
            newsprint_frequency: None,
            invert: None,
            dot_width_correction_mm: None,
            ramp_length_mm: None,
            notes: String::new(),
            category: String::new(),
            device_profile: None,
        };
        let id = preset.id;

        ctx.save_material(preset).unwrap();

        let presets = ctx.get_materials().unwrap();
        assert_eq!(presets.len(), 1);
        assert_eq!(presets[0].id, id);
        assert_eq!(presets[0].name, "Plywood");
    }

    #[test]
    fn save_material_updates_existing_preset() {
        let ctx = ServiceContext::with_settings(AppSettings::default());
        let id = Uuid::new_v4();
        let preset1 = MaterialPreset {
            id,
            name: "Old Name".to_string(),
            material: "Wood".to_string(),
            thickness_mm: 3.0,
            operation: beambench_core::OperationType::Cut,
            speed_mm_min: 800.0,
            power_percent: 80.0,
            passes: 1,
            dpi: None,
            raster_mode: None,
            line_interval_mm: None,
            scan_angle: None,
            bidirectional: None,
            overscan_mm: None,
            flood_fill: None,
            angle_passes: None,
            angle_increment_deg: None,
            pass_through: None,
            halftone_cells_per_inch: None,
            halftone_angle_deg: None,
            newsprint_angle_deg: None,
            newsprint_frequency: None,
            invert: None,
            dot_width_correction_mm: None,
            ramp_length_mm: None,
            notes: String::new(),
            category: String::new(),
            device_profile: None,
        };

        ctx.save_material(preset1).unwrap();

        let preset2 = MaterialPreset {
            id,
            name: "New Name".to_string(),
            material: "Wood".to_string(),
            thickness_mm: 3.0,
            operation: beambench_core::OperationType::Cut,
            speed_mm_min: 900.0,
            power_percent: 85.0,
            passes: 2,
            dpi: None,
            raster_mode: None,
            line_interval_mm: None,
            scan_angle: None,
            bidirectional: None,
            overscan_mm: None,
            flood_fill: None,
            angle_passes: None,
            angle_increment_deg: None,
            pass_through: None,
            halftone_cells_per_inch: None,
            halftone_angle_deg: None,
            newsprint_angle_deg: None,
            newsprint_frequency: None,
            invert: None,
            dot_width_correction_mm: None,
            ramp_length_mm: None,
            notes: String::new(),
            category: String::new(),
            device_profile: None,
        };

        ctx.save_material(preset2).unwrap();

        let presets = ctx.get_materials().unwrap();
        assert_eq!(presets.len(), 1);
        assert_eq!(presets[0].name, "New Name");
        assert_eq!(presets[0].speed_mm_min, 900.0);
    }

    #[test]
    fn delete_material_removes_preset() {
        let ctx = ServiceContext::with_settings(AppSettings::default());
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        let preset1 = MaterialPreset {
            id: id1,
            name: "Preset 1".to_string(),
            ..Default::default()
        };
        let preset2 = MaterialPreset {
            id: id2,
            name: "Preset 2".to_string(),
            ..Default::default()
        };

        ctx.save_material(preset1).unwrap();
        ctx.save_material(preset2).unwrap();

        ctx.delete_material(id1).unwrap();

        let presets = ctx.get_materials().unwrap();
        assert_eq!(presets.len(), 1);
        assert_eq!(presets[0].id, id2);
    }

    #[test]
    fn save_macro_inserts_new_macro() {
        let ctx = ServiceContext::with_settings(AppSettings::default());
        let macro_def = MacroDefinition {
            id: Uuid::new_v4(),
            name: "Home".to_string(),
            description: "Home the machine".to_string(),
            commands: vec!["$H".to_string()],
            hotkey: None,
            show_in_toolbar: false,
        };
        let id = macro_def.id;

        ctx.save_macro(macro_def).unwrap();

        let macros = ctx.get_macros().unwrap();
        assert_eq!(macros.len(), 1);
        assert_eq!(macros[0].id, id);
        assert_eq!(macros[0].name, "Home");
    }

    #[test]
    fn save_macro_updates_existing_macro() {
        let ctx = ServiceContext::with_settings(AppSettings::default());
        let id = Uuid::new_v4();
        let macro1 = MacroDefinition {
            id,
            name: "Old".to_string(),
            description: "Old desc".to_string(),
            commands: vec!["G0 X0".to_string()],
            hotkey: None,
            show_in_toolbar: false,
        };

        ctx.save_macro(macro1).unwrap();

        let macro2 = MacroDefinition {
            id,
            name: "New".to_string(),
            description: "New desc".to_string(),
            commands: vec!["G0 X10".to_string()],
            hotkey: None,
            show_in_toolbar: false,
        };

        ctx.save_macro(macro2).unwrap();

        let macros = ctx.get_macros().unwrap();
        assert_eq!(macros.len(), 1);
        assert_eq!(macros[0].name, "New");
    }

    #[test]
    fn delete_macro_removes_macro() {
        let ctx = ServiceContext::with_settings(AppSettings::default());
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        let macro1 = MacroDefinition {
            id: id1,
            name: "Macro 1".to_string(),
            ..Default::default()
        };
        let macro2 = MacroDefinition {
            id: id2,
            name: "Macro 2".to_string(),
            ..Default::default()
        };

        ctx.save_macro(macro1).unwrap();
        ctx.save_macro(macro2).unwrap();

        ctx.delete_macro(id1).unwrap();

        let macros = ctx.get_macros().unwrap();
        assert_eq!(macros.len(), 1);
        assert_eq!(macros[0].id, id2);
    }

    #[test]
    fn get_console_log_returns_empty_when_empty() {
        let ctx = ServiceContext::new();
        let log = ctx.get_console_log(100).unwrap();
        assert!(log.is_empty());
    }

    #[test]
    fn get_console_log_returns_last_n_entries() {
        let ctx = ServiceContext::new();

        // Manually add entries to console log
        {
            let mut log = ctx.console_log.lock().unwrap();
            for i in 0..10 {
                log.push_back(ConsoleEntry {
                    timestamp: Utc::now(),
                    direction: ConsoleDirection::Sent,
                    content: format!("line {i}"),
                });
            }
        }

        let result = ctx.get_console_log(5).unwrap();
        assert_eq!(result.len(), 5);
        assert_eq!(result[0].content, "line 5");
        assert_eq!(result[4].content, "line 9");
    }

    #[test]
    fn console_log_syncs_raw_responses_without_duplicate_sent_entries() {
        let ctx = ServiceContext::new();
        let mut transport = MockSerialTransport::new("mock");
        transport.enqueue_response("ok");
        let mut session = GrblSession::new(Box::new(transport));
        session.connect().unwrap();
        *ctx.session.lock().unwrap() = Some(MachineSessionHandle::Grbl(session.into()));

        ctx.send_gcode_line("T10M6").unwrap();
        let first = ctx.get_console_log(100).unwrap();

        assert_eq!(
            first
                .iter()
                .filter(|entry| {
                    entry.direction == ConsoleDirection::Sent && entry.content == "T10M6"
                })
                .count(),
            1
        );
        assert!(first.iter().any(|entry| {
            entry.direction == ConsoleDirection::Received && entry.content == "ok"
        }));
        assert_eq!(ctx.get_console_log(100).unwrap(), first);
    }

    #[test]
    fn clear_console_log_clears_service_and_session_backing_logs() {
        let ctx = ServiceContext::new();
        let mut transport = MockSerialTransport::new("mock");
        transport.enqueue_response("ok");
        let mut session = GrblSession::new(Box::new(transport));
        session.connect().unwrap();
        *ctx.session.lock().unwrap() = Some(MachineSessionHandle::Grbl(session.into()));
        ctx.send_gcode_line("T10M6").unwrap();
        assert!(!ctx.get_console_log(100).unwrap().is_empty());

        ctx.clear_console_log().unwrap();

        assert!(ctx.get_console_log(100).unwrap().is_empty());
        assert!(ctx.console_log.lock().unwrap().is_empty());
        assert!(
            ctx.session
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .console_entries(100)
                .is_empty()
        );
    }

    #[test]
    fn apply_material_updates_layer_speed_power_passes() {
        let ctx = ServiceContext::with_settings(AppSettings::default());
        let mut project = Project::new("Test");
        let layer_id = project.ensure_default_layer();

        let preset = MaterialPreset {
            id: Uuid::new_v4(),
            name: "Acrylic".to_string(),
            material: "Acrylic".to_string(),
            thickness_mm: 3.0,
            operation: beambench_core::OperationType::Cut,
            speed_mm_min: 500.0,
            power_percent: 75.0,
            passes: 3,
            dpi: Some(300),
            raster_mode: None,
            line_interval_mm: None,
            scan_angle: None,
            bidirectional: None,
            overscan_mm: None,
            flood_fill: None,
            angle_passes: None,
            angle_increment_deg: None,
            pass_through: None,
            halftone_cells_per_inch: None,
            halftone_angle_deg: None,
            newsprint_angle_deg: None,
            newsprint_frequency: None,
            invert: None,
            dot_width_correction_mm: None,
            ramp_length_mm: None,
            notes: String::new(),
            category: String::new(),
            device_profile: None,
        };
        let preset_id = preset.id;
        ctx.save_material(preset).unwrap();

        ctx.apply_material(preset_id, layer_id, &mut project)
            .unwrap();

        let layer = &project.layers[0];
        assert_eq!(layer.primary_entry().speed_mm_min, 500.0);
        assert_eq!(layer.primary_entry().power_percent, 75.0);
        if let Some(ref vector_settings) = layer.primary_entry().vector_settings {
            assert_eq!(vector_settings.passes, 3);
        }
    }

    #[test]
    fn apply_material_updates_raster_angle_settings() {
        let ctx = ServiceContext::with_settings(AppSettings::default());
        let mut project = Project::new("Test");
        let layer_id = project.ensure_default_layer();
        if let Some(layer) = project.layers.iter_mut().find(|l| l.id == layer_id) {
            layer.primary_entry_mut().operation = beambench_core::OperationType::Image;
            layer.primary_entry_mut().raster_settings =
                Some(beambench_core::RasterSettings::default());
            layer.primary_entry_mut().vector_settings = None;
        }

        let preset = MaterialPreset {
            id: Uuid::new_v4(),
            name: "Diagonal Raster".to_string(),
            material: "Birch".to_string(),
            thickness_mm: 3.0,
            operation: beambench_core::OperationType::Image,
            speed_mm_min: 1200.0,
            power_percent: 55.0,
            passes: 2,
            dpi: Some(254),
            raster_mode: Some(beambench_common::RasterMode::OrderedDither),
            line_interval_mm: Some(0.12),
            scan_angle: Some(45.0),
            bidirectional: Some(false),
            overscan_mm: Some(4.0),
            flood_fill: Some(true),
            angle_passes: Some(3),
            angle_increment_deg: Some(60.0),
            pass_through: None,
            halftone_cells_per_inch: None,
            halftone_angle_deg: None,
            newsprint_angle_deg: None,
            newsprint_frequency: None,
            invert: None,
            dot_width_correction_mm: None,
            ramp_length_mm: None,
            notes: String::new(),
            category: String::new(),
            device_profile: None,
        };
        let preset_id = preset.id;
        ctx.save_material(preset).unwrap();

        ctx.apply_material(preset_id, layer_id, &mut project)
            .unwrap();

        let layer = project.layers.iter().find(|l| l.id == layer_id).unwrap();
        let raster_settings = layer.primary_entry().raster_settings.as_ref().unwrap();
        assert_eq!(
            raster_settings.mode,
            beambench_common::RasterMode::OrderedDither
        );
        assert_eq!(raster_settings.line_interval_mm, 0.12);
        assert_eq!(raster_settings.scan_angle, 45.0);
        assert!(!raster_settings.bidirectional);
        assert_eq!(raster_settings.overscan_mm, 4.0);
        assert!(raster_settings.flood_fill);
        assert_eq!(raster_settings.angle_passes, 3);
        assert_eq!(raster_settings.angle_increment_deg, 60.0);
        assert!(!raster_settings.crosshatch);
    }

    #[test]
    fn apply_material_warns_when_targeting_primary_entry_of_multi_entry_layer() {
        let ctx = ServiceContext::with_settings(AppSettings::default());
        let mut project = Project::new("Test");
        let layer_id = project.ensure_default_layer();
        let layer = project
            .layers
            .iter_mut()
            .find(|layer| layer.id == layer_id)
            .unwrap();
        layer.entries.push(beambench_core::CutEntry::new(
            beambench_core::OperationType::Line,
        ));
        let secondary_id = layer.entries[1].id;
        let secondary_speed = layer.entries[1].speed_mm_min;

        let preset = MaterialPreset {
            id: Uuid::new_v4(),
            name: "Primary Only".to_string(),
            material: "Acrylic".to_string(),
            thickness_mm: 3.0,
            operation: beambench_core::OperationType::Cut,
            speed_mm_min: 333.0,
            power_percent: 44.0,
            passes: 2,
            dpi: Some(254),
            raster_mode: None,
            line_interval_mm: None,
            scan_angle: None,
            bidirectional: None,
            overscan_mm: None,
            flood_fill: None,
            angle_passes: None,
            angle_increment_deg: None,
            pass_through: None,
            halftone_cells_per_inch: None,
            halftone_angle_deg: None,
            newsprint_angle_deg: None,
            newsprint_frequency: None,
            invert: None,
            dot_width_correction_mm: None,
            ramp_length_mm: None,
            notes: String::new(),
            category: String::new(),
            device_profile: None,
        };
        let preset_id = preset.id;
        ctx.save_material(preset).unwrap();

        let response = ctx
            .apply_material(preset_id, layer_id, &mut project)
            .unwrap();

        assert_eq!(response.applied_layer_id, layer_id);
        assert_eq!(response.targeted_entry_id, project.layers[0].entries[0].id);
        assert_eq!(response.warnings.len(), 1);
        assert_eq!(
            response.warnings[0].code,
            crate::MaterialApplyWarningCode::MultiEntryLayerTargetedPrimary
        );

        let layer = project
            .layers
            .iter()
            .find(|layer| layer.id == layer_id)
            .unwrap();
        assert_eq!(layer.entries[0].speed_mm_min, 333.0);
        assert_eq!(layer.entries[1].id, secondary_id);
        assert_eq!(layer.entries[1].speed_mm_min, secondary_speed);
    }

    #[test]
    fn apply_material_returns_error_if_preset_not_found() {
        let ctx = ServiceContext::with_settings(AppSettings::default());
        let mut project = Project::new("Test");
        let layer_id = project.ensure_default_layer();
        let nonexistent_id = Uuid::new_v4();

        let result = ctx.apply_material(nonexistent_id, layer_id, &mut project);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[test]
    fn apply_material_returns_error_if_layer_not_found() {
        let ctx = ServiceContext::with_settings(AppSettings::default());
        let mut project = Project::new("Test");

        let preset = MaterialPreset::default();
        let preset_id = preset.id;
        ctx.save_material(preset).unwrap();

        let nonexistent_layer_id = LayerId::new();
        let result = ctx.apply_material(preset_id, nonexistent_layer_id, &mut project);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    // --- Art Library Tests ---

    #[test]
    fn new_context_has_empty_art_libraries() {
        let ctx = ServiceContext::with_settings(AppSettings::default());
        let libs = ctx.art_libraries.lock().unwrap();
        assert!(libs.is_empty());
    }

    #[test]
    fn create_and_get_art_library() {
        let ctx = ServiceContext::with_settings(AppSettings::default());
        {
            let mut libs = ctx.art_libraries.lock().unwrap();
            libs.push(persist::LoadedArtLibrary {
                document: beambench_core::ArtLibraryDocument::new("Test"),
                path: std::path::PathBuf::from("/tmp/Test.bbart"),
                save_error: None,
            });
        }
        let libs = ctx.get_art_libraries().unwrap();
        assert_eq!(libs.libraries.len(), 1);
        assert_eq!(libs.libraries[0].document.name, "Test");
    }

    #[test]
    fn delete_art_library_removes_from_memory() {
        let ctx = ServiceContext::with_settings(AppSettings::default());
        {
            let mut libs = ctx.art_libraries.lock().unwrap();
            libs.push(persist::LoadedArtLibrary {
                document: beambench_core::ArtLibraryDocument::new("Lib1"),
                path: std::path::PathBuf::from("/tmp/Lib1.bbart"),
                save_error: None,
            });
            libs.push(persist::LoadedArtLibrary {
                document: beambench_core::ArtLibraryDocument::new("Lib2"),
                path: std::path::PathBuf::from("/tmp/Lib2.bbart"),
                save_error: None,
            });
        }
        {
            let mut libs = ctx.art_libraries.lock().unwrap();
            libs.retain(|l| l.document.name != "Lib1");
        }
        let libs = ctx.get_art_libraries().unwrap();
        assert_eq!(libs.libraries.len(), 1);
        assert_eq!(libs.libraries[0].document.name, "Lib2");
    }

    #[test]
    fn find_duplicate_art_library_external_item_matches_by_media_type_and_data() {
        let ctx = ServiceContext::with_settings(AppSettings::default());
        let library = persist::LoadedArtLibrary {
            document: beambench_core::ArtLibraryDocument::new("Shapes"),
            path: std::path::PathBuf::from("/tmp/Shapes.bbart"),
            save_error: None,
        };
        let library_id = library.document.library_id;
        let expected = ArtLibraryItem {
            id: Uuid::new_v4(),
            kind: ArtLibraryItemKind::ExternalFile,
            name: "Star".to_string(),
            category: "General".to_string(),
            tags: vec![],
            source_filename: "star.png".to_string(),
            media_type: "image/png".to_string(),
            data: "abc123".to_string(),
            thumbnail: None,
            created_at: Utc::now().to_rfc3339(),
        };
        let snapshot = ArtLibraryItem {
            id: Uuid::new_v4(),
            kind: ArtLibraryItemKind::SelectionSnapshot,
            name: "Selection".to_string(),
            category: "General".to_string(),
            tags: vec![],
            source_filename: "Selection".to_string(),
            media_type: "application/vnd.beambench.art-snapshot+json".to_string(),
            data: "abc123".to_string(),
            thumbnail: None,
            created_at: Utc::now().to_rfc3339(),
        };

        {
            let mut libs = ctx.art_libraries.lock().unwrap();
            let mut library = library;
            library.document.items.push(expected.clone());
            library.document.items.push(snapshot);
            libs.push(library);
        }

        let found = ctx
            .find_duplicate_art_library_external_item(library_id, "image/png", "abc123")
            .unwrap();

        assert_eq!(found, Some(expected));
        assert_eq!(
            ctx.find_duplicate_art_library_external_item(library_id, "image/jpeg", "abc123")
                .unwrap(),
            None
        );
    }

    #[test]
    fn add_art_library_item_dedupe_external_returns_existing_item_without_inserting_again() {
        let ctx = ServiceContext::with_settings(AppSettings::default());
        let library = persist::LoadedArtLibrary {
            document: beambench_core::ArtLibraryDocument::new("Shapes"),
            path: std::path::PathBuf::from("/tmp/Shapes.bbart"),
            save_error: None,
        };
        let library_id = library.document.library_id;
        {
            let mut libs = ctx.art_libraries.lock().unwrap();
            libs.push(library);
        }

        let first = ArtLibraryItem {
            id: Uuid::new_v4(),
            kind: ArtLibraryItemKind::ExternalFile,
            name: "Star".to_string(),
            category: "General".to_string(),
            tags: vec!["a".to_string()],
            source_filename: "star.png".to_string(),
            media_type: "image/png".to_string(),
            data: "abc123".to_string(),
            thumbnail: None,
            created_at: Utc::now().to_rfc3339(),
        };
        let second = ArtLibraryItem {
            id: Uuid::new_v4(),
            kind: ArtLibraryItemKind::ExternalFile,
            name: "Star renamed".to_string(),
            category: "General".to_string(),
            tags: vec!["b".to_string()],
            source_filename: "renamed.png".to_string(),
            media_type: "image/png".to_string(),
            data: "abc123".to_string(),
            thumbnail: None,
            created_at: Utc::now().to_rfc3339(),
        };

        let (inserted, duplicate_first) = ctx
            .add_art_library_item_dedupe_external(library_id, first.clone())
            .unwrap();
        let (deduped, duplicate_second) = ctx
            .add_art_library_item_dedupe_external(library_id, second)
            .unwrap();

        assert!(!duplicate_first);
        assert_eq!(inserted, first);
        assert!(duplicate_second);
        assert_eq!(deduped, first);

        let libs = ctx.get_art_libraries().unwrap();
        let loaded = libs
            .libraries
            .into_iter()
            .find(|entry| entry.document.library_id == library_id)
            .unwrap();
        assert_eq!(loaded.document.items.len(), 1);
    }
}
