/// Configuration for the local HTTP API server.
#[derive(Clone, PartialEq, Eq)]
pub struct ApiConfig {
    pub port: u16,
    pub localhost_only: bool,
    /// Required for every client when binding to the network. Never logged.
    pub bearer_token: Option<String>,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            port: 5900,
            localhost_only: true,
            bearer_token: None,
        }
    }
}

impl ApiConfig {
    /// Build from `AppSettings` API fields.
    pub fn from_settings(settings: &beambench_core::AppSettings) -> Self {
        Self {
            port: settings.api_port,
            localhost_only: settings.api_localhost_only,
            bearer_token: std::env::var("BEAMBENCH_API_TOKEN").ok(),
        }
    }
}

impl std::fmt::Debug for ApiConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApiConfig")
            .field("port", &self.port)
            .field("localhost_only", &self.localhost_only)
            .field("bearer_token_configured", &self.bearer_token.is_some())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config() {
        let cfg = ApiConfig::default();
        assert_eq!(cfg.port, 5900);
        assert!(cfg.localhost_only, "API must default to localhost-only");
    }

    #[test]
    fn from_settings() {
        let s = beambench_core::AppSettings {
            api_port: 8080,
            api_localhost_only: false,
            ..Default::default()
        };
        let cfg = ApiConfig::from_settings(&s);
        assert_eq!(cfg.port, 8080);
        assert!(!cfg.localhost_only);
    }
}
