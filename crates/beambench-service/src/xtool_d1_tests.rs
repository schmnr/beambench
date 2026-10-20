//! Native transport and product-flow tests using local sockets, never a laser.
use crate::{
    ServiceContext,
    ops::{machine, planning},
    runtime::MachineSessionHandle,
    xtool_d1_runtime::{ReqwestD1NetworkIo, XToolD1RuntimeSession},
};
use beambench_common::{
    AnchorPoint, JobState, StartFromMode,
    geometry::{Bounds, Point2D},
};
use beambench_core::{
    AppSettings, MachineProfile, ObjectData, Project, ProjectObject, WorkspaceOrigin,
    object::ShapeKind,
};
use std::{
    collections::VecDeque,
    io::{Read, Write},
    net::TcpListener,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Default)]
struct State {
    working: u8,
    events: VecDeque<String>,
    file: Vec<u8>,
    requests: Vec<String>,
    uploads: Vec<String>,
    close_status: bool,
}
struct Emulator {
    state: Arc<Mutex<State>>,
    stopped: Arc<AtomicBool>,
    threads: Vec<std::thread::JoinHandle<()>>,
    http_port: u16,
    status_port: u16,
}
impl Emulator {
    fn new() -> Self {
        let http = TcpListener::bind("127.0.0.1:0").unwrap();
        let http_port = http.local_addr().unwrap().port();
        http.set_nonblocking(true).unwrap();
        let ws = TcpListener::bind("127.0.0.1:0").unwrap();
        let status_port = ws.local_addr().unwrap().port();
        ws.set_nonblocking(true).unwrap();
        let state = Arc::new(Mutex::new(State::default()));
        let stopped = Arc::new(AtomicBool::new(false));
        let data = state.clone();
        let done = stopped.clone();
        let http_thread = std::thread::spawn(move || {
            while !done.load(Ordering::Acquire) {
                let (mut stream, _) = match http.accept() {
                    Ok(pair) => pair,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(e) => panic!("{e}"),
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut bytes = Vec::new();
                let mut chunk = [0u8; 2048];
                let header_end = loop {
                    let n = stream.read(&mut chunk).unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&chunk[..n]);
                    if let Some(i) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
                        break i + 4;
                    }
                };
                let headers = String::from_utf8_lossy(&bytes[..header_end]).to_string();
                let length = headers
                    .lines()
                    .find_map(|line| {
                        let (k, v) = line.split_once(':')?;
                        k.eq_ignore_ascii_case("content-length")
                            .then(|| v.trim().parse::<usize>().unwrap())
                    })
                    .unwrap_or(0);
                assert!(length < 1024 * 1024);
                while bytes.len() < header_end + length {
                    let n = stream.read(&mut chunk).unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&chunk[..n]);
                }
                let path = headers
                    .lines()
                    .next()
                    .unwrap()
                    .split_whitespace()
                    .nth(1)
                    .unwrap();
                let mut m = data.lock().unwrap();
                m.requests.push(path.into());
                let body = match path {
                    "/getmachinetype" => br#"{"result":"ok","type":"xTool D1Pro"}"#.to_vec(),
                    "/getlaserpowerinfo" => br#"{"result":"ok","type":0,"power":10}"#.to_vec(),
                    "/system?action=version" => {
                        br#"{"result":"ok","version":"V40.31.006.01 B2"}"#.to_vec()
                    }
                    "/peripherystatus" => br#"{"result":"ok","sdCard":1}"#.to_vec(),
                    "/system?action=get_working_sta" => {
                        format!("{{\"result\":\"ok\",\"working\":\"{}\"}}", m.working).into_bytes()
                    }
                    "/progress" => br#"{"progress":100,"working":1000,"line":0}"#.to_vec(),
                    "/cnc/data?filetype=1" => {
                        let payload =
                            String::from_utf8(bytes[header_end..header_end + length].to_vec())
                                .unwrap();
                        let file = payload
                            .split_once("\r\n\r\n")
                            .unwrap()
                            .1
                            .rsplit_once("\r\n--")
                            .unwrap()
                            .0
                            .to_string();
                        m.file = file.as_bytes().to_vec();
                        m.uploads.push(file);
                        m.events.push_back("ok:WORKING_ONLINE_READY".into());
                        br#"{"result":"ok"}"#.to_vec()
                    }
                    "/tmp.gcode" => m.file.clone(),
                    "/cnc/data?action=stop" => {
                        m.working = 0;
                        m.events.push_back("WORK_STOPPED".into());
                        m.events.push_back("ok:IDLE".into());
                        br#"{"result":"ok"}"#.to_vec()
                    }
                    "/delete?file=/tmp.gcode" => {
                        m.file.clear();
                        br#"{"result":"ok"}"#.to_vec()
                    }
                    "/ping" | "/cmd?cmd=M9+S0+N0" => br#"{"result":"ok"}"#.to_vec(),
                    _ => panic!("Unexpected native request {path}"),
                };
                drop(m);
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
                .unwrap();
                stream.write_all(&body).unwrap();
            }
        });
        let data = state.clone();
        let done = stopped.clone();
        let status_thread = std::thread::spawn(move || {
            let stream = loop {
                match ws.accept() {
                    Ok((stream, _)) => break stream,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        if done.load(Ordering::Acquire) {
                            return;
                        }
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(e) => panic!("{e}"),
                }
            };
            let mut socket = tungstenite::accept(stream).unwrap();
            socket.get_mut().set_nonblocking(true).unwrap();
            while !done.load(Ordering::Acquire) {
                let mut m = data.lock().unwrap();
                if m.close_status {
                    let _ = socket.close(None);
                    return;
                }
                let event = m.events.pop_front();
                drop(m);
                if let Some(event) = event
                    && socket
                        .send(tungstenite::Message::Text(event.into()))
                        .is_err()
                {
                    return;
                }
                match socket.read() {
                    Ok(tungstenite::Message::Ping(_)) => {
                        let _ = socket.flush();
                    }
                    Ok(_) => {}
                    Err(tungstenite::Error::Io(e))
                        if e.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(_) => return,
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        });
        Self {
            state,
            stopped,
            threads: vec![http_thread, status_thread],
            http_port,
            status_port,
        }
    }
    fn session(&self) -> XToolD1RuntimeSession {
        let io =
            ReqwestD1NetworkIo::with_status_port("127.0.0.1", self.http_port, self.status_port)
                .unwrap();
        XToolD1RuntimeSession::from_io("127.0.0.1", self.http_port, io).unwrap()
    }
    fn running(&self) {
        let mut m = self.state.lock().unwrap();
        m.working = 2;
        m.events.push_back("ok:WORKING_OFFLINE".into());
    }
    fn complete(&self) {
        let mut m = self.state.lock().unwrap();
        m.working = 0;
        m.events.push_back("ok:IDLE".into());
    }
}
impl Drop for Emulator {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);
        for t in self.threads.drain(..) {
            t.join().unwrap();
        }
    }
}
fn context(emulator: &Emulator) -> ServiceContext {
    let profile = MachineProfile {
        bed_width_mm: 430.0,
        bed_height_mm: 400.0,
        origin: WorkspaceOrigin::TopLeft,
        firmware_type: "xtool_d1".into(),
        use_g0_for_overscan: false,
        ..Default::default()
    };
    let ctx = ServiceContext::with_settings(AppSettings {
        active_profile_id: Some(profile.id),
        machine_profiles: vec![profile],
        ..Default::default()
    });
    let mut project = Project::new("D1 asymmetric frame");
    project.start_from = StartFromMode::CurrentPosition;
    project.job_origin = AnchorPoint::Center;
    project.workspace.origin = WorkspaceOrigin::TopLeft;
    let layer = project.ensure_default_layer();
    project.layers[0].entries[0].power_percent = 25.0;
    project.layers[0].entries[0].speed_mm_min = 1000.0;
    project.add_object(ProjectObject::new(
        "Rectangle",
        layer,
        Bounds::new(Point2D::new(10.0, 20.0), Point2D::new(30.0, 50.0)),
        ObjectData::Shape {
            kind: ShapeKind::Rectangle,
            width: 20.0,
            height: 30.0,
            corner_radius: 0.0,
        },
    ));
    *ctx.project.lock().unwrap() = Some(project);
    *ctx.session.lock().unwrap() = Some(MachineSessionHandle::XToolD1(emulator.session()));
    ctx
}
fn wait_state(ctx: &ServiceContext, expected: JobState) {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let progress = machine::tick_job(ctx)
            .unwrap()
            .expect("job ended before the expected state");
        assert!(
            !matches!(progress.state, JobState::Failed | JobState::Cancelled)
                || progress.state == expected,
            "{progress:?}"
        );
        if progress.state == expected {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "expected {expected:?}, got {progress:?}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}
#[test]
fn d1_native_sockets_require_completed_frame_then_upload_real_planner_output() {
    let emulator = Emulator::new();
    let ctx = context(&emulator);
    assert!(
        planning::prepare_gcode_export(&ctx, &planning::SessionJobOptions::default())
            .err()
            .unwrap()
            .message
            .contains("native files")
    );
    assert!(machine::start_job(&ctx).is_err());
    assert!(emulator.state.lock().unwrap().uploads.is_empty());
    let frame = machine::frame_job(&ctx, "rectangular", &[], true, None).unwrap();
    assert_eq!(frame.state, JobState::ReadyToRun);
    let frame_text = emulator.state.lock().unwrap().uploads[0].clone();
    assert!(frame_text.contains("G92 X10.000 Y15.000"), "{frame_text}");
    assert!(
        frame_text
            .lines()
            .filter(|line| line.starts_with("G1"))
            .flat_map(|line| line.split_whitespace())
            .filter_map(|word| word.strip_prefix('S'))
            .all(|s| s == "0")
    );
    emulator.running();
    wait_state(&ctx, JobState::Running);
    assert!(ctx.relative_frame_confirmation.lock().unwrap().is_none());
    emulator.complete();
    wait_state(&ctx, JobState::Completed);
    assert!(ctx.relative_frame_confirmation.lock().unwrap().is_some());
    let job = machine::start_job_with_options_confirming_advisories(
        &ctx,
        &planning::SessionJobOptions::default(),
        true,
    )
    .unwrap();
    assert_eq!(job.state, JobState::ReadyToRun);
    let text = emulator.state.lock().unwrap().uploads[1].clone();
    assert!(text.contains("S250"), "{text}");
    assert!(text.contains("G92 X10.000 Y15.000"), "{text}");
    machine::cancel_job(&ctx).unwrap();
    assert!(emulator.state.lock().unwrap().file.is_empty());
    assert!(ctx.relative_frame_confirmation.lock().unwrap().is_none());
}
#[test]
fn d1_status_close_cancels_pending_owned_file_and_never_confirms_frame() {
    let emulator = Emulator::new();
    let ctx = context(&emulator);
    machine::frame_job(&ctx, "rectangular", &[], false, None).unwrap();
    emulator.state.lock().unwrap().close_status = true;
    wait_state(&ctx, JobState::Failed);
    assert!(ctx.relative_frame_confirmation.lock().unwrap().is_none());
    assert!(emulator.state.lock().unwrap().file.is_empty());
    assert!(
        emulator
            .state
            .lock()
            .unwrap()
            .requests
            .iter()
            .any(|path| path == "/cmd?cmd=M9+S0+N0")
    );
}
#[test]
fn d1_independent_stop_does_not_need_the_status_socket_or_a_session() {
    let emulator = Emulator::new();
    crate::xtool_d1_runtime::send_independent_stop("127.0.0.1", emulator.http_port).unwrap();
    assert_eq!(
        emulator.state.lock().unwrap().requests,
        vec!["/cnc/data?action=stop", "/cmd?cmd=M9+S0+N0"]
    );
}
