/// Configuration for the local HTTP API server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiConfig {
    pub port: u16,
    pub localhost_only: bool,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            port: 5900,
            localhost_only: true,
        }
    }
}

impl ApiConfig {
    /// Build from `AppSettings` API fields.
    pub fn from_settings(settings: &beambench_core::AppSettings) -> Self {
        Self {
            port: settings.api_port,
            localhost_only: settings.api_localhost_only,
        }
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
