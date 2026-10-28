//! Launch the actual headless executable with isolated settings. Runs on Windows
//! and Linux CI as well as macOS; no Beam Bench desktop window is involved.
use beambench_core::{AppSettings, MachineProfile};
use beambench_service::{ServiceContext, ops::profiles};
use std::{
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

struct Host(Child);
impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
#[test]
fn headless_host_uses_imported_profile_and_requires_network_authentication() {
    let directory = tempfile::Builder::new()
        .prefix("Beam Bench remote host ")
        .tempdir()
        .unwrap();
    let path = directory.path().join("laser.bbprofile");
    let profile = MachineProfile {
        name: "Stationary laser".into(),
        ..Default::default()
    };
    let ctx = ServiceContext::with_settings(AppSettings {
        machine_profiles: vec![profile.clone()],
        ..Default::default()
    });
    profiles::export_machine_profile_to_path(&ctx, profile.id, &path).unwrap();
    let exe = env!("CARGO_BIN_EXE_beambench-cli");
    let import = Command::new(exe)
        .env_remove("NEXTEST_EXECUTION_MODE")
        .args(["host", "import-profile"])
        .arg(path)
        .env("BEAMBENCH_CONFIG_DIR", directory.path())
        .output()
        .unwrap();
    assert!(
        import.status.success(),
        "{}",
        String::from_utf8_lossy(&import.stderr)
    );
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    // A targeted Windows console event must never hit cargo or the CI runner.
    #[cfg(windows)]
    let _console = windows_console::Console::ensure();
    let mut host_command = Command::new(exe);
    host_command
        .env_remove("NEXTEST_EXECUTION_MODE")
        .args(["host", "serve", "--network", "--port", &port.to_string()])
        .env("BEAMBENCH_CONFIG_DIR", directory.path())
        .env(
            "BEAMBENCH_API_TOKEN",
            "headless-test-token-at-least-32-characters",
        )
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        host_command
            .creation_flags(windows_sys::Win32::System::Threading::CREATE_NEW_PROCESS_GROUP);
    }
    let mut host = Host(host_command.spawn().unwrap());
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();
    let url = format!("http://127.0.0.1:{port}/api/v1/remote/status");
    let deadline = Instant::now() + Duration::from_secs(15);
    let response = loop {
        if let Ok(response) = client
            .get(&url)
            .bearer_auth("headless-test-token-at-least-32-characters")
            .send()
        {
            break response;
        }
        assert!(
            host.0.try_wait().unwrap().is_none(),
            "Headless host exited during startup"
        );
        assert!(Instant::now() < deadline, "Headless host did not start");
        std::thread::sleep(Duration::from_millis(50));
    };
    assert!(response.status().is_success());
    let snapshot: serde_json::Value = response.json().unwrap();
    assert_eq!(snapshot["protocol_version"], 1);
    assert_eq!(snapshot["profiles"][0]["name"], "Stationary laser");
    assert_eq!(client.get(&url).send().unwrap().status().as_u16(), 401);
    #[cfg(unix)]
    assert!(
        Command::new("kill")
            .args(["-TERM", &host.0.id().to_string()])
            .status()
            .unwrap()
            .success()
    );
    #[cfg(windows)]
    windows_console::stop_group(host.0.id());
    #[cfg(any(unix, windows))]
    {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(status) = host.0.try_wait().unwrap() {
                assert!(
                    status.success(),
                    "Host did not exit cleanly after its stop signal: {status}"
                );
                break;
            }
            assert!(
                Instant::now() < deadline,
                "Host did not handle its stop signal"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}
#[test]
fn network_host_refuses_to_start_without_authentication() {
    let directory = tempfile::tempdir().unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_beambench-cli"))
        .env_remove("NEXTEST_EXECUTION_MODE")
        .args(["host", "serve", "--network"])
        .env("BEAMBENCH_CONFIG_DIR", directory.path())
        .env_remove("BEAMBENCH_API_TOKEN")
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("BEAMBENCH_API_TOKEN"));
}

#[cfg(windows)]
mod windows_console {
    use windows_sys::Win32::System::Console::{
        AllocConsole, CTRL_BREAK_EVENT, FreeConsole, GenerateConsoleCtrlEvent,
        GetConsoleProcessList, GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
        SetStdHandle,
    };

    pub struct Console {
        allocated: bool,
    }
    impl Console {
        pub fn ensure() -> Self {
            // Hosted runners can have piped stdio and no attached console.
            // Allocate one only for this test, preserving the harness's pipes.
            let mut pid = 0;
            let allocated = unsafe { GetConsoleProcessList(&mut pid, 1) } == 0;
            if allocated {
                let handles = [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE]
                    .map(|kind| (kind, unsafe { GetStdHandle(kind) }));
                assert_ne!(
                    unsafe { AllocConsole() },
                    0,
                    "AllocConsole: {}",
                    std::io::Error::last_os_error()
                );
                for (kind, handle) in handles {
                    assert_ne!(
                        unsafe { SetStdHandle(kind, handle) },
                        0,
                        "SetStdHandle: {}",
                        std::io::Error::last_os_error()
                    );
                }
            }
            Self { allocated }
        }
    }
    impl Drop for Console {
        fn drop(&mut self) {
            if self.allocated {
                unsafe {
                    FreeConsole();
                }
            }
        }
    }
    pub fn stop_group(pid: u32) {
        assert_ne!(pid, 0, "Never broadcast a stop event to the runner console");
        // Microsoft: Ctrl+C cannot target a process group, but Ctrl+Break can.
        // https://learn.microsoft.com/en-us/windows/console/generateconsolectrlevent
        assert_ne!(
            unsafe { GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, pid) },
            0,
            "GenerateConsoleCtrlEvent: {}",
            std::io::Error::last_os_error()
        );
    }
}
