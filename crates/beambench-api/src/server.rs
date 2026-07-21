use std::sync::Arc;

use axum::Router;
use beambench_service::ServiceContext;

use crate::config::ApiConfig;
use crate::routes;

/// Local HTTP API server for Beam Bench.
pub struct ApiServer {
    config: ApiConfig,
    ctx: Arc<ServiceContext>,
}

impl ApiServer {
    pub fn new(config: ApiConfig, ctx: Arc<ServiceContext>) -> Self {
        Self { config, ctx }
    }

    /// Build the Axum router (useful for testing with `oneshot`).
    pub fn router(&self) -> Router {
        routes::build_router(self.ctx.clone())
    }

    pub fn listen_addr(&self) -> String {
        if self.config.localhost_only {
            format!("127.0.0.1:{}", self.config.port)
        } else {
            format!("0.0.0.0:{}", self.config.port)
        }
    }

    pub fn bind_std_listener(&self) -> Result<std::net::TcpListener, std::io::Error> {
        let listener = std::net::TcpListener::bind(self.listen_addr())?;
        listener.set_nonblocking(true)?;
        Ok(listener)
    }

    pub async fn run_with_listener(
        &self,
        listener: tokio::net::TcpListener,
    ) -> Result<(), Box<dyn std::error::Error>> {
        tracing::info!("API server listening on {}", self.listen_addr());
        axum::serve(listener, self.router()).await?;
        Ok(())
    }

    /// Start the server. Returns when the server shuts down.
    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        let listener = tokio::net::TcpListener::bind(self.listen_addr()).await?;
        self.run_with_listener(listener).await
    }
}
