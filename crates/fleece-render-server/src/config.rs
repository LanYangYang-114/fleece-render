use std::fmt;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use fleece_render_core::{BackendKind, RenderOptions};
use serde::Deserialize;

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub render: RenderConfig,
    pub network: NetworkConfig,
}

#[derive(Clone, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub allow_cors: bool,
    pub auth_token: String,
    pub render_concurrency: usize,
    pub log_filter: String,
    pub log_file: Option<String>,
    pub log_max_files: usize,
    pub body_limit_bytes: usize,
}

impl fmt::Debug for ServerConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ServerConfig")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("allow_cors", &self.allow_cors)
            .field("auth_token", &"[redacted]")
            .field("render_concurrency", &self.render_concurrency)
            .field("log_filter", &self.log_filter)
            .field("log_file", &self.log_file)
            .field("log_max_files", &self.log_max_files)
            .field("body_limit_bytes", &self.body_limit_bytes)
            .finish()
    }
}

impl ServerConfig {
    pub(crate) fn validate_auth_token(&self) -> Result<()> {
        if self.auth_token.is_empty() {
            return Ok(());
        }
        // RFC 6750 b64token: one or more token characters, then optional '=' padding.
        let token = self.auth_token.trim_end_matches('=');
        if token.is_empty()
            || !token
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-._~+/".contains(&b))
        {
            anyhow::bail!("server.auth_token must be empty or a valid Bearer token");
        }
        Ok(())
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_owned(),
            port: 57680,
            allow_cors: false,
            auth_token: String::new(),
            render_concurrency: 4,
            log_filter:
                "fleece_render=info,fleece_render_server=info,fleece_render_core=info,tower_http=info"
                    .to_owned(),
            log_file: Some("logs/fleece-render.log".to_owned()),
            log_max_files: 30,
            body_limit_bytes: 16 * 1024 * 1024,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct RenderConfig {
    pub backend: String,
    pub definition: f32,
    pub transparent: bool,
    pub panel_width: u32,
    pub panel_height: u32,
    pub fov: f32,
    pub zoom: f32,
    pub target_y: f32,
    pub name_tag_height: f32,
    pub name_tag_y: f32,
    pub name_tag_max_width_ratio: f32,
    pub shadow: bool,
    pub supersample: u32,
    pub timeout_seconds: f64,
    pub max_texture_bytes: usize,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            backend: "auto".to_owned(),
            definition: 1.5,
            transparent: false,
            panel_width: 350,
            panel_height: 670,
            fov: 70.0,
            zoom: 1.0,
            target_y: 4.0,
            name_tag_height: 4.0,
            name_tag_y: 22.0,
            name_tag_max_width_ratio: 0.80,
            shadow: true,
            supersample: 2,
            timeout_seconds: 30.0,
            max_texture_bytes: 8 * 1024 * 1024,
        }
    }
}

impl RenderConfig {
    pub fn backend_kind(&self) -> Result<BackendKind> {
        self.backend
            .parse()
            .map_err(|error| anyhow::anyhow!("invalid render.backend: {error}"))
    }

    pub fn default_options(&self) -> RenderOptions {
        RenderOptions {
            definition: self.definition,
            transparent: self.transparent,
            panel_width: self.panel_width,
            panel_height: self.panel_height,
            fov: self.fov,
            zoom: self.zoom,
            target_y: self.target_y,
            name_tag_height: self.name_tag_height,
            name_tag_y: self.name_tag_y,
            name_tag_max_width_ratio: self.name_tag_max_width_ratio,
            shadow: self.shadow,
            supersample: self.supersample,
            ..RenderOptions::default()
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct NetworkConfig {
    pub profile_timeout_seconds: f64,
    pub texture_timeout_seconds: f64,
    pub user_agent: String,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            profile_timeout_seconds: 15.0,
            texture_timeout_seconds: 20.0,
            user_agent: format!("fleece-render/{}", env!("CARGO_PKG_VERSION")),
        }
    }
}

impl AppConfig {
    pub fn load(path: Option<&Path>) -> Result<Self> {
        let mut config = match path {
            Some(path) if path.exists() => {
                let text = std::fs::read_to_string(path)
                    .with_context(|| format!("failed to read config {}", path.display()))?;
                toml::from_str(&text)
                    .with_context(|| format!("failed to parse config {}", path.display()))?
            }
            Some(path) => {
                anyhow::bail!("config file does not exist: {}", path.display());
            }
            None => Self::default(),
        };
        config.apply_env();
        config.validate()?;
        Ok(config)
    }

    fn apply_env(&mut self) {
        if let Ok(value) = std::env::var("FLEECE_RENDER_HOST") {
            self.server.host = value;
        }
        if let Ok(value) = std::env::var("FLEECE_RENDER_PORT")
            && let Ok(port) = value.parse()
        {
            self.server.port = port;
        }
        if let Ok(value) = std::env::var("FLEECE_RENDER_BACKEND") {
            self.render.backend = value;
        }
        if let Ok(value) = std::env::var("FLEECE_RENDER_LOG") {
            self.server.log_filter = value;
        }
        if let Ok(value) = std::env::var("FLEECE_RENDER_LOG_FILE") {
            self.server.log_file = (!value.trim().is_empty()).then_some(value);
        }
    }

    fn validate(&self) -> Result<()> {
        self.server.validate_auth_token()?;
        if self.server.port == 0 {
            anyhow::bail!("server.port must be non-zero");
        }
        if self.server.render_concurrency == 0 {
            anyhow::bail!("server.render_concurrency must be positive");
        }
        if self.server.log_max_files == 0 {
            anyhow::bail!("server.log_max_files must be positive");
        }
        if self.render.panel_width == 0 || self.render.panel_height == 0 {
            anyhow::bail!("panel dimensions must be positive");
        }
        if !(0.25..=5.0).contains(&self.render.definition) {
            anyhow::bail!("render.definition must be between 0.25 and 5.0");
        }
        if self.render.timeout_seconds <= 0.0 {
            anyhow::bail!("render.timeout_seconds must be positive");
        }
        self.render.backend_kind()?;
        self.render
            .default_options()
            .validate()
            .map_err(|error| anyhow::anyhow!("{error}"))?;
        Ok(())
    }

    pub fn config_path() -> Option<PathBuf> {
        std::env::var_os("FLEECE_RENDER_CONFIG").map(PathBuf::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bearer_token_defaults_to_empty_and_is_redacted_in_debug_output() {
        let defaults: AppConfig = toml::from_str("").unwrap();
        assert!(defaults.server.auth_token.is_empty());
        let config: AppConfig =
            toml::from_str("[server]\nauth_token = 'my-Secret_123+/=='").unwrap();
        assert_eq!(config.server.auth_token, "my-Secret_123+/==");
        config.validate().unwrap();
        assert!(!format!("{config:?}").contains("my-Secret_123+/=="));
    }

    #[test]
    fn invalid_bearer_tokens_are_rejected() {
        let mut config = AppConfig::default();
        for token in [" ", "secret value", "secret\t", "secret=more", "=", "令牌"] {
            config.server.auth_token = token.to_owned();
            let error = config.validate().unwrap_err().to_string();
            assert_eq!(
                error,
                "server.auth_token must be empty or a valid Bearer token"
            );
        }
    }

    #[test]
    fn log_retention_defaults_to_thirty_and_accepts_positive_overrides() {
        let default: AppConfig = toml::from_str("").unwrap();
        assert_eq!(default.server.log_max_files, 30);
        for limit in [1, 2, 30, 100] {
            let config: AppConfig =
                toml::from_str(&format!("[server]\nlog_max_files = {limit}")).unwrap();
            assert_eq!(config.server.log_max_files, limit);
            config.validate().unwrap();
        }
    }

    #[test]
    fn zero_log_retention_is_rejected() {
        let config: AppConfig = toml::from_str("[server]\nlog_max_files = 0").unwrap();
        assert!(
            config
                .validate()
                .unwrap_err()
                .to_string()
                .contains("log_max_files")
        );
        assert!(toml::from_str::<AppConfig>("[server]\nlog_max_files = -1").is_err());
    }
}
