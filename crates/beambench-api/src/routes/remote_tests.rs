use axum::Router;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use beambench_common::{Bounds, Point2D};
use beambench_core::{AppSettings, MachineProfile, ObjectData, Project, ProjectObject, ShapeKind};
use beambench_grbl::GrblSession;
use beambench_serial::{SerialError, SerialTransport};
use beambench_service::ServiceContext;
use beambench_service::runtime::MachineSessionHandle;
use http_body_util::BodyExt;
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tower::ServiceExt;
use uuid::Uuid;

const IDLE: &str = "<Idle|MPos:0.000,0.000,0.000|WCO:0.000,0.000,0.000|FS:0,0>";
#[derive(Default)]
struct Controller {
    rx: VecDeque<String>,
    lines: Vec<String>,
    held_acks: usize,
    release: bool,
    open: bool,
}
struct Transport(Arc<Mutex<Controller>>);
impl SerialTransport for Transport {
    fn open(&mut self) -> Result<(), SerialError> {
        self.0.lock().unwrap().open = true;
        Ok(())
    }
    fn close(&mut self) -> Result<(), SerialError> {
        self.0.lock().unwrap().open = false;
        Ok(())
    }
    fn is_open(&self) -> bool {
        self.0.lock().unwrap().open
    }
    fn write_bytes(&mut self, data: &[u8]) -> Result<usize, SerialError> {
        if data.contains(&b'?') {
            self.0.lock().unwrap().rx.push_back(IDLE.into());
        }
        Ok(data.len())
    }
    fn write_line(&mut self, line: &str) -> Result<(), SerialError> {
        let mut c = self.0.lock().unwrap();
        c.lines.push(line.into());
        if line == "$$" {
            for s in ["$30=1000", "$32=1", "$130=400", "$131=400"] {
                c.rx.push_back(s.into());
            }
        }
        if !c.release && (line.starts_with("G1 ") || c.held_acks > 0) {
            c.held_acks += 1;
        } else {
            c.rx.push_back("ok".into());
        }
        Ok(())
    }
    fn read_available(&mut self) -> Result<Vec<u8>, SerialError> {
        Ok(vec![])
    }
    fn read_line(&mut self) -> Result<Option<String>, SerialError> {
        Ok(self.0.lock().unwrap().rx.pop_front())
    }
    fn flush(&mut self) -> Result<(), SerialError> {
        Ok(())
    }
    fn port_name(&self) -> &str {
        "remote-test"
    }
}
fn fixture() -> (
    Arc<ServiceContext>,
    MachineProfile,
    Project,
    Arc<Mutex<Controller>>,
) {
    let profile = MachineProfile {
        bed_width_mm: 400.0,
        bed_height_mm: 400.0,
        ..Default::default()
    };
    let ctx = Arc::new(ServiceContext::with_settings(AppSettings {
        active_profile_id: Some(profile.id),
        machine_profiles: vec![profile.clone()],
        ..Default::default()
    }));
    let controller = Arc::new(Mutex::new(Controller::default()));
    controller
        .lock()
        .unwrap()
        .rx
        .extend(["Grbl 1.1h".into(), IDLE.into()]);
    let mut session = GrblSession::new(Box::new(Transport(controller.clone())));
    session.connect().unwrap();
    session.poll().unwrap();
    session.mark_ready().unwrap();
    *ctx.session.lock().unwrap() = Some(MachineSessionHandle::Grbl(session.into()));
    ctx.machine_coordinates_valid
        .store(true, std::sync::atomic::Ordering::Release);
    let mut project = Project::new("Laptop artwork");
    let layer = project.ensure_default_layer();
    project.add_object(ProjectObject::new(
        "Rectangle",
        layer,
        Bounds::new(Point2D::new(10.0, 10.0), Point2D::new(30.0, 30.0)),
        ObjectData::Shape {
            kind: ShapeKind::Rectangle,
            width: 20.0,
            height: 20.0,
            corner_radius: 0.0,
        },
    ));
    (ctx, profile, project, controller)
}
async fn call(app: &Router, id: Uuid, path: &str, body: Option<Value>) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(if body.is_some() { "POST" } else { "GET" })
                .uri(format!("/api/v1/{path}"))
                .header("content-type", "application/json")
                .header("x-beambench-client", id.to_string())
                .body(Body::from(body.map(|b| b.to_string()).unwrap_or_default()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}
async fn prepare_fixture(
    app: &Router,
    id: Uuid,
    profile: &MachineProfile,
    project: &Project,
) -> Value {
    assert_eq!(
        call(app, id, "remote/claim", Some(json!({}))).await.0,
        StatusCode::OK
    );
    let (status, value) = call(app,id,"remote/prepare",Some(json!({"archive_base64":STANDARD.encode(beambench_project::save_project_to_bytes(project).unwrap()),"profile_id":profile.id}))).await;
    assert_eq!(status, StatusCode::OK, "{value}");
    value
}
// Each call drops its HTTP client and socket pool before returning. The host
// must stream independently across that actual network disconnect.
async fn http(base: &str, id: Uuid, path: &str, body: Option<Value>) -> (StatusCode, Value) {
    let client = reqwest::Client::new();
    let mut request = client
        .request(
            if body.is_some() {
                reqwest::Method::POST
            } else {
                reqwest::Method::GET
            },
            format!("{base}/api/v1/{path}"),
        )
        .bearer_auth("remote-test-token-at-least-32-bytes")
        .header("x-beambench-client", id.to_string());
    if let Some(body) = body {
        request = request.json(&body);
    }
    let response = request.send().await.unwrap();
    let status = response.status();
    let value = response.json().await.unwrap();
    (status, value)
}
#[tokio::test(flavor = "multi_thread")]
async fn laptop_disconnect_does_not_own_streaming_and_receipt_cannot_replay() {
    let (ctx, profile, project, controller) = fixture();
    let config = crate::ApiConfig {
        localhost_only: false,
        bearer_token: Some("remote-test-token-at-least-32-bytes".into()),
        ..Default::default()
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let app = crate::ApiServer::new(config, ctx.clone()).router();
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let id = Uuid::new_v4();
    assert_eq!(
        http(&base, id, "remote/claim", Some(json!({}))).await.0,
        StatusCode::OK
    );
    let (status,receipt)=http(&base,id,"remote/prepare",Some(json!({"archive_base64":STANDARD.encode(beambench_project::save_project_to_bytes(&project).unwrap()),"profile_id":profile.id}))).await;
    assert_eq!(status, StatusCode::OK, "{receipt}");
    assert_ne!(receipt["preflight"]["outcome"], "fail", "{receipt}");
    let start = json!({"transfer_id":receipt["transfer_id"],"confirm_motion":true,"confirm_laser_on":true,"confirm_advisories":true});
    let (status, result) = http(&base, id, "remote/start", Some(start.clone())).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    // No client requests drive progress. Hold the controller's acknowledgements
    // while the desktop is absent, then release them without reconnecting.
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(ctx.job.lock().unwrap().is_some());
    assert!(controller.lock().unwrap().held_acks > 0);
    let other = Uuid::new_v4();
    assert_eq!(
        http(&base, other, "remote/claim", Some(json!({}))).await.0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        http(&base, other, "projects/", Some(json!({"name":"conflict"})))
            .await
            .0,
        StatusCode::CONFLICT
    );
    {
        let mut c = controller.lock().unwrap();
        c.release = true;
        for _ in 0..c.held_acks {
            c.rx.push_back("ok".into());
        }
        c.held_acks = 0;
    }
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if ctx.job.lock().unwrap().is_none() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap();
    let (_, snapshot) = http(&base, id, "remote/status", None).await;
    assert_eq!(snapshot["progress"]["state"], "completed", "{snapshot}");
    assert_eq!(snapshot["transfer_id"], receipt["transfer_id"]);
    let before = controller.lock().unwrap().lines.len();
    assert_eq!(
        http(&base, id, "remote/start", Some(start)).await.0,
        StatusCode::CONFLICT
    );
    assert_eq!(controller.lock().unwrap().lines.len(), before);
    server.abort();
}
#[tokio::test]
async fn transfer_validates_archive_profile_assets_and_confirmations() {
    let (ctx, profile, mut project, _) = fixture();
    let app = super::build_router(ctx.clone());
    let id = Uuid::new_v4();
    // Embedded, unreferenced image assets must survive alongside the vectors.
    let asset = project
        .add_asset(
            beambench_core::Asset::new(
                "test.png",
                beambench_core::AssetMediaType::Png,
                3,
                None,
                None,
            ),
            vec![1, 2, 3],
        )
        .id;
    let receipt = prepare_fixture(&app, id, &profile, &project).await;
    assert_eq!(
        ctx.project
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .get_asset_data(asset)
            .unwrap(),
        &[1, 2, 3]
    );
    assert_eq!(
        call(
            &app,
            id,
            "remote/start",
            Some(json!({"transfer_id":receipt["transfer_id"]}))
        )
        .await
        .0,
        StatusCode::PRECONDITION_REQUIRED
    );
    assert_eq!(
        call(
            &app,
            id,
            "remote/prepare",
            Some(json!({"archive_base64":"not a zip","profile_id":profile.id}))
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(call(&app,id,"remote/start",Some(json!({"transfer_id":receipt["transfer_id"],"confirm_motion":true,"confirm_laser_on":true}))).await.0,StatusCode::CONFLICT);
}
#[tokio::test]
async fn emergency_stop_bypasses_another_clients_lease() {
    let (ctx, _, _, controller) = fixture();
    let app = super::build_router(ctx);
    let owner = Uuid::new_v4();
    call(&app, owner, "remote/claim", Some(json!({}))).await;
    let (status, value) = call(
        &app,
        Uuid::new_v4(),
        "machine/emergency-stop",
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{value}");
    assert!(!controller.lock().unwrap().lines.is_empty());
}

#[tokio::test]
async fn changing_the_host_profile_invalidates_the_reviewed_receipt() {
    let (ctx, profile, project, _) = fixture();
    let app = super::build_router(ctx.clone());
    let id = Uuid::new_v4();
    let receipt = prepare_fixture(&app, id, &profile, &project).await;
    ctx.settings.lock().unwrap().machine_profiles[0].s_value_max = 255;
    let (status,value)=call(&app,id,"remote/start",Some(json!({"transfer_id":receipt["transfer_id"],"confirm_motion":true,"confirm_laser_on":true,"confirm_advisories":true}))).await;
    assert_eq!(status, StatusCode::CONFLICT, "{value}");
    assert!(ctx.job.lock().unwrap().is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn remote_frame_is_laser_off_and_single_pass_despite_local_profile_preferences() {
    let (ctx, profile, project, controller) = fixture();
    controller.lock().unwrap().release = true;
    {
        let mut settings = ctx.settings.lock().unwrap();
        let profile = &mut settings.machine_profiles[0];
        profile.laser_on_when_framing = true;
        profile.frame_continuously = true;
        profile.job_header_gcode = "M3 S1000".into();
        profile.job_footer_gcode = "M3 S1000".into();
    }
    let app = super::build_router(ctx.clone());
    let id = Uuid::new_v4();
    let receipt = prepare_fixture(&app, id, &profile, &project).await;
    let (status, value) = call(
        &app,
        id,
        "remote/frame",
        Some(json!({"transfer_id":receipt["transfer_id"],"confirm_motion":true})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{value}");
    tokio::time::timeout(Duration::from_secs(5), async {
        while ctx
            .job_tick_loop_running
            .load(std::sync::atomic::Ordering::Acquire)
        {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    let lines = controller.lock().unwrap().lines.clone();
    assert!(lines.iter().any(|line| line.starts_with("G1 ")));
    for line in &lines {
        assert!(
            !line.split_whitespace().any(|word| word
                .strip_prefix('S')
                .and_then(|s| s.parse::<f64>().ok())
                .is_some_and(|s| s > 0.0)),
            "Laser-on framing command: {line}"
        );
    }
    assert!(ctx.job.lock().unwrap().is_none());
}

#[tokio::test]
async fn stop_invalidates_prepared_motion_including_delayed_start_and_frame_requests() {
    let (ctx, profile, project, controller) = fixture();
    let app = super::build_router(ctx.clone());
    let id = Uuid::new_v4();
    let receipt = prepare_fixture(&app, id, &profile, &project).await;
    ctx.emergency_stop_generation
        .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
    let before = controller.lock().unwrap().lines.len();
    for path in ["remote/start", "remote/frame"] {
        let (status,value)=call(&app,id,path,Some(json!({"transfer_id":receipt["transfer_id"],"confirm_motion":true,"confirm_laser_on":true,"confirm_advisories":true}))).await;
        assert_eq!(status, StatusCode::CONFLICT, "{value}");
    }
    assert_eq!(controller.lock().unwrap().lines.len(), before);
}
