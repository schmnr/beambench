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

        let mut guard = self
            .server
            .lock()
            .map_err(|e| format!("Failed to lock API runtime: {e}"))?;

        if let Some(running) = guard.as_ref()
            && desired.as_ref() == Some(&running.config)
        {
            return Ok(());
        }

        if let Some(running) = guard.take() {
            running.handle.abort();
            tracing::info!("API server stopped");
        }

        if let Some(config) = desired {
            let server = ApiServer::new(config.clone(), ctx);
            let std_listener = server
                .bind_std_listener()
                .map_err(|e| format!("Failed to bind API listener: {e}"))?;
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
            *guard = Some(RunningApiServer { config, handle });
            tracing::info!("API server started");
        }

        Ok(())
    }
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
}
