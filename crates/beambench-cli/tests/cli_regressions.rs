//! Exercise the shipped command parser, HTTP client, files, JSON and exit codes.
//! Every API listener and settings directory is private to its test.
use beambench_common::geometry::{Bounds, Point2D};
use beambench_common::machine::{JobProgress, PreflightOutcome, PreflightReport};
use beambench_core::{
    AppSettings, Layer, MachineProfile, ObjectData, OperationType, Project, ProjectObject,
    ShapeKind,
};
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::{Command, Output, Stdio};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

struct Sandbox(tempfile::TempDir);

impl Sandbox {
    fn new() -> Self {
        Self(tempfile::tempdir().unwrap())
    }

    fn settings(&self, settings: &AppSettings) {
        std::fs::write(
            self.0.path().join("settings.json"),
            serde_json::to_vec(settings).unwrap(),
        )
        .unwrap();
    }

    fn run(&self, args: &[&str]) -> Output {
        let mut child = Command::new(env!("CARGO_BIN_EXE_beambench-cli"))
            .args(args)
            .current_dir(self.0.path())
            .env("BEAMBENCH_CONFIG_DIR", self.0.path())
            .env("BEAMBENCH_DATA_DIR", self.0.path().join("data"))
            // A local API request must bypass inherited proxy configuration.
            .env("HTTP_PROXY", "http://127.0.0.1:1")
            .env("ALL_PROXY", "http://127.0.0.1:1")
            .env("NO_PROXY", "")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        while child.try_wait().unwrap().is_none() {
            if Instant::now() >= deadline {
                child.kill().unwrap();
                let output = child.wait_with_output().unwrap();
                panic!(
                    "CLI did not finish: {args:?}: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        child.wait_with_output().unwrap()
    }

    fn project(&self) -> (Project, MachineProfile) {
        let profile = MachineProfile {
            bed_width_mm: 400.0,
            bed_height_mm: 400.0,
            s_value_max: 200,
            use_constant_power: true,
            ..Default::default()
        };
        let mut project = Project::new("CLI regression");
        project.machine_profile_id = Some(profile.id);
        let mut layer = Layer::new("Cut", OperationType::Line);
        layer.entries[0].power_percent = 50.0;
        project.add_object(ProjectObject::new(
            "rectangle",
            layer.id,
            Bounds::new(Point2D::new(10.0, 10.0), Point2D::new(30.0, 30.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 20.0,
                height: 20.0,
                corner_radius: 0.0,
            },
        ));
        project.layers.push(layer);
        self.save_project(&project);
        self.settings(&AppSettings {
            active_profile_id: Some(profile.id),
            machine_profiles: vec![profile.clone()],
            ..Default::default()
        });
        (project, profile)
    }

    fn save_project(&self, project: &Project) {
        beambench_project::save_project(project, &self.0.path().join("input.lzrproj")).unwrap();
    }
}

#[derive(Clone, Debug)]
struct Request {
    method: String,
    path: String,
    body: Value,
}
struct Reply {
    status: u16,
    body: String,
    truncate: bool,
}
fn reply(status: u16, body: Value) -> Reply {
    Reply {
        status,
        body: body.to_string(),
        truncate: false,
    }
}

#[test]
fn shipped_cli_edits_previews_exports_and_undoes_against_real_api() {
    let sandbox = Sandbox::new();
    let (project, profile) = sandbox.project();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let settings = AppSettings {
        api_port: listener.local_addr().unwrap().port(),
        active_profile_id: Some(profile.id),
        machine_profiles: vec![profile],
        ..Default::default()
    };
    sandbox.settings(&settings);
    let ctx = Arc::new(beambench_service::ServiceContext::with_settings(settings));
    *ctx.project.lock().unwrap() = Some(project.clone());
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let listener = tokio::net::TcpListener::from_std(listener).unwrap();
    let api = beambench_api::server::ApiServer::new(Default::default(), ctx.clone());
    let server = runtime.spawn(async move {
        api.run_with_listener(listener).await.unwrap();
    });
    std::fs::write(sandbox.0.path().join("height.json"), r#"{"value":3}"#).unwrap();
    json_output(
        &sandbox.run(&[
            "project",
            "edit",
            "set-material-height",
            "--input",
            "height.json",
            "--json",
        ]),
        0,
    );
    assert_eq!(
        ctx.project
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .material_height_mm,
        Some(3.0)
    );
    let preview = json_output(&sandbox.run(&["preview", "current", "--json"]), 0);
    assert!(preview["preview"].is_object());
    json_output(
        &sandbox.run(&["export", "gcode-current", "live.gcode", "--json"]),
        0,
    );
    assert!(
        std::fs::read_to_string(sandbox.0.path().join("live.gcode"))
            .unwrap()
            .contains("M5")
    );
    json_output(&sandbox.run(&["project", "undo", "--json"]), 0);
    assert_eq!(
        ctx.project
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .material_height_mm,
        None
    );
    assert_eq!(
        ctx.project.lock().unwrap().as_ref().unwrap().objects,
        project.objects
    );
    server.abort();
}

#[test]
fn current_session_start_uses_no_reconnect_and_forwards_selection_and_advisories() {
    let sandbox = Sandbox::new();
    let api = MockApi::new(&sandbox, vec![reply(200, json!({"started":true}))]);
    json_output(
        &sandbox.run(&[
            "job",
            "start",
            "--selected-ids",
            "one,two",
            "--use-selection-origin",
            "--confirm-advisories",
            "--confirm-motion",
            "--confirm-laser-on",
            "--json",
        ]),
        0,
    );
    let requests = api.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].path, "/api/v1/jobs/start");
    assert_eq!(
        requests[0].body,
        json!({"cut_selected_graphics":true,"use_selection_origin":true,"selected_object_ids":["one","two"],"confirm_motion":true,"confirm_laser_on":true,"confirm_advisories":true})
    );
}

#[test]
fn current_session_start_requires_both_hardware_confirmations_without_api_calls() {
    let sandbox = Sandbox::new();
    let api = MockApi::new(&sandbox, vec![]);
    json_output(
        &sandbox.run(&["job", "start", "--confirm-motion", "--json"]),
        4,
    );
    assert!(api.requests().is_empty());
}

#[test]
fn z_jog_and_overrides_forward_native_options() {
    let sandbox = Sandbox::new();
    let api = MockApi::new(
        &sandbox,
        (0..3)
            .map(|_| reply(200, json!({"success":true})))
            .collect(),
    );
    json_output(
        &sandbox.run(&[
            "machine",
            "jog",
            "0",
            "0",
            "--z-mm",
            "-2",
            "--confirm-motion",
            "--json",
        ]),
        0,
    );
    json_output(
        &sandbox.run(&["machine", "feed-override", "increase10", "--json"]),
        0,
    );
    json_output(
        &sandbox.run(&["machine", "power-override", "decrease1", "--json"]),
        0,
    );
    let requests = api.requests();
    assert_eq!(requests[0].body["z_mm"], -2.0);
    assert_eq!(requests[1].body["action"], "increase_10");
    assert_eq!(requests[2].body["action"], "decrease_1");
}

#[test]
fn native_cli_keeps_paths_relative_to_caller_and_confirms_only_through_flags() {
    let sandbox = Sandbox::new();
    std::fs::write(
        sandbox.0.path().join("request.json"),
        r#"{"path":"art.bbart","name":"Art"}"#,
    )
    .unwrap();
    let api = MockApi::new(&sandbox, vec![reply(200, json!({"id":"library"}))]);
    json_output(
        &sandbox.run(&[
            "art-library",
            "create-art-library",
            "--input",
            "request.json",
            "--json",
        ]),
        0,
    );
    let request = &api.requests()[0];
    assert_eq!(request.path, "/api/v1/workflows/art_library");
    assert_eq!(request.body["command"], "create_art_library");
    assert_eq!(
        request.body["path"],
        sandbox
            .0
            .path()
            .canonicalize()
            .unwrap()
            .join("art.bbart")
            .to_str()
            .unwrap()
    );
    std::fs::write(
        sandbox.0.path().join("request.json"),
        r#"{"confirm_laser_on":true}"#,
    )
    .unwrap();
    let output = sandbox.run(&[
        "quality-test",
        "quality-test-start",
        "--input",
        "request.json",
        "--json",
    ]);
    assert!(!output.status.success());
    assert_eq!(api.requests().len(), 1);
}

#[test]
fn live_preview_and_export_never_open_or_replace_the_project() {
    let sandbox = Sandbox::new();
    let api = MockApi::new(
        &sandbox,
        (0..2)
            .map(|_| reply(200, json!({"success":true})))
            .collect(),
    );
    json_output(
        &sandbox.run(&["preview", "current", "--selected-ids", "one", "--json"]),
        0,
    );
    json_output(
        &sandbox.run(&["export", "gcode-current", "out.gcode", "--json"]),
        0,
    );
    let requests = api.requests();
    assert_eq!(requests[0].path, "/api/v1/preview/generate");
    assert_eq!(requests[0].body["selected_object_ids"], json!(["one"]));
    assert_eq!(requests[1].path, "/api/v1/preview/export/gcode");
    assert_eq!(
        requests[1].body["output_path"],
        sandbox
            .0
            .path()
            .canonicalize()
            .unwrap()
            .join("out.gcode")
            .to_str()
            .unwrap()
    );
}

struct MockApi {
    requests: Arc<Mutex<Vec<Request>>>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl MockApi {
    fn new(sandbox: &Sandbox, replies: Vec<Reply>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        sandbox.settings(&AppSettings {
            api_port: listener.local_addr().unwrap().port(),
            api_localhost_only: false,
            ..Default::default()
        });
        listener.set_nonblocking(true).unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let captured = requests.clone();
        let stopped = stop.clone();
        let thread = std::thread::spawn(move || {
            let mut replies = replies.into_iter();
            while !stopped.load(Ordering::Acquire) {
                let (mut stream, _) = match listener.accept() {
                    Ok(value) => value,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(e) => panic!("mock listener: {e}"),
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut bytes = Vec::new();
                let header_end = loop {
                    let mut buffer = [0; 4096];
                    let size = stream.read(&mut buffer).unwrap();
                    assert!(size > 0, "request ended before headers");
                    bytes.extend_from_slice(&buffer[..size]);
                    if let Some(index) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                        break index + 4;
                    }
                };
                let headers = String::from_utf8(bytes[..header_end].to_vec()).unwrap();
                let content_length = headers
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().unwrap())
                    })
                    .unwrap_or(0);
                while bytes.len() < header_end + content_length {
                    let mut buffer = [0; 4096];
                    let size = stream.read(&mut buffer).unwrap();
                    assert!(size > 0, "request ended before body");
                    bytes.extend_from_slice(&buffer[..size]);
                }
                let mut line = headers.lines().next().unwrap().split_whitespace();
                captured.lock().unwrap().push(Request {
                    method: line.next().unwrap().to_string(),
                    path: line.next().unwrap().to_string(),
                    body: if content_length == 0 {
                        Value::Null
                    } else {
                        serde_json::from_slice(&bytes[header_end..header_end + content_length])
                            .unwrap()
                    },
                });
                let response = replies
                    .next()
                    .unwrap_or_else(|| reply(500, json!({"error":"Unexpected request"})));
                let length = response.body.len() + if response.truncate { 100 } else { 0 };
                write!(stream, "HTTP/1.1 {} Test\r\nContent-Type: application/json\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n{}", response.status, response.body).unwrap();
            }
        });
        Self {
            requests,
            stop,
            thread: Some(thread),
        }
    }

    fn requests(&self) -> Vec<Request> {
        self.requests.lock().unwrap().clone()
    }
}

impl Drop for MockApi {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take()
            && thread.join().is_err()
            && !std::thread::panicking()
        {
            panic!("Mock API thread failed");
        }
    }
}

fn json_output(output: &Output, code: i32) -> Value {
    assert_eq!(
        output.status.code(),
        Some(code),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("stdout must be one complete JSON document")
}

#[test]
fn feedback_report_loading_a_project_does_not_rewrite_settings() {
    let sandbox = Sandbox::new();
    sandbox.project();
    let settings_path = sandbox.0.path().join("settings.json");
    let original = std::fs::read(&settings_path).unwrap();
    let response = sandbox.run(&[
        "--json",
        "feedback",
        "save",
        "--project",
        "input.lzrproj",
        "--description",
        "CLI audit regression",
        "--output",
        "report.json",
    ]);
    json_output(&response, 0);
    assert!(sandbox.0.path().join("report.json").is_file());
    assert_eq!(std::fs::read(settings_path).unwrap(), original);
}

#[test]
fn camera_doctor_reports_live_api_despite_stale_disabled_setting() {
    let sandbox = Sandbox::new();
    let api = MockApi::new(
        &sandbox,
        vec![
            reply(200, json!({})),
            reply(200, json!({"frontend_bridge_connected":true})),
            reply(200, json!({"devices":[]})),
        ],
    );
    let settings_path = sandbox.0.path().join("settings.json");
    let mut settings: AppSettings =
        serde_json::from_slice(&std::fs::read(&settings_path).unwrap()).unwrap();
    settings.api_enabled = false;
    sandbox.settings(&settings);
    let report = json_output(&sandbox.run(&["--json", "camera", "doctor"]), 0);
    assert_eq!(report["ok"], true);
    assert_eq!(report["api"]["reachable"], true);
    assert_eq!(
        report["api"]["base_url"],
        format!("http://127.0.0.1:{}", settings.api_port)
    );
    assert_eq!(api.requests().len(), 3);
}

#[test]
fn offline_output_migrates_legacy_tool_layers_before_planning() {
    let sandbox = Sandbox::new();
    let (mut project, _) = sandbox.project();
    let object_layer = project.objects[0].layer_id;
    project
        .layers
        .iter_mut()
        .find(|layer| layer.id == object_layer)
        .unwrap()
        .color_tag
        .0 = "#DA0B3F".into();
    sandbox.save_project(&project);
    let response = json_output(
        &sandbox.run(&["--json", "export", "gcode", "input.lzrproj", "output.gcode"]),
        1,
    );
    assert!(response.to_string().contains("empty"), "{response}");
    assert!(!sandbox.0.path().join("output.gcode").exists());
}

fn preflight(outcome: PreflightOutcome) -> Value {
    serde_json::to_value(PreflightReport {
        outcome,
        checks: vec![],
        advisories: vec![],
    })
    .unwrap()
}

#[test]
fn local_api_uses_shared_config_and_bypasses_proxies() {
    let sandbox = Sandbox::new();
    let server = MockApi::new(
        &sandbox,
        vec![reply(200, json!({"project":{"name":"live"}}))],
    );
    assert_eq!(
        json_output(&sandbox.run(&["agent", "state", "--json"]), 0)["project"]["name"],
        "live"
    );
    assert_eq!(server.requests()[0].path, "/api/v1/agent/state");
}

#[test]
fn api_resolves_input_and_output_paths_from_cli_directory() {
    let sandbox = Sandbox::new();
    std::fs::write(sandbox.0.path().join("input.lzrproj"), b"mock").unwrap();
    std::fs::create_dir(sandbox.0.path().join("sub")).unwrap();
    let server = MockApi::new(
        &sandbox,
        vec![
            reply(200, json!({})),
            reply(200, json!({})),
            reply(200, json!({})),
        ],
    );
    json_output(
        &sandbox.run(&["project", "open", "sub/../input.lzrproj", "--json"]),
        0,
    );
    json_output(
        &sandbox.run(&["project", "save-as", "sub/../copy.lzrproj", "--json"]),
        0,
    );
    json_output(
        &sandbox.run(&["export", "svg", "--path", "drawing.svg", "--json"]),
        0,
    );
    let requests = server.requests();
    let root = sandbox.0.path().canonicalize().unwrap();
    for (request, filename) in requests
        .iter()
        .zip(["input.lzrproj", "copy.lzrproj", "drawing.svg"])
    {
        assert_eq!(request.body["path"], root.join(filename).to_str().unwrap());
    }
}

#[test]
fn design_apply_retries_busy_but_never_retries_stale_revision() {
    let sandbox = Sandbox::new();
    std::fs::write(
        sandbox.0.path().join("plan.json"),
        r#"{"schema_version":1}"#,
    )
    .unwrap();
    let server = MockApi::new(
        &sandbox,
        vec![
            reply(
                409,
                json!({"error":{"code":"busy","message":"In progress"}}),
            ),
            reply(200, json!({"applied":true})),
            reply(
                412,
                json!({"error":{"code":"stale_revision","message":"Changed"}}),
            ),
        ],
    );
    assert_eq!(
        json_output(
            &sandbox.run(&[
                "design",
                "apply",
                "plan.json",
                "--wait-ms",
                "1000",
                "--json"
            ]),
            0
        )["applied"],
        true
    );
    assert_eq!(
        json_output(
            &sandbox.run(&[
                "design",
                "apply",
                "plan.json",
                "--wait-ms",
                "1000",
                "--json"
            ]),
            1
        )["error"]["code"],
        "stale_revision"
    );
    assert_eq!(server.requests().len(), 3);
}

#[test]
fn truncated_success_response_is_an_error() {
    let sandbox = Sandbox::new();
    let _server = MockApi::new(
        &sandbox,
        vec![Reply {
            status: 200,
            body: "{}".into(),
            truncate: true,
        }],
    );
    let value = json_output(&sandbox.run(&["project", "save", "--json"]), 2);
    assert!(
        value["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Failed to read")
    );
}

#[test]
fn human_errors_preserve_legacy_api_error_strings() {
    let sandbox = Sandbox::new();
    let _server = MockApi::new(
        &sandbox,
        vec![reply(400, json!({"error":"Cannot send while paused"}))],
    );
    let output = sandbox.run(&["console", "send", "M5", "--confirm-raw-gcode"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("Cannot send while paused"));
}

#[test]
fn profile_update_uses_live_profile_and_sends_only_requested_fields() {
    let sandbox = Sandbox::new();
    let profile = MachineProfile {
        name: "Live machine".into(),
        ..Default::default()
    };
    let server = MockApi::new(
        &sandbox,
        vec![
            reply(
                200,
                json!({"profiles":[profile],"active_profile_id":profile.id}),
            ),
            reply(200, json!(profile)),
            reply(200, json!(profile)),
        ],
    );
    let before = std::fs::read(sandbox.0.path().join("settings.json")).unwrap();
    json_output(
        &sandbox.run(&[
            "profile",
            "update",
            "Live machine",
            "--max-speed-mm-min",
            "2500",
            "--homing-enabled",
            "false",
            "--json",
        ]),
        0,
    );
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[2].method, "PATCH");
    assert_eq!(
        requests[2].body,
        json!({"max_speed_mm_min":2500.0,"homing_enabled":false})
    );
    assert_eq!(
        before,
        std::fs::read(sandbox.0.path().join("settings.json")).unwrap()
    );
}

#[test]
fn job_run_does_not_reuse_an_unverified_machine_connection() {
    let sandbox = Sandbox::new();
    let server = MockApi::new(
        &sandbox,
        vec![reply(
            409,
            json!({"error":{"code":"conflict","message":"Already connected. Disconnect first."}}),
        )],
    );
    json_output(
        &sandbox.run(&[
            "job",
            "run",
            "input.lzrproj",
            "--port",
            "other-machine",
            "--confirm-motion",
            "--confirm-laser-on",
            "--json",
        ]),
        1,
    );
    let requests = server.requests();
    assert_eq!(
        requests.len(),
        1,
        "must not open a project or start on the other machine"
    );
    assert_eq!(requests[0].body["port"], "other-machine");
}

#[test]
fn failed_preflight_has_nonzero_exit_and_one_json_report() {
    let sandbox = Sandbox::new();
    std::fs::write(sandbox.0.path().join("input.lzrproj"), b"mock").unwrap();
    let server = MockApi::new(
        &sandbox,
        vec![
            reply(200, json!({})),
            reply(200, preflight(PreflightOutcome::Fail)),
        ],
    );
    assert_eq!(
        json_output(
            &sandbox.run(&["job", "preflight", "input.lzrproj", "--json"]),
            1
        )["outcome"],
        "fail"
    );
    assert_eq!(server.requests().len(), 2);
}

#[test]
fn job_run_preflight_failures_and_warnings_preserve_report() {
    for outcome in [PreflightOutcome::Fail, PreflightOutcome::PassWithWarnings] {
        let sandbox = Sandbox::new();
        std::fs::write(sandbox.0.path().join("input.lzrproj"), b"mock").unwrap();
        let expected = preflight(outcome);
        let server = MockApi::new(
            &sandbox,
            vec![
                reply(200, json!({})),
                reply(200, json!({})),
                reply(200, expected.clone()),
            ],
        );
        assert_eq!(
            json_output(
                &sandbox.run(&[
                    "job",
                    "run",
                    "input.lzrproj",
                    "--port",
                    "mock",
                    "--confirm-motion",
                    "--confirm-laser-on",
                    "--json"
                ]),
                1
            ),
            expected
        );
        assert_eq!(server.requests().len(), 3, "preflight must block start");
    }
}

#[test]
fn job_run_reports_a_job_that_disappears_before_first_poll() {
    let sandbox = Sandbox::new();
    std::fs::write(sandbox.0.path().join("input.lzrproj"), b"mock").unwrap();
    let server = MockApi::new(
        &sandbox,
        vec![
            reply(200, json!({})),
            reply(200, json!({})),
            reply(200, preflight(PreflightOutcome::Pass)),
            reply(200, json!({"started":true})),
            reply(200, json!(JobProgress::default())),
        ],
    );
    let value = json_output(
        &sandbox.run(&[
            "job",
            "run",
            "input.lzrproj",
            "--port",
            "mock",
            "--confirm-motion",
            "--confirm-laser-on",
            "--json",
        ]),
        1,
    );
    assert!(
        value["error"]["message"]
            .as_str()
            .unwrap()
            .contains("stopped before completion")
    );
    assert_eq!(server.requests().len(), 5);
}

#[test]
fn frame_without_confirmation_does_not_open_the_input_project() {
    let sandbox = Sandbox::new();
    let server = MockApi::new(&sandbox, vec![]);
    assert_eq!(
        json_output(
            &sandbox.run(&["job", "frame", "--input", "missing.lzrproj", "--json"]),
            4
        )["error_code"],
        "CONFIRMATION_REQUIRED"
    );
    assert!(server.requests().is_empty());
}

#[test]
fn macro_passes_confirmation_flags_to_api() {
    let sandbox = Sandbox::new();
    let server = MockApi::new(&sandbox, vec![reply(200, json!({"commands_sent":1}))]);
    json_output(
        &sandbox.run(&[
            "macro",
            "run",
            "test",
            "--confirm-raw-gcode",
            "--confirm-laser-on",
            "--json",
        ]),
        0,
    );
    assert_eq!(
        server.requests()[0].body,
        json!({"confirm_raw_gcode":true,"confirm_laser_on":true})
    );
}

#[test]
fn offline_export_uses_saved_profile_power_scaling_and_mode() {
    let sandbox = Sandbox::new();
    sandbox.project();
    json_output(
        &sandbox.run(&["export", "gcode", "input.lzrproj", "out.gcode", "--json"]),
        0,
    );
    let gcode = std::fs::read_to_string(sandbox.0.path().join("out.gcode")).unwrap();
    assert!(
        gcode.contains("M3"),
        "configured constant power missing: {gcode}"
    );
    assert!(
        gcode.split_whitespace().any(|word| word == "S100"),
        "50% of S200 missing: {gcode}"
    );
    assert!(
        !gcode.contains("S500"),
        "default S1000 scaling must not leak into output"
    );
}

#[test]
fn offline_output_rejects_runtime_dependent_or_missing_profile_jobs() {
    for mode in [
        beambench_common::StartFromMode::CurrentPosition,
        beambench_common::StartFromMode::UserOrigin,
    ] {
        let sandbox = Sandbox::new();
        let (mut project, _) = sandbox.project();
        project.start_from = mode;
        project.user_origin = None;
        sandbox.save_project(&project);
        std::fs::write(sandbox.0.path().join("out.gcode"), "original").unwrap();
        json_output(
            &sandbox.run(&["export", "gcode", "input.lzrproj", "out.gcode", "--json"]),
            1,
        );
        assert_eq!(
            std::fs::read_to_string(sandbox.0.path().join("out.gcode")).unwrap(),
            "original"
        );
        json_output(
            &sandbox.run(&["job", "dry-run", "input.lzrproj", "--json"]),
            1,
        );
        json_output(
            &sandbox.run(&["preview", "generate", "input.lzrproj", "--json"]),
            1,
        );
    }
    let sandbox = Sandbox::new();
    sandbox.project();
    sandbox.settings(&AppSettings::default());
    let value = json_output(
        &sandbox.run(&["export", "gcode", "input.lzrproj", "out.gcode", "--json"]),
        1,
    );
    assert!(
        value["error"]["message"]
            .as_str()
            .unwrap()
            .contains("not configured")
    );
    assert!(!sandbox.0.path().join("out.gcode").exists());
}

#[test]
fn offline_export_rejects_empty_and_out_of_machine_bounds_output() {
    for empty in [true, false] {
        let sandbox = Sandbox::new();
        let (mut project, mut profile) = sandbox.project();
        if empty {
            project.objects.clear();
            sandbox.save_project(&project);
        } else {
            profile.bed_width_mm = 15.0;
            sandbox.settings(&AppSettings {
                active_profile_id: Some(profile.id),
                machine_profiles: vec![profile],
                ..Default::default()
            });
        }
        std::fs::write(sandbox.0.path().join("out.gcode"), "original").unwrap();
        json_output(
            &sandbox.run(&["export", "gcode", "input.lzrproj", "out.gcode", "--json"]),
            1,
        );
        assert_eq!(
            std::fs::read_to_string(sandbox.0.path().join("out.gcode")).unwrap(),
            "original"
        );
    }
}

#[test]
fn export_cannot_overwrite_its_input_project() {
    let sandbox = Sandbox::new();
    sandbox.project();
    let original = std::fs::read(sandbox.0.path().join("input.lzrproj")).unwrap();
    json_output(
        &sandbox.run(&[
            "export",
            "gcode",
            "input.lzrproj",
            "./input.lzrproj",
            "--json",
        ]),
        1,
    );
    assert_eq!(
        original,
        std::fs::read(sandbox.0.path().join("input.lzrproj")).unwrap()
    );
}

#[test]
fn exports_without_paths_write_file_contents_to_stdout() {
    let sandbox = Sandbox::new();
    let _server = MockApi::new(
        &sandbox,
        vec![
            reply(200, json!({"format":"svg","content":"<svg/>"})),
            reply(200, json!({"format":"pdf","content_base64":"JVBERg=="})),
        ],
    );
    let svg = sandbox.run(&["export", "svg"]);
    assert!(svg.status.success());
    assert_eq!(svg.stdout, b"<svg/>");
    let pdf = sandbox.run(&["export", "pdf"]);
    assert!(pdf.status.success());
    assert_eq!(pdf.stdout, b"%PDF");
}

#[test]
fn incomplete_masked_image_plan_cannot_be_exported_or_reported_as_a_dry_run() {
    use beambench_core::object::{ImageMaskPolarity, ImageMaskRef};
    use beambench_core::{Asset, AssetMediaType};
    let sandbox = Sandbox::new();
    let (mut project, _) = sandbox.project();
    let mask_id = project.objects[0].id;
    project.objects[0].data = ObjectData::VectorPath {
        path_data: "M10 10 L30 30".into(),
        closed: false,
        ruler_guide_axis: None,
    };
    let png = vec![
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 10, 0, 0, 0, 10, 8,
        0, 0, 0, 0, 168, 89, 144, 97, 0, 0, 0, 12, 73, 68, 65, 84, 120, 156, 99, 96, 160, 39, 0, 0,
        0, 110, 0, 1, 72, 93, 122, 99, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ];
    let asset = Asset::new(
        "black.png",
        AssetMediaType::Png,
        png.len() as u64,
        Some(10),
        Some(10),
    );
    let asset_key = asset.id.to_string();
    project.add_asset(asset, png);
    let layer = Layer::new("Image", OperationType::Image);
    project.add_object(ProjectObject::new(
        "image",
        layer.id,
        Bounds::new(Point2D::new(10.0, 10.0), Point2D::new(30.0, 30.0)),
        ObjectData::RasterImage {
            asset_key,
            original_width_px: 10,
            original_height_px: 10,
            adjustments: None,
            masks: vec![ImageMaskRef {
                object_id: mask_id,
                polarity: ImageMaskPolarity::KeepInside,
            }],
        },
    ));
    project.layers.push(layer);
    sandbox.save_project(&project);
    let plan = beambench_planner::build_plan(&project).unwrap();
    assert!(
        !plan.segments.is_empty(),
        "fixture must include successful output too"
    );
    assert!(
        !plan.failed_entries.is_empty(),
        "fixture must expose a partial plan"
    );
    std::fs::write(sandbox.0.path().join("out.gcode"), "original").unwrap();
    for args in [
        vec!["export", "gcode", "input.lzrproj", "out.gcode", "--json"],
        vec!["job", "dry-run", "input.lzrproj", "--json"],
        vec!["preview", "stats", "input.lzrproj", "--json"],
    ] {
        let error = json_output(&sandbox.run(&args), 1);
        assert!(
            error["error"]["message"]
                .as_str()
                .unwrap()
                .contains("failed planning")
        );
    }
    assert_eq!(
        std::fs::read_to_string(sandbox.0.path().join("out.gcode")).unwrap(),
        "original"
    );
}
