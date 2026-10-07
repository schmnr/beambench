//! Tauri managed state bridge.
//! The application state is provided by `beambench_service::ServiceContext`,
//! wrapped in `Arc` for shared ownership across Tauri command handlers.

use std::sync::{Arc, Mutex};

use beambench_api::{ApiConfig, ApiServer};
use beambench_core::AppSettings;
use beambench_service::ServiceContext;

/// Set once the user has resolved the unsaved-changes prompt (saved or chose
/// to discard). The window CloseRequested handler allows the close to proceed
/// only when this flag is set or the project is clean.
#[derive(Default)]
pub struct CloseConfirmed(pub std::sync::atomic::AtomicBool);

/// An asynchronous controller stop must finish before window close is accepted.
#[derive(Default)]
pub struct CloseShutdown {
    pub pending: std::sync::atomic::AtomicBool,
    pub ready: std::sync::atomic::AtomicBool,
}

/// Set by `mark_frontend_ready` once React has mounted inside the webview.
/// While unset, the webview may be dead (a too-old system WebKit cannot run
/// the bundled JS), so menu events emitted to it go nowhere: the startup
/// watchdog uses this to show a native explanation dialog, and the Quit menu
/// item falls back to a native exit.
#[derive(Default)]
pub struct FrontendReady(pub std::sync::atomic::AtomicBool);

struct RunningApiServer {
    config: ApiConfig,
    handle: tauri::async_runtime::JoinHandle<()>,
}

#[derive(Clone, Default)]
pub struct ApiRuntime {
    server: Arc<Mutex<Option<RunningApiServer>>>,
}

impl ApiRuntime {
    pub fn sync_from_settings(
        &self,
        ctx: Arc<ServiceContext>,
        settings: &AppSettings,
    ) -> Result<(), String> {
        let desired = settings
            .api_enabled
            .then(|| ApiConfig::from_settings(settings));

        // Reject invalid authentication settings before disrupting a working API.
        if let Some(config) = desired.as_ref() {
            ApiServer::new(config.clone(), ctx.clone())
                .validate_config()
                .map_err(|e| format!("Invalid API configuration: {e}"))?;
        }

        let mut guard = self
            .server
            .lock()
            .map_err(|e| format!("Failed to lock API runtime: {e}"))?;

        if let Some(running) = guard.as_ref()
            && desired.as_ref() == Some(&running.config)
        {
            return Ok(());
        }

        let Some(config) = desired else {
            if let Some(running) = guard.take() {
                running.handle.abort();
                tracing::info!("API server stopped");
            }
            return Ok(());
        };

        // Bind the new address before stopping the old server, so a port that
        // is taken leaves the working API running.
        let listener = match bind_api_listener(&config, &ctx) {
            Ok(listener) => listener,
            Err(first_error) => {
                // Only the same port can be held by the running server; any
                // other bind failure leaves it untouched.
                let overlaps = guard
                    .as_ref()
                    .is_some_and(|running| running.config.port == config.port);
                let Some(previous) = guard.take_if(|_| overlaps) else {
                    return Err(format!("Failed to bind API listener: {first_error}"));
                };
                // The new address may overlap the old one (same port, other
                // interface): free it and try again, restoring the old server
                // if the new one still cannot start.
                previous.handle.abort();
                match rebind_after_release(&config, &ctx) {
                    Ok(listener) => listener,
                    Err(error) => {
                        let restored = rebind_after_release(&previous.config, &ctx)
                            .map(|listener| {
                                *guard = Some(spawn_api_server(
                                    previous.config.clone(),
                                    ctx.clone(),
                                    listener,
                                ));
                            })
                            .is_ok();
                        return Err(if restored {
                            format!(
                                "Failed to bind API listener: {error}. The previous API settings are still active."
                            )
                        } else {
                            format!("Failed to bind API listener: {error}. The API is stopped.")
                        });
                    }
                }
            }
        };
        if let Some(running) = guard.take() {
            running.handle.abort();
            tracing::info!("API server stopped");
        }
        *guard = Some(spawn_api_server(config, ctx, listener));
        tracing::info!("API server started");

        Ok(())
    }
}

fn bind_api_listener(
    config: &ApiConfig,
    ctx: &Arc<ServiceContext>,
) -> std::io::Result<std::net::TcpListener> {
    ApiServer::new(config.clone(), ctx.clone()).bind_std_listener()
}

/// Bind after aborting a server on the same address. The aborted task drops
/// its socket on a runtime worker, so retry briefly until the address is free.
fn rebind_after_release(
    config: &ApiConfig,
    ctx: &Arc<ServiceContext>,
) -> std::io::Result<std::net::TcpListener> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
    loop {
        match bind_api_listener(config, ctx) {
            Err(error)
                if error.kind() == std::io::ErrorKind::AddrInUse
                    && std::time::Instant::now() < deadline =>
            {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            result => return result,
        }
    }
}

fn spawn_api_server(
    config: ApiConfig,
    ctx: Arc<ServiceContext>,
    std_listener: std::net::TcpListener,
) -> RunningApiServer {
    let server = ApiServer::new(config.clone(), ctx);
    let task_config = config.clone();
    let handle = tauri::async_runtime::spawn(async move {
        let listener = match tokio::net::TcpListener::from_std(std_listener) {
            Ok(listener) => listener,
            Err(err) => {
                tracing::warn!(
                    port = task_config.port,
                    localhost_only = task_config.localhost_only,
                    error = %err,
                    "API server failed to adopt listener"
                );
                return;
            }
        };
        if let Err(err) = server.run_with_listener(listener).await {
            tracing::warn!(
                port = task_config.port,
                localhost_only = task_config.localhost_only,
                error = %err,
                "API server exited"
            );
        }
    });
    RunningApiServer { config, handle }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_context_wraps_all_state() {
        let svc = Arc::new(ServiceContext::new());
        assert!(svc.project.lock().unwrap().is_none());
        assert!(svc.settings.lock().unwrap().autosave_enabled);
    }

    #[test]
    fn plan_cache_starts_empty() {
        let svc = Arc::new(ServiceContext::new());
        assert!(svc.plan_cache.lock().unwrap().is_none());
    }

    #[test]
    fn session_starts_empty() {
        let svc = Arc::new(ServiceContext::new());
        assert!(svc.session.lock().unwrap().is_none());
        assert!(svc.job.lock().unwrap().is_none());
    }

    #[test]
    fn api_runtime_starts_empty() {
        let runtime = ApiRuntime::default();
        assert!(runtime.server.lock().unwrap().is_none());
    }
    #[test]
    fn failed_network_enable_preserves_working_loopback_api() {
        const TEST: &str = "state::tests::failed_network_enable_preserves_working_loopback_api";
        if std::env::var("BEAMBENCH_ROLLBACK_TEST_CHILD").as_deref() != Ok("1") {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", TEST, "--nocapture"])
                .env("BEAMBENCH_ROLLBACK_TEST_CHILD", "1")
                .env("BEAMBENCH_API_TOKEN", "")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        let reserved = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = reserved.local_addr().unwrap().port();
        drop(reserved);
        let settings = AppSettings {
            api_enabled: true,
            api_port: port,
            ..Default::default()
        };
        let ctx = Arc::new(ServiceContext::with_settings(settings.clone()));
        let runtime = ApiRuntime::default();
        runtime.sync_from_settings(ctx.clone(), &settings).unwrap();
        let assert_serving = || {
            use std::io::{Read, Write};
            let mut socket = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
            socket
                .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                .unwrap();
            write!(socket, "GET /api/v1/app/status HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n").unwrap();
            let mut response = String::new();
            socket.read_to_string(&mut response).unwrap();
            assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        };
        assert_serving();
        let remote = AppSettings {
            api_localhost_only: false,
            ..settings.clone()
        };
        assert!(runtime.sync_from_settings(ctx.clone(), &remote).is_err());
        assert!(runtime.server.lock().unwrap().is_some());
        assert_serving();
        runtime
            .sync_from_settings(
                ctx,
                &AppSettings {
                    api_enabled: false,
                    ..settings
                },
            )
            .unwrap();
    }

    #[test]
    fn taken_port_keeps_the_running_api() {
        let reserve = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = reserve.local_addr().unwrap().port();
        drop(reserve);
        let occupied = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let settings = AppSettings {
            api_enabled: true,
            api_port: port,
            ..Default::default()
        };
        let ctx = Arc::new(ServiceContext::with_settings(settings.clone()));
        let runtime = ApiRuntime::default();
        runtime.sync_from_settings(ctx.clone(), &settings).unwrap();
        let rejected = AppSettings {
            api_port: occupied.local_addr().unwrap().port(),
            ..settings.clone()
        };
        assert!(runtime.sync_from_settings(ctx.clone(), &rejected).is_err());
        let running_port = runtime
            .server
            .lock()
            .unwrap()
            .as_ref()
            .map(|running| running.config.port);
        assert_eq!(running_port, Some(port));
        assert!(std::net::TcpStream::connect(("127.0.0.1", port)).is_ok());
        runtime
            .sync_from_settings(
                ctx,
                &AppSettings {
                    api_enabled: false,
                    ..settings
                },
            )
            .unwrap();
    }

    #[test]
    fn switching_to_network_mode_on_the_same_port_restarts_the_api() {
        const TEST: &str =
            "state::tests::switching_to_network_mode_on_the_same_port_restarts_the_api";
        if std::env::var("BEAMBENCH_SAME_PORT_TEST_CHILD").as_deref() != Ok("1") {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", TEST, "--nocapture"])
                .env("BEAMBENCH_SAME_PORT_TEST_CHILD", "1")
                .env("BEAMBENCH_API_TOKEN", "a".repeat(40))
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        let reserve = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = reserve.local_addr().unwrap().port();
        drop(reserve);
        let settings = AppSettings {
            api_enabled: true,
            api_port: port,
            ..Default::default()
        };
        let ctx = Arc::new(ServiceContext::with_settings(settings.clone()));
        let runtime = ApiRuntime::default();
        runtime.sync_from_settings(ctx.clone(), &settings).unwrap();
        let network = AppSettings {
            api_localhost_only: false,
            ..settings.clone()
        };
        runtime.sync_from_settings(ctx.clone(), &network).unwrap();
        let running = runtime
            .server
            .lock()
            .unwrap()
            .as_ref()
            .map(|running| running.config.localhost_only);
        assert_eq!(running, Some(false));
        runtime
            .sync_from_settings(
                ctx,
                &AppSettings {
                    api_enabled: false,
                    ..settings
                },
            )
            .unwrap();
    }
}
