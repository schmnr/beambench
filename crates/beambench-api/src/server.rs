use std::sync::Arc;

use axum::{
    Router,
    extract::{Request, State},
    http::{StatusCode, header},
    middleware::{self, Next},
    response::Response,
};
use beambench_service::ServiceContext;
use sha2::{Digest, Sha256};

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
        routes::build_router(self.ctx.clone()).layer(middleware::from_fn_with_state(
            self.config.clone(),
            guard_request,
        ))
    }

    fn validate_config(&self) -> std::io::Result<()> {
        if !self.config.localhost_only && !valid_token(self.config.bearer_token.as_deref()) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "Network API requires BEAMBENCH_API_TOKEN with at least 32 ASCII characters and no whitespace",
            ));
        }
        Ok(())
    }

    pub fn listen_addr(&self) -> String {
        if self.config.localhost_only {
            format!("127.0.0.1:{}", self.config.port)
        } else {
            format!("0.0.0.0:{}", self.config.port)
        }
    }

    pub fn bind_std_listener(&self) -> Result<std::net::TcpListener, std::io::Error> {
        self.validate_config()?;
        let listener = std::net::TcpListener::bind(self.listen_addr())?;
        listener.set_nonblocking(true)?;
        Ok(listener)
    }

    pub async fn run_with_listener(
        &self,
        listener: tokio::net::TcpListener,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.validate_config()?;
        tracing::info!("API server listening on {}", self.listen_addr());
        axum::serve(listener, self.router()).await?;
        Ok(())
    }

    /// Start the server. Returns when the server shuts down.
    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.validate_config()?;
        let listener = tokio::net::TcpListener::bind(self.listen_addr()).await?;
        self.run_with_listener(listener).await
    }
}

fn valid_token(token: Option<&str>) -> bool {
    token.is_some_and(|token| {
        token.len() >= 32
            && token.is_ascii()
            && !token
                .bytes()
                .any(|b| b.is_ascii_whitespace() || b.is_ascii_control())
    })
}

async fn guard_request(
    State(config): State<ApiConfig>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    if config.localhost_only {
        let host = request
            .headers()
            .get(header::HOST)
            .and_then(|v| v.to_str().ok());
        let allowed = host.is_some_and(|host| {
            host.parse::<axum::http::uri::Authority>()
                .ok()
                .is_some_and(|authority| {
                    matches!(authority.host(), "127.0.0.1" | "localhost" | "[::1]")
                        && authority.port_u16().unwrap_or(80) == config.port
                })
        });
        if !allowed {
            return Err(StatusCode::FORBIDDEN);
        }
    } else {
        if !valid_token(config.bearer_token.as_deref()) {
            return Err(StatusCode::UNAUTHORIZED);
        }
        let supplied = request
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "));
        let Some(supplied) = supplied else {
            return Err(StatusCode::UNAUTHORIZED);
        };
        let expected = Sha256::digest(config.bearer_token.as_deref().unwrap().as_bytes());
        let actual = Sha256::digest(supplied.as_bytes());
        let difference = expected
            .iter()
            .zip(actual.iter())
            .fold(0u8, |difference, (a, b)| difference | (a ^ b));
        if difference != 0 {
            return Err(StatusCode::UNAUTHORIZED);
        }
    }
    Ok(next.run(request).await)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower::ServiceExt;

    async fn status(
        config: ApiConfig,
        host: &str,
        token: Option<&str>,
        origin: bool,
    ) -> StatusCode {
        let server = ApiServer::new(config, Arc::new(ServiceContext::new()));
        let mut request = axum::http::Request::builder()
            .uri("/api/v1/app/status")
            .header(header::HOST, host);
        if let Some(token) = token {
            request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
        }
        if origin {
            request = request.header(header::ORIGIN, "https://example.invalid");
        }
        server
            .router()
            .oneshot(request.body(axum::body::Body::empty()).unwrap())
            .await
            .unwrap()
            .status()
    }

    #[tokio::test]
    async fn localhost_rejects_rebinding_hosts_and_wrong_ports() {
        for host in [
            "evil.example:5900",
            "127.0.0.1:9999",
            "localhost.evil:5900",
            "localhost",
            "localhost:5900@evil.example:5900",
        ] {
            assert_eq!(
                status(ApiConfig::default(), host, None, false).await,
                StatusCode::FORBIDDEN,
                "{host}"
            );
        }
        for host in ["localhost:5900", "127.0.0.1:5900", "[::1]:5900"] {
            assert_eq!(
                status(ApiConfig::default(), host, None, false).await,
                StatusCode::OK
            );
        }
    }

    #[tokio::test]
    async fn network_requires_token_and_still_rejects_browser_origins() {
        let token = "0123456789abcdef0123456789abcdef";
        let config = ApiConfig {
            localhost_only: false,
            bearer_token: Some(token.into()),
            ..Default::default()
        };
        assert_eq!(
            status(config.clone(), "192.168.1.2:5900", None, false).await,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            status(config.clone(), "192.168.1.2:5900", Some("wrong"), false).await,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            status(config.clone(), "192.168.1.2:5900", Some(token), false).await,
            StatusCode::OK
        );
        assert_eq!(
            status(config, "192.168.1.2:5900", Some(token), true).await,
            StatusCode::FORBIDDEN
        );
    }

    #[test]
    fn unauthenticated_network_config_cannot_bind() {
        for token in [
            None,
            Some("short"),
            Some("0123456789abcdef0123456789abcdef\n"),
        ] {
            let config = ApiConfig {
                localhost_only: false,
                bearer_token: token.map(str::to_owned),
                ..Default::default()
            };
            assert!(
                ApiServer::new(config, Arc::new(ServiceContext::new()))
                    .bind_std_listener()
                    .is_err()
            );
        }
    }
}
