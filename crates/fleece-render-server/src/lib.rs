//! HTTP server and profile providers for fleece-render.

mod config;
mod error;
mod http;
mod input;
mod providers;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use fleece_render_core::{RenderEngine, RenderOptions, RenderOutput, SkinAssets, Texture};
use reqwest::Client;
use tokio::sync::Semaphore;

pub use config::AppConfig;
pub use error::{ApiError, ApiResult};
pub use http::build_router;
pub use providers::{PlayerProfile, ProfileSource, fetch_profile};

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<AppConfig>,
    pub engine: Arc<RenderEngine>,
    pub client: Client,
    pub renders: Arc<Semaphore>,
    pub started_at: Instant,
    texture_cache: Arc<Mutex<HashMap<String, CachedBytes>>>,
    profile_cache: Arc<Mutex<HashMap<String, CachedProfile>>>,
}

#[derive(Clone)]
struct CachedBytes {
    fetched_at: Instant,
    bytes: Arc<Vec<u8>>,
}

struct CachedProfile {
    fetched_at: Instant,
    profile: PlayerProfile,
}

impl AppState {
    pub fn new(config: AppConfig) -> Result<Self> {
        let backend = config.render.backend_kind()?;
        let engine =
            RenderEngine::new(backend).with_context(|| "failed to initialize rendering engine")?;
        let client = Client::builder()
            .no_proxy()
            .user_agent(config.network.user_agent.clone())
            .connect_timeout(Duration::from_secs(10))
            .pool_idle_timeout(Duration::from_secs(90))
            .build()
            .context("failed to initialize HTTP client")?;
        Ok(Self {
            renders: Arc::new(Semaphore::new(config.server.render_concurrency)),
            client,
            engine: Arc::new(engine),
            started_at: Instant::now(),
            texture_cache: Arc::new(Mutex::new(HashMap::new())),
            profile_cache: Arc::new(Mutex::new(HashMap::new())),
            config: Arc::new(config),
        })
    }

    /// Warm DNS/TLS/connection pools without blocking startup or failing if
    /// the machine is offline. Mojang remains direct (no proxy), matching the
    /// actual request path used by the providers.
    pub fn warmup_network(&self) {
        let client = self.client.clone();
        tokio::spawn(async move {
            let started = Instant::now();
            let timeout = Duration::from_secs(5);
            let (a, b, c, d) = tokio::join!(
                client
                    .get("https://api.mojang.com/")
                    .timeout(timeout)
                    .send(),
                client
                    .get("https://sessionserver.mojang.com/")
                    .timeout(timeout)
                    .send(),
                client
                    .get("http://textures.minecraft.net/")
                    .timeout(timeout)
                    .send(),
                client.get("https://littleskin.cn/").timeout(timeout).send(),
            );
            let successful = [a, b, c, d]
                .into_iter()
                .filter(|result| result.is_ok())
                .count();
            tracing::info!(
                successful,
                elapsed_ms = started.elapsed().as_secs_f64() * 1000.0,
                "network warmup finished"
            );
        });
    }

    pub async fn texture(&self, source: &str, role: &str) -> ApiResult<Texture> {
        let texture = self.decoded_texture(source, role).await?;
        if role == "skin" {
            Ok(texture.prepare_skin()?)
        } else {
            Ok(texture.prepare_back(role)?)
        }
    }

    async fn skin_texture(&self, source: &str, options: &RenderOptions) -> ApiResult<Texture> {
        prepare_skin(self.decoded_texture(source, "skin").await?, options)
    }

    async fn decoded_texture(&self, source: &str, role: &str) -> ApiResult<Texture> {
        let cacheable = is_cacheable_source(source);
        if cacheable {
            let cached = self
                .texture_cache
                .lock()
                .ok()
                .and_then(|cache| cache.get(source).cloned())
                .filter(|entry| entry.fetched_at.elapsed() < Duration::from_secs(600));
            if let Some(entry) = cached {
                return Ok(Texture::from_bytes(&entry.bytes, role)?);
            }
        }

        let bytes =
            crate::input::bytes_from_source(source, role, &self.client, &self.config).await?;
        let bytes = Arc::new(bytes);
        if cacheable && let Ok(mut cache) = self.texture_cache.lock() {
            if cache.len() >= 256 {
                cache.clear();
            }
            cache.insert(
                source.to_owned(),
                CachedBytes {
                    fetched_at: Instant::now(),
                    bytes: bytes.clone(),
                },
            );
        }
        Ok(Texture::from_bytes(&bytes, role)?)
    }

    pub async fn profile(&self, source: ProfileSource, player: &str) -> ApiResult<PlayerProfile> {
        let key = format!("{}:{}", source.id(), player.trim().to_ascii_lowercase());
        if let Ok(cache) = self.profile_cache.lock()
            && let Some(entry) = cache.get(&key)
            && entry.fetched_at.elapsed() < Duration::from_secs(300)
        {
            return Ok(entry.profile.clone());
        }
        let profile = fetch_profile(&self.client, &self.config, source, player).await?;
        if let Ok(mut cache) = self.profile_cache.lock() {
            if cache.len() >= 256 {
                cache.clear();
            }
            cache.insert(
                key,
                CachedProfile {
                    fetched_at: Instant::now(),
                    profile: profile.clone(),
                },
            );
        }
        Ok(profile)
    }

    pub async fn render(
        &self,
        assets: SkinAssets,
        options: RenderOptions,
        timeout_seconds: Option<f64>,
    ) -> ApiResult<RenderOutput> {
        let permit = self
            .renders
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| ApiError::unavailable("render queue is closed"))?;
        let engine = self.engine.clone();
        let started = Instant::now();
        let style = options.style.clone();
        let parts = options.parts.to_string();
        let handle = tokio::task::spawn_blocking(move || {
            // A timed-out HTTP request does not cancel a blocking render.
            let _permit = permit;
            engine.render(&assets, &options)
        });
        let timeout =
            Duration::from_secs_f64(timeout_seconds.unwrap_or(self.config.render.timeout_seconds));
        let result = tokio::time::timeout(timeout, handle)
            .await
            .map_err(|_| {
                ApiError::timeout(format!(
                    "render timed out after {:.3}s",
                    timeout.as_secs_f64()
                ))
            })?
            .map_err(|error| ApiError::internal(format!("render worker failed: {error}")))??;
        tracing::info!(
            elapsed_ms = started.elapsed().as_secs_f64() * 1000.0,
            core_ms = result.info.elapsed_ms,
            style,
            parts,
            model = %result.info.model,
            backend = %result.info.backend,
            triangles = result.info.triangles,
            "render complete"
        );
        Ok(result)
    }
}

fn prepare_skin(texture: Texture, options: &RenderOptions) -> ApiResult<Texture> {
    // Avatar compositing needs the original alpha and legacy layout for its hat heuristic.
    if options.is_avatar() {
        Ok(texture)
    } else {
        Ok(texture.prepare_skin()?)
    }
}

fn is_cacheable_source(source: &str) -> bool {
    let source = source.trim();
    !source.starts_with("data:") && source.len() <= 4096
}
