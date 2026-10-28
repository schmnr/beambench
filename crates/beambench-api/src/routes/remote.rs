//! Remote desktop ownership and portable job preparation. Socket lifetimes never
//! own a machine job. A consumed transfer can never be started a second time.
use crate::response::{ApiError, confirmation_required, map_service_error};
use axum::{
    Extension, Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Request, State},
    http::{HeaderMap, Method},
    middleware::Next,
    response::Response,
    routing::{get, post},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use beambench_core::MachineProfileId;
use beambench_service::{
    ServiceContext, ServiceError,
    ops::{
        machine,
        planning::{self, SessionJobOptions},
        profiles, project,
    },
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use uuid::Uuid;

pub const MAX_ARCHIVE_BYTES: usize = 64 * 1024 * 1024;
const IDLE_LEASE: Duration = Duration::from_secs(30);
#[derive(Default)]
struct RemoteState {
    owner: Option<(Uuid, Instant)>,
    prepared: Option<Prepared>,
}
struct Prepared {
    id: Uuid,
    options: SessionJobOptions,
    revision: String,
    profile: Value,
    session_epoch: u64,
    stop_generation: u64,
    consumed: bool,
    review: Value,
}
pub struct RemoteControl {
    ctx: Arc<ServiceContext>,
    boot_id: Uuid,
    state: Mutex<RemoteState>,
    gate: tokio::sync::Mutex<()>,
    busy: std::sync::atomic::AtomicBool,
}
struct Busy(Arc<RemoteControl>);
impl Drop for Busy {
    fn drop(&mut self) {
        self.0
            .busy
            .store(false, std::sync::atomic::Ordering::Release);
    }
}
impl RemoteControl {
    fn active(&self) -> bool {
        self.busy.load(std::sync::atomic::Ordering::Acquire) || active(&self.ctx)
    }
    fn begin(self: &Arc<Self>) -> Busy {
        self.busy.store(true, std::sync::atomic::Ordering::Release);
        Busy(self.clone())
    }
    pub fn new(ctx: Arc<ServiceContext>) -> Arc<Self> {
        Arc::new(Self {
            ctx,
            boot_id: Uuid::new_v4(),
            state: Mutex::new(RemoteState::default()),
            gate: tokio::sync::Mutex::new(()),
            busy: std::sync::atomic::AtomicBool::new(false),
        })
    }
}
fn conflict(message: &str) -> ApiError {
    map_service_error(ServiceError::conflict(message))
}
fn client(headers: &HeaderMap) -> Result<Uuid, ApiError> {
    headers
        .get("x-beambench-client")
        .and_then(|h| h.to_str().ok())
        .and_then(|h| Uuid::parse_str(h).ok())
        .ok_or_else(|| {
            map_service_error(ServiceError::invalid_input(
                "A stable x-beambench-client UUID is required",
            ))
        })
}
fn active(ctx: &ServiceContext) -> bool {
    ctx.job.lock().unwrap().is_some()
        || ctx
            .job_tick_loop_running
            .load(std::sync::atomic::Ordering::Acquire)
}

/// Serialize API writes across clients. Emergency stop always bypasses ownership
/// and the write gate so it can interrupt a slow planner or connection attempt.
pub async fn guard(
    State(remote): State<Arc<RemoteControl>>,
    request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let path = request.uri().path().to_owned();
    let caller = client(request.headers()).ok();
    if path == "/api/v1/machine/emergency-stop" {
        return Ok(next.run(request).await);
    }
    if request.method() == Method::GET {
        if let Some(caller) = caller {
            let mut state = remote.state.lock().unwrap();
            if state.owner.is_some_and(|(owner, _)| owner == caller) {
                state.owner = Some((caller, Instant::now()));
            }
        }
        return Ok(next.run(request).await);
    }
    let _gate = remote.gate.lock().await;
    {
        let mut state = remote.state.lock().unwrap();
        let is_active = remote.active();
        if !is_active
            && state
                .owner
                .is_some_and(|(_, seen)| seen.elapsed() > IDLE_LEASE)
        {
            state.owner = None;
            state.prepared = None;
        }
        if path != "/api/v1/remote/claim" {
            if remote.busy.load(std::sync::atomic::Ordering::Acquire) {
                return Err(conflict(
                    "Host preparation is still in progress. Reconnect to read its state.",
                ));
            }
            if path.starts_with("/api/v1/remote/")
                && (caller.is_none() || state.owner.map(|(id, _)| id) != caller)
            {
                return Err(conflict("Claim this host before controlling it"));
            }
            if let Some((owner, _)) = state.owner {
                if caller != Some(owner) {
                    return Err(conflict("Another desktop controls this host"));
                }
                // Remote starts must consume a prepared receipt, including after completion.
                if path == "/api/v1/jobs/start" {
                    return Err(conflict("Use the prepared remote job to start"));
                }
                state.owner = Some((owner, Instant::now()));
                if !path.starts_with("/api/v1/remote/")
                    && !matches!(
                        path.as_str(),
                        "/api/v1/jobs/pause" | "/api/v1/jobs/resume" | "/api/v1/jobs/cancel"
                    )
                {
                    state.prepared = None;
                }
            }
        }
    }
    Ok(next.run(request).await)
}

pub fn router() -> Router<Arc<ServiceContext>> {
    Router::new()
        .route("/claim", post(claim))
        .route("/connect", post(connect))
        .route("/origin", post(capture_origin))
        .route("/release", post(release))
        .route("/status", get(status))
        .route("/review", get(review))
        .route("/prepare", post(prepare))
        .route("/start", post(start))
        .route("/frame", post(frame))
        .layer(DefaultBodyLimit::max(
            MAX_ARCHIVE_BYTES * 4 / 3 + 1024 * 1024,
        ))
}
async fn claim(
    Extension(remote): Extension<Arc<RemoteControl>>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let id = client(&headers)?;
    let mut state = remote.state.lock().unwrap();
    if state.owner.is_some_and(|(owner, _)| owner != id)
        || (state.owner.is_none() && remote.active())
    {
        return Err(conflict(
            "Another client owns this host or a local job is active",
        ));
    }
    state.owner = Some((id, Instant::now()));
    Ok(Json(json!({"claimed":true, "boot_id":remote.boot_id})))
}
async fn release(
    Extension(remote): Extension<Arc<RemoteControl>>,
) -> Result<Json<Value>, ApiError> {
    if remote.active() {
        return Err(conflict(
            "A running job retains its owner. Disconnecting the desktop leaves it running.",
        ));
    }
    *remote.state.lock().unwrap() = RemoteState::default();
    Ok(Json(json!({"released":true})))
}
async fn status(
    State(ctx): State<Arc<ServiceContext>>,
    Extension(remote): Extension<Arc<RemoteControl>>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let caller = client(&headers).ok();
    tokio::task::spawn_blocking(move || snapshot(&ctx, &remote, caller))
        .await
        .map_err(|_| map_service_error(ServiceError::internal("Host snapshot task failed")))?
}
fn snapshot(
    ctx: &ServiceContext,
    remote: &RemoteControl,
    caller: Option<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let machine = machine::runtime_state(ctx).map_err(map_service_error)?;
    let (owned, transfer_id, consumed) = {
        let state = remote.state.lock().unwrap();
        (
            state.owner.map(|(id, _)| id) == caller && caller.is_some(),
            state.prepared.as_ref().map(|p| p.id),
            state.prepared.as_ref().is_some_and(|p| p.consumed),
        )
    };
    let progress = ctx
        .job
        .lock()
        .unwrap()
        .as_ref()
        .map(|j| j.progress())
        .or_else(|| ctx.last_job_progress.lock().unwrap().clone())
        .or_else(|| {
            ctx.last_terminal_job
                .lock()
                .unwrap()
                .as_ref()
                .and_then(|job| job.progress.clone())
        });
    let project_name = ctx
        .project
        .lock()
        .unwrap()
        .as_ref()
        .map(|p| p.metadata.project_name.clone());
    Ok(Json(
        json!({"protocol_version":1,"boot_id":remote.boot_id,"owned":owned,"transfer_id":transfer_id,"consumed":consumed,"busy":remote.busy.load(std::sync::atomic::Ordering::Acquire),"project_name":project_name,"machine":machine,"progress":progress,"profiles":profiles::list_profiles(ctx).map_err(map_service_error)?,"active_profile_id":profiles::get_active_profile_id(ctx).map_err(map_service_error)?}),
    ))
}
async fn capture_origin(
    State(ctx): State<Arc<ServiceContext>>,
    Extension(remote): Extension<Arc<RemoteControl>>,
) -> Result<Json<Value>, ApiError> {
    if remote.active() {
        return Err(conflict("Wait for the host job before capturing an origin"));
    }
    tokio::task::spawn_blocking(move || {
        let mut session = ctx.session.lock().unwrap();
        let session = session.as_mut().ok_or_else(|| {
            map_service_error(ServiceError::invalid_state("Connect the host laser first"))
        })?;
        let position = session
            .fresh_work_position(Duration::from_secs(1))
            .map_err(|e| map_service_error(ServiceError::machine(e)))?
            .ok_or_else(|| {
                map_service_error(ServiceError::invalid_state(
                    "The host controller does not report a work position",
                ))
            })?;
        Ok(Json(json!({"position":[position.x,position.y]})))
    })
    .await
    .map_err(|_| map_service_error(ServiceError::internal("Host origin capture failed")))?
}
#[derive(Deserialize)]
struct ConnectBody {
    port: String,
    baud_rate: u32,
    profile_id: MachineProfileId,
    #[serde(default)]
    confirm_motion: bool,
}
async fn connect(
    State(ctx): State<Arc<ServiceContext>>,
    Extension(remote): Extension<Arc<RemoteControl>>,
    Json(body): Json<ConnectBody>,
) -> Result<Json<Value>, ApiError> {
    if !body.confirm_motion {
        return Err(confirmation_required(&["confirm_motion"]));
    }
    if remote.active() {
        return Err(conflict(
            "Wait for the active host job before reconnecting the controller",
        ));
    }
    remote.state.lock().unwrap().prepared = None;
    let busy = remote.begin();
    tokio::task::spawn_blocking(move || {
        let _busy = busy;
        profiles::set_active_profile(&ctx, Some(body.profile_id)).map_err(map_service_error)?;
        let state = machine::connect_machine(
            &ctx,
            machine::ConnectMachineInput {
                target: beambench_common::machine::MachineConnectionTarget::GrblSerial {
                    port_name: body.port,
                    baud_rate: body.baud_rate,
                },
            },
        )
        .map_err(map_service_error)?;
        Ok(Json(json!({"session_state":state})))
    })
    .await
    .map_err(|_| {
        map_service_error(ServiceError::internal(
            "Host controller connection task failed",
        ))
    })?
}
#[derive(Deserialize)]
struct PrepareBody {
    archive_base64: String,
    profile_id: MachineProfileId,
    #[serde(default)]
    options: SessionJobOptions,
}
async fn prepare(
    State(ctx): State<Arc<ServiceContext>>,
    Extension(remote): Extension<Arc<RemoteControl>>,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    if remote.active() {
        return Err(conflict(
            "Wait for the host job to finish before transferring a project",
        ));
    }
    if !matches!(
        ctx.session.lock().unwrap().as_ref(),
        Some(beambench_service::runtime::MachineSessionHandle::Grbl(_))
    ) {
        return Err(map_service_error(ServiceError::invalid_state(
            "Connect a GRBL controller on the host before preparing a remote job",
        )));
    }
    // Invalidate the old receipt even if the new transfer fails validation.
    remote.state.lock().unwrap().prepared = None;
    let stop_generation = ctx
        .emergency_stop_generation
        .load(std::sync::atomic::Ordering::Acquire);
    let busy = remote.begin();
    let prepared = tokio::task::spawn_blocking(move || -> Result<_, ApiError> {
        let _busy = busy;
        let body: PrepareBody = serde_json::from_slice(&body).map_err(|_| map_service_error(ServiceError::invalid_input("Invalid remote project package")))?;
        if body.options.use_selection_origin && !body.options.cut_selected_graphics {
            return Err(map_service_error(ServiceError::invalid_input("Remote selection origin requires Cut selected graphics. Disable selection origin to transfer the whole job.")));
        }
        let bytes = STANDARD.decode(body.archive_base64).map_err(|_| map_service_error(ServiceError::invalid_input("Invalid project archive encoding")))?;
        if bytes.len() > MAX_ARCHIVE_BYTES { return Err(map_service_error(ServiceError::invalid_input("Remote project exceeds 64 MiB"))); }
        let document = beambench_project::persistence::load_project_from_bytes(&bytes).map_err(|e| map_service_error(ServiceError::invalid_input(e.to_string())))?;
        // Bind to an explicitly chosen host profile; laptop profile IDs never select a laser.
        let profile = profiles::get_profile(&ctx, profiles::ProfileLookup::Id(body.profile_id)).map_err(map_service_error)?;
        profiles::set_active_profile(&ctx, Some(body.profile_id)).map_err(map_service_error)?;
        project::install_transferred_project(&ctx, document).map_err(map_service_error)?;
        *ctx.last_job_progress.lock().unwrap() = None;
        *ctx.last_terminal_job.lock().unwrap() = None;
        let document = project::bind_active_machine_profile(&ctx).map_err(map_service_error)?;
        let preflight = machine::run_preflight_check_with_options(&ctx, &body.options).map_err(map_service_error)?;
        let preview = planning::generate_preview_with_options(&ctx, &body.options).map_err(map_service_error)?;
        let mut receipt = Prepared { id: Uuid::new_v4(), options: body.options, revision: planning::revision_hash(&document).map_err(map_service_error)?, session_epoch: ctx.session_epoch.load(std::sync::atomic::Ordering::Acquire), consumed: false, stop_generation, profile: json!(profile), review: Value::Null };
        let result = json!({"transfer_id":receipt.id,"project_name":document.metadata.project_name,"preflight":preflight,"preview":preview,"layers":document.layers,"workspace":document.workspace});
        receipt.review = result.clone();
        remote.state.lock().unwrap().prepared = Some(receipt);
        Ok(Json(result))
    }).await.map_err(|_| map_service_error(ServiceError::internal("Remote preparation task failed")))??;
    Ok(prepared)
}
async fn review(
    State(ctx): State<Arc<ServiceContext>>,
    Extension(remote): Extension<Arc<RemoteControl>>,
) -> Result<Json<Value>, ApiError> {
    // A running job retains its frozen preview. Idle review refreshes preflight
    // after a frame or a controller state change without transferring laptop edits.
    let (id, options, mut review, consumed) = {
        let state = remote.state.lock().unwrap();
        let p = state
            .prepared
            .as_ref()
            .ok_or_else(|| conflict("No remote project prepared"))?;
        (p.id, p.options.clone(), p.review.clone(), p.consumed)
    };
    if !remote.active() && !consumed {
        let preflight = tokio::task::spawn_blocking(move || {
            machine::run_preflight_check_with_options(&ctx, &options)
        })
        .await
        .map_err(|_| map_service_error(ServiceError::internal("Host review failed")))?
        .map_err(map_service_error)?;
        review["preflight"] = json!(preflight);
        let mut state = remote.state.lock().unwrap();
        if let Some(p) = state.prepared.as_mut().filter(|p| p.id == id) {
            p.review = review.clone();
        }
    }
    Ok(Json(review))
}
#[derive(Deserialize)]
struct ActionBody {
    transfer_id: Uuid,
    #[serde(default)]
    confirm_motion: bool,
    #[serde(default)]
    confirm_laser_on: bool,
    #[serde(default)]
    confirm_advisories: bool,
}
fn options(
    ctx: &ServiceContext,
    remote: &RemoteControl,
    id: Uuid,
    consume: bool,
) -> Result<(SessionJobOptions, u64), ApiError> {
    let mut state = remote.state.lock().unwrap();
    let p = state
        .prepared
        .as_mut()
        .ok_or_else(|| conflict("Transfer and review the project on this host first"))?;
    if p.id != id || p.consumed {
        return Err(conflict(
            "This transfer was replaced or already started. Reconnect to inspect the current job; prepare again for a new job.",
        ));
    }
    let document = ctx.project.lock().unwrap();
    if p.session_epoch != ctx.session_epoch.load(std::sync::atomic::Ordering::Acquire)
        || document
            .as_ref()
            .map(planning::revision_hash)
            .transpose()
            .map_err(map_service_error)?
            .as_deref()
            != Some(p.revision.as_str())
    {
        return Err(conflict(
            "The host project or controller changed. Prepare the project again.",
        ));
    }
    let profile_id = profiles::get_active_profile_id(ctx)
        .map_err(map_service_error)?
        .ok_or_else(|| conflict("Select the host profile and prepare again"))?;
    let profile = profiles::get_profile(ctx, profiles::ProfileLookup::Id(profile_id))
        .map_err(map_service_error)?;
    if json!(profile) != p.profile {
        return Err(conflict(
            "The host machine profile changed. Prepare and review again.",
        ));
    }
    if ctx
        .emergency_stop_generation
        .load(std::sync::atomic::Ordering::Acquire)
        != p.stop_generation
    {
        return Err(conflict(
            "The host was stopped after this project was prepared. Prepare and review again.",
        ));
    }
    p.consumed = consume;
    Ok((p.options.clone(), p.stop_generation))
}
async fn start(
    State(ctx): State<Arc<ServiceContext>>,
    Extension(remote): Extension<Arc<RemoteControl>>,
    Json(body): Json<ActionBody>,
) -> Result<Json<Value>, ApiError> {
    if !body.confirm_motion {
        return Err(confirmation_required(&["confirm_motion"]));
    }
    if !body.confirm_laser_on {
        return Err(confirmation_required(&["confirm_laser_on"]));
    }
    let (job_options, stop_generation) = options(&ctx, &remote, body.transfer_id, true)?;
    // Detach ownership from the request before planning or touching hardware.
    let busy = remote.begin();
    tokio::spawn(async move {
        let _busy = busy;
        let start_ctx = ctx.clone();
        let progress = tokio::task::spawn_blocking(move || {
            machine::start_job_at_generation(
                &start_ctx,
                &job_options,
                body.confirm_advisories,
                stop_generation,
            )
        })
        .await
        .map_err(|_| map_service_error(ServiceError::internal("Remote job start task failed")))?
        .map_err(map_service_error)?;
        machine::spawn_job_tick_loop(ctx);
        Ok(Json(json!({"started":true,"progress":progress})))
    })
    .await
    .map_err(|_| map_service_error(ServiceError::internal("Remote job start task failed")))?
}
async fn frame(
    State(ctx): State<Arc<ServiceContext>>,
    Extension(remote): Extension<Arc<RemoteControl>>,
    Json(body): Json<ActionBody>,
) -> Result<Json<Value>, ApiError> {
    if !body.confirm_motion {
        return Err(confirmation_required(&["confirm_motion"]));
    }
    let (options, stop_generation) = options(&ctx, &remote, body.transfer_id, false)?;
    let busy = remote.begin();
    tokio::spawn(async move {
        let _busy = busy;
        let frame_ctx = ctx.clone();
        let progress = tokio::task::spawn_blocking(move || {
            machine::frame_job_laser_off_once(
                &frame_ctx,
                &options.selected_object_ids,
                stop_generation,
            )
        })
        .await
        .map_err(|_| map_service_error(ServiceError::internal("Remote frame task failed")))?
        .map_err(map_service_error)?;
        machine::spawn_job_tick_loop(ctx);
        Ok(Json(json!({"framing":true,"progress":progress})))
    })
    .await
    .map_err(|_| map_service_error(ServiceError::internal("Remote frame task failed")))?
}

#[cfg(test)]
mod lease_tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        middleware,
    };
    use beambench_core::AppSettings;
    use tower::ServiceExt;

    #[tokio::test]
    async fn idle_owner_expires_but_active_owner_survives_an_outage() {
        let ctx = Arc::new(ServiceContext::with_settings(AppSettings::default()));
        let remote = RemoteControl::new(ctx.clone());
        let owner = Uuid::new_v4();
        remote.state.lock().unwrap().owner =
            Some((owner, Instant::now() - Duration::from_secs(60)));
        ctx.job_tick_loop_running
            .store(true, std::sync::atomic::Ordering::Release);
        let app = Router::new()
            .route("/api/v1/remote/claim", post(claim))
            .layer(middleware::from_fn_with_state(remote.clone(), guard))
            .layer(Extension(remote.clone()));
        let other = Uuid::new_v4();
        let call = || {
            Request::builder()
                .method("POST")
                .uri("/api/v1/remote/claim")
                .header("x-beambench-client", other.to_string())
                .body(Body::empty())
                .unwrap()
        };
        assert_eq!(
            app.clone().oneshot(call()).await.unwrap().status(),
            StatusCode::CONFLICT
        );
        ctx.job_tick_loop_running
            .store(false, std::sync::atomic::Ordering::Release);
        assert_eq!(app.oneshot(call()).await.unwrap().status(), StatusCode::OK);
        assert_eq!(remote.state.lock().unwrap().owner.unwrap().0, other);
    }
}
