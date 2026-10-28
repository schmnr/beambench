//! A stationary host needs no webview or desktop login to own a USB job.
use beambench_api::{ApiConfig, ApiServer};
use beambench_service::{
    ServiceContext,
    ops::{machine, profiles},
};
use clap::Subcommand;
use std::{path::PathBuf, sync::Arc};

#[derive(Subcommand)]
pub enum HostCmd {
    /// Import a desktop-exported .bbprofile into this host's settings (run before serve)
    ImportProfile { path: PathBuf },
    /// Serve the machine API until stopped. Laptop disconnects do not stop jobs.
    Serve {
        #[arg(long, default_value_t = 5900)]
        port: u16,
        /// Listen on the LAN; requires BEAMBENCH_API_TOKEN (32+ ASCII characters)
        #[arg(long)]
        network: bool,
    },
}
impl HostCmd {
    pub fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        let ctx = Arc::new(ServiceContext::new());
        match self {
            Self::ImportProfile { path } => {
                let profile = profiles::import_machine_profile_from_path(&ctx, &path)?;
                println!("Imported host profile '{}' ({})", profile.name, profile.id);
                Ok(())
            }
            Self::Serve { port, network } => {
                let server = ApiServer::new(
                    ApiConfig {
                        port,
                        localhost_only: !network,
                        bearer_token: std::env::var("BEAMBENCH_API_TOKEN").ok(),
                    },
                    ctx.clone(),
                );
                server.validate_config()?;
                let runtime = tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()?;
                runtime.block_on(async move {
                    // Poll idle controller state even with no connected desktop.
                    let poll_ctx = ctx.clone();
                    let poller = tokio::spawn(async move {
                        loop {
                            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                            let ctx = poll_ctx.clone();
                            let _ = tokio::task::spawn_blocking(move || {
                                if !ctx.job_tick_loop_running.load(std::sync::atomic::Ordering::Acquire) { let _ = machine::machine_status(&ctx); }
                            }).await;
                        }
                    });
                    println!("Beam Bench host listening on {}. Keep this process running; no job resumes after a host restart.", server.listen_addr());
                    // Install stop handlers before the listener can accept requests.
                    let result = tokio::select! {
                        biased;
                        signal = shutdown_signal() => signal.map_err(Into::into),
                        result = server.run() => result,
                    };
                    poller.abort();
                    // A host shutdown is distinct from a laptop disconnect.
                    let shutdown = tokio::task::spawn_blocking(move || machine::prepare_shutdown(&ctx)).await?;
                    shutdown?;
                    result
                })
            }
        }
    }
}

async fn shutdown_signal() -> std::io::Result<()> {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        tokio::select! { result = tokio::signal::ctrl_c() => result, _ = terminate.recv() => Ok(()) }
    }
    #[cfg(windows)]
    {
        // Launchers stop an isolated Windows process group with Ctrl+Break.
        // Handle it through the same controller shutdown path as Ctrl+C.
        let mut interrupt = tokio::signal::windows::ctrl_c()?;
        let mut terminate = tokio::signal::windows::ctrl_break()?;
        tokio::select! { _ = interrupt.recv() => Ok(()), _ = terminate.recv() => Ok(()) }
    }
    #[cfg(not(any(unix, windows)))]
    tokio::signal::ctrl_c().await
}
