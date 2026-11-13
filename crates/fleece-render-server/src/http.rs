use axum::body::{Body, Bytes};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderValue, Request, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine as _;
use fleece_render_core::{
    BackEquipment, ModelOverride, Parts, RenderInfo, RenderOptions, RenderedImage, SkinAssets,
    available_backends,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::str::FromStr;
use std::sync::Arc;
use subtle::ConstantTimeEq;
use tower_http::cors::CorsLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::trace::TraceLayer;
use tracing::Span;

use crate::config::AppConfig;
use crate::error::{ApiError, ApiResult};
use crate::input::decode_request_body;
use crate::providers::{PlayerProfile, ProfileSource};
use crate::{AppState, prepare_skin};

pub fn build_router(state: AppState) -> Router {
    let body_limit = state.config.server.body_limit_bytes;
    let mut router = Router::new()
        .route("/", get(root))
        .route("/health", get(health))
        .route("/url/image/{parts}", get(url_image))
        .route("/url/json/all", get(url_json_all))
        .route("/raw/image/{parts}", post(raw_image))
        .route("/mojang/image/{parts}", get(mojang_image))
        .route("/littleskin/image/{parts}", get(littleskin_image))
        .route("/api/v1", get(api_root))
        .route("/api/v1/styles", get(styles))
        .route("/api/v1/render/{parts}", post(api_v1_render))
        .layer(RequestBodyLimitLayer::new(body_limit));
    if state.config.server.allow_cors {
        router = router.layer(CorsLayer::permissive());
    }
    if !state.config.server.auth_token.is_empty() {
        // Outside CORS and body limits so OPTIONS and oversized requests also require auth.
        router = router.layer(middleware::from_fn_with_state(
            state.config.clone(),
            require_bearer_token,
        ));
    }
    router
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(|request: &Request<Body>| {
                    tracing::info_span!(
                        "http_request",
                        method = %request.method(),
                        path = %request.uri().path(),
                        query_keys = %query_keys(request.uri().query()),
                        status = tracing::field::Empty,
                    )
                })
                .on_request(|request: &Request<Body>, _span: &Span| {
                    tracing::info!(
                        method = %request.method(),
                        path = %request.uri().path(),
                        query_keys = %query_keys(request.uri().query()),
                        content_length = ?request.headers().get(header::CONTENT_LENGTH),
                        "incoming HTTP request"
                    );
                })
                .on_response(
                    |response: &axum::http::Response<Body>,
                     latency: std::time::Duration,
                     span: &Span| {
                        span.record("status", tracing::field::display(response.status()));
                        tracing::info!(
                            status = %response.status(),
                            latency_ms = latency.as_secs_f64() * 1000.0,
                            content_length = ?response.headers().get(header::CONTENT_LENGTH),
                            "HTTP response completed"
                        );
                    },
                ),
        )
        .with_state(state)
}

async fn require_bearer_token(
    State(config): State<Arc<AppConfig>>,
    request: Request<Body>,
    next: Next,
) -> Response {
    if request.uri().path() == "/health" {
        return next.run(request).await;
    }

    let mut values = request.headers().get_all(header::AUTHORIZATION).iter();
    let credential = values
        .next()
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split_once(' '))
        .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("Bearer"))
        .map(|(_, token)| token.trim_start_matches(' '));
    let authorized = values.next().is_none()
        && credential.is_some_and(|token| {
            bool::from(token.as_bytes().ct_eq(config.server.auth_token.as_bytes()))
        });
    let mut response = if authorized {
        next.run(request).await
    } else {
        let mut response = ApiError::new(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "A valid Bearer token is required",
        )
        .into_response();
        response.headers_mut().insert(
            header::WWW_AUTHENTICATE,
            HeaderValue::from_static("Bearer realm=\"fleece-render\""),
        );
        response
    };
    // Override the normal public PNG caching policy on protected routes.
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// Return query parameter names without logging their values. This keeps skin
/// URLs and base64 image data out of logs while preserving request diagnostics.
fn query_keys(query: Option<&str>) -> String {
    query
        .unwrap_or_default()
        .split('&')
        .filter(|item| !item.is_empty())
        .map(|item| item.split('=').next().unwrap_or_default())
        .filter(|key| !key.is_empty())
        .collect::<Vec<_>>()
        .join(",")
}

async fn root() -> Json<Value> {
    Json(json!({
        "status": 200,
        "msg": "fleece-render",
        "package": "fleece-render",
        "version": env!("CARGO_PKG_VERSION"),
        "api": "/api/v1",
    }))
}

async fn health(State(state): State<AppState>) -> Json<Value> {
    Json(json!({
        "status": 200,
        "ok": true,
        "package": {
            "name": "fleece-render",
            "version": env!("CARGO_PKG_VERSION"),
            "implementation": "rust",
        },
        "styles": state.engine.styles().ids(),
        "sources": ["mojang", "littleskin"],
        "backend": state.engine.backend_id(),
        "availableBackends": available_backends(),
        "uptimeSeconds": state.started_at.elapsed().as_secs_f64(),
        "render": {
            "definition": state.config.render.definition,
            "panelWidth": state.config.render.panel_width,
            "panelHeight": state.config.render.panel_height,
            "fov": state.config.render.fov,
            "supersample": state.config.render.supersample,
            "concurrency": state.config.server.render_concurrency,
        }
    }))
}

async fn api_root() -> Json<Value> {
    Json(json!({
        "name": "fleece-render API",
        "version": env!("CARGO_PKG_VERSION"),
        "endpoints": {
            "legacy": [
                "GET /url/image/{front|back|both|all}",
                "GET /url/json/all",
                "POST /raw/image/{front|back|both|all}",
                "GET /mojang/image/{front|back|both|all}?name=",
                "GET /littleskin/image/{front|back|both|all}?name="
            ],
            "v1": [
                "GET /api/v1/styles",
                "POST /api/v1/render/{front|back|both|all}?format=png|json"
            ]
        }
    }))
}

async fn styles(State(state): State<AppState>) -> Json<Value> {
    Json(json!({
        "styles": state.engine.styles().descriptions().into_iter().map(|(id, label)| {
            json!({"id": id, "label": label})
        }).collect::<Vec<_>>()
    }))
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommonQuery {
    style: Option<String>,
    definition: Option<f32>,
    transparent: Option<bool>,
    timeout: Option<f64>,
    show_outer_layer: Option<bool>,
    show_name: Option<bool>,
    name_tag: Option<String>,
    model: Option<String>,
}

impl CommonQuery {
    fn options(&self, state: &AppState, parts: Parts) -> ApiResult<RenderOptions> {
        let mut options = state.config.render.default_options();
        options.parts = parts;
        if let Some(style) = &self.style {
            options.style = style.clone();
        }
        if let Some(transparent) = self.transparent {
            options.transparent = transparent;
        }
        if let Some(show_outer_layer) = self.show_outer_layer {
            options.show_outer_layer = show_outer_layer;
        }
        if options.is_avatar() {
            options.validate()?;
            return Ok(options);
        }
        if let Some(definition) = self.definition {
            options.definition = definition;
        }
        options.show_name = self.show_name.unwrap_or(true);
        options.name_tag = self.name_tag.clone();
        if let Some(model) = &self.model {
            options.model = model.parse::<ModelOverride>()?;
        }
        options.validate()?;
        Ok(options)
    }

    fn timeout_seconds(&self, state: &AppState) -> f64 {
        self.timeout.unwrap_or(state.config.render.timeout_seconds)
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UrlQuery {
    style: Option<String>,
    definition: Option<f32>,
    transparent: Option<bool>,
    timeout: Option<f64>,
    show_outer_layer: Option<bool>,
    show_name: Option<bool>,
    name_tag: Option<String>,
    model: Option<String>,
    skin_url: Option<String>,
    skin_data: Option<String>,
    cape_url: Option<String>,
    cape_data: Option<String>,
    elytra_url: Option<String>,
    elytra_data: Option<String>,
    back_equipment: Option<String>,
}

impl UrlQuery {
    fn common_options(&self) -> CommonQuery {
        CommonQuery {
            style: self.style.clone(),
            definition: self.definition,
            transparent: self.transparent,
            timeout: self.timeout,
            show_outer_layer: self.show_outer_layer,
            show_name: self.show_name,
            name_tag: self.name_tag.clone(),
            model: self.model.clone(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProfileQuery {
    style: Option<String>,
    definition: Option<f32>,
    transparent: Option<bool>,
    timeout: Option<f64>,
    show_outer_layer: Option<bool>,
    show_name: Option<bool>,
    name_tag: Option<String>,
    model: Option<String>,
    name: String,
    use_cape: Option<bool>,
    back_equipment: Option<String>,
}

impl ProfileQuery {
    fn common_options(&self) -> CommonQuery {
        CommonQuery {
            style: self.style.clone(),
            definition: self.definition,
            transparent: self.transparent,
            timeout: self.timeout,
            show_outer_layer: self.show_outer_layer,
            show_name: self.show_name,
            name_tag: self.name_tag.clone(),
            model: self.model.clone(),
        }
    }
}

async fn url_image(
    State(state): State<AppState>,
    Path(parts): Path<String>,
    Query(query): Query<UrlQuery>,
) -> ApiResult<Response> {
    let parts = parse_parts(&parts)?;
    let mut options = query.common_options().options(&state, parts)?;
    apply_back_equipment(&mut options, query.back_equipment.as_deref())?;
    let assets = assets_from_url_query(&state, &query, &options).await?;
    let result = state
        .render(
            assets,
            options,
            Some(query.common_options().timeout_seconds(&state)),
        )
        .await?;
    Ok(png_response(primary_image(&result)?, &result.info))
}

async fn url_json_all(
    State(state): State<AppState>,
    Query(query): Query<UrlQuery>,
) -> ApiResult<Json<Value>> {
    let mut options = query.common_options().options(&state, Parts::All)?;
    apply_back_equipment(&mut options, query.back_equipment.as_deref())?;
    let assets = assets_from_url_query(&state, &query, &options).await?;
    let result = state
        .render(
            assets,
            options,
            Some(query.common_options().timeout_seconds(&state)),
        )
        .await?;
    Ok(Json(json!({
        "front_b64": b64(result.front.as_ref()),
        "back_b64": b64(result.back.as_ref()),
        "both_b64": b64(result.both.as_ref()),
        "backendInfo": info_json(&result.info),
    })))
}

async fn raw_image(
    State(state): State<AppState>,
    Path(parts): Path<String>,
    Query(query): Query<UrlQuery>,
    body: Bytes,
) -> ApiResult<Response> {
    let parts = parse_parts(&parts)?;
    let mut options = query.common_options().options(&state, parts)?;
    apply_back_equipment(&mut options, query.back_equipment.as_deref())?;
    let skin_bytes = decode_request_body(&body, "skin", &state.config)?;
    let skin = prepare_skin(
        fleece_render_core::Texture::from_bytes(&skin_bytes, "skin")?,
        &options,
    )?;
    let back = back_texture_from_query(&state, &query, &options).await?;
    let assets = SkinAssets {
        skin,
        back,
        display_name: None,
    };
    let result = state
        .render(
            assets,
            options,
            Some(query.common_options().timeout_seconds(&state)),
        )
        .await?;
    Ok(png_response(primary_image(&result)?, &result.info))
}

async fn mojang_image(
    State(state): State<AppState>,
    Path(parts): Path<String>,
    Query(query): Query<ProfileQuery>,
) -> ApiResult<Response> {
    profile_image(state, parts, query, ProfileSource::Mojang).await
}

async fn littleskin_image(
    State(state): State<AppState>,
    Path(parts): Path<String>,
    Query(query): Query<ProfileQuery>,
) -> ApiResult<Response> {
    profile_image(state, parts, query, ProfileSource::LittleSkin).await
}

async fn profile_image(
    state: AppState,
    parts: String,
    query: ProfileQuery,
    source: ProfileSource,
) -> ApiResult<Response> {
    let parts = parse_parts(&parts)?;
    let mut options = query.common_options().options(&state, parts)?;
    apply_back_equipment(&mut options, query.back_equipment.as_deref())?;
    let profile = state.profile(source, &query.name).await?;
    let assets =
        assets_from_profile(&state, &profile, &options, query.use_cape.unwrap_or(true)).await?;
    let result = state
        .render(
            assets,
            options,
            Some(query.common_options().timeout_seconds(&state)),
        )
        .await?;
    Ok(png_response(primary_image(&result)?, &result.info))
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TextureSpec {
    url: Option<String>,
    data: Option<String>,
    path: Option<String>,
}

impl TextureSpec {
    fn source(&self) -> Option<&str> {
        self.data
            .as_deref()
            .or(self.url.as_deref())
            .or(self.path.as_deref())
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlayerSpec {
    name: Option<String>,
    uuid: Option<String>,
    source: Option<String>,
    use_cape: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct V1Options {
    style: Option<String>,
    definition: Option<f32>,
    transparent: Option<bool>,
    timeout: Option<f64>,
    show_outer_layer: Option<bool>,
    show_name: Option<bool>,
    name_tag: Option<String>,
    model: Option<String>,
    back_equipment: Option<String>,
}

impl V1Options {
    fn common(&self) -> CommonQuery {
        CommonQuery {
            style: self.style.clone(),
            definition: self.definition,
            transparent: self.transparent,
            timeout: self.timeout,
            show_outer_layer: self.show_outer_layer,
            show_name: self.show_name,
            name_tag: self.name_tag.clone(),
            model: self.model.clone(),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct V1Request {
    player: Option<PlayerSpec>,
    skin: Option<TextureSpec>,
    cape: Option<TextureSpec>,
    elytra: Option<TextureSpec>,
    #[serde(flatten)]
    options: V1Options,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct V1Query {
    format: Option<String>,
}

async fn api_v1_render(
    State(state): State<AppState>,
    Path(parts): Path<String>,
    Query(query): Query<V1Query>,
    Json(request): Json<V1Request>,
) -> ApiResult<Response> {
    let parts = parse_parts(&parts)?;
    let common = request.options.common();
    let mut options = common.options(&state, parts)?;
    apply_back_equipment(&mut options, request.options.back_equipment.as_deref())?;
    let back_equipment = options.back_equipment;

    let mut profile = None;
    let mut display_name = None;
    let skin_input = request.skin.as_ref().and_then(TextureSpec::source);
    let mut cape_input = request
        .cape
        .as_ref()
        .and_then(TextureSpec::source)
        .map(str::to_owned);
    let mut elytra_input = request
        .elytra
        .as_ref()
        .and_then(TextureSpec::source)
        .map(str::to_owned);

    let skin = if let Some(source) = skin_input {
        state.skin_texture(source, &options).await?
    } else if let Some(player) = &request.player {
        let source = ProfileSource::parse(player.source.as_deref().unwrap_or("mojang"))?;
        let lookup = player
            .uuid
            .as_deref()
            .or(player.name.as_deref())
            .ok_or_else(|| ApiError::invalid("player requires name or uuid"))?;
        let fetched = state.profile(source, lookup).await?;
        if !options.is_avatar() && player.use_cape.unwrap_or(true) {
            cape_input = cape_input.or_else(|| fetched.cape_url.clone());
            if back_equipment == BackEquipment::Elytra {
                elytra_input = elytra_input.or_else(|| fetched.cape_url.clone());
            }
        }
        display_name = Some(fetched.name.clone());
        let skin = state.skin_texture(&fetched.skin_url, &options).await?;
        profile = Some(fetched);
        skin
    } else {
        return Err(ApiError::invalid("request must include skin or player"));
    };

    let back = match back_equipment {
        BackEquipment::Elytra => elytra_input.or(cape_input),
        BackEquipment::Cape => cape_input.or(elytra_input),
    };
    let back = if !options.is_avatar()
        && let Some(source) = back
    {
        Some(state.texture(&source, "back").await?)
    } else {
        None
    };
    let assets = SkinAssets {
        skin,
        back,
        display_name,
    };
    let result = state
        .render(assets, options, Some(common.timeout_seconds(&state)))
        .await?;

    let format = query.format.as_deref().unwrap_or("json");
    match format {
        "json" => Ok(Json(json!({
            "ok": true,
            "front_b64": b64(result.front.as_ref()),
            "back_b64": b64(result.back.as_ref()),
            "both_b64": b64(result.both.as_ref()),
            "backendInfo": info_json(&result.info),
            "profile": profile.as_ref().map(PlayerProfile::summary),
        }))
        .into_response()),
        "png" | "image" => Ok(png_response(primary_image(&result)?, &result.info)),
        other => Err(ApiError::invalid(format!(
            "unknown response format {other:?}; expected png or json"
        ))),
    }
}

async fn assets_from_url_query(
    state: &AppState,
    query: &UrlQuery,
    options: &RenderOptions,
) -> ApiResult<SkinAssets> {
    let skin_source = query
        .skin_data
        .as_deref()
        .or(query.skin_url.as_deref())
        .ok_or_else(|| ApiError::invalid("skinUrl or skinData is required"))?;
    let skin = state.skin_texture(skin_source, options).await?;
    let back = back_texture_from_query(state, query, options).await?;
    Ok(SkinAssets {
        skin,
        back,
        display_name: None,
    })
}

async fn back_texture_from_query(
    state: &AppState,
    query: &UrlQuery,
    options: &RenderOptions,
) -> ApiResult<Option<fleece_render_core::Texture>> {
    if options.is_avatar() {
        return Ok(None);
    }
    let equipment = query
        .back_equipment
        .as_deref()
        .map(BackEquipment::from_str)
        .transpose()?
        .unwrap_or_default();
    let cape = query.cape_data.as_deref().or(query.cape_url.as_deref());
    let elytra = query.elytra_data.as_deref().or(query.elytra_url.as_deref());
    let source = match equipment {
        BackEquipment::Cape => cape.or(elytra),
        BackEquipment::Elytra => elytra.or(cape),
    };
    match source {
        Some(source) => Ok(Some(state.texture(source, "back").await?)),
        None => Ok(None),
    }
}

async fn assets_from_profile(
    state: &AppState,
    profile: &PlayerProfile,
    options: &RenderOptions,
    use_cape: bool,
) -> ApiResult<SkinAssets> {
    let skin_future = state.skin_texture(&profile.skin_url, options);
    let back_future = async {
        if !options.is_avatar() && use_cape {
            match &profile.cape_url {
                Some(url) => Ok::<_, ApiError>(Some(state.texture(url, "back").await?)),
                None => Ok::<_, ApiError>(None),
            }
        } else {
            Ok::<_, ApiError>(None)
        }
    };
    let (skin, back) = tokio::join!(skin_future, back_future);
    let skin = skin?;
    let back = back?;
    Ok(SkinAssets {
        skin,
        back,
        display_name: Some(profile.name.clone()),
    })
}

fn apply_back_equipment(options: &mut RenderOptions, value: Option<&str>) -> ApiResult<()> {
    if !options.is_avatar()
        && let Some(value) = value
    {
        options.back_equipment = value.parse::<BackEquipment>()?;
    }
    Ok(())
}

fn parse_parts(value: &str) -> ApiResult<Parts> {
    value.parse::<Parts>().map_err(ApiError::from)
}

fn primary_image(result: &fleece_render_core::RenderOutput) -> ApiResult<&RenderedImage> {
    result
        .primary()
        .ok_or_else(|| ApiError::internal("renderer returned no primary image"))
}

fn png_response(image: &RenderedImage, info: &RenderInfo) -> Response {
    let mut response = Response::new(Body::from(image.bytes.clone()));
    *response.status_mut() = StatusCode::OK;
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("image/png"));
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=300"),
    );
    insert_header(headers, "x-fleece-render-style", &info.style);
    insert_header(headers, "x-fleece-render-parts", &info.parts);
    insert_header(headers, "x-fleece-render-model", &info.model);
    insert_header(
        headers,
        "x-fleece-render-size",
        &format!("{}x{}", info.output_width, info.output_height),
    );
    insert_header(
        headers,
        "x-fleece-render-elapsed",
        &format!("{:.3}", info.elapsed_ms / 1000.0),
    );
    insert_header(headers, "x-fleece-render-backend", &info.backend);
    response
}

fn insert_header(headers: &mut axum::http::HeaderMap, name: &'static str, value: &str) {
    if let Ok(value) = HeaderValue::from_str(value) {
        headers.insert(name, value);
    }
}

fn b64(image: Option<&RenderedImage>) -> String {
    image
        .map(|image| base64::engine::general_purpose::STANDARD.encode(&image.bytes))
        .unwrap_or_default()
}

fn info_json(info: &RenderInfo) -> Value {
    json!({
        "backend": info.backend,
        "style": info.style,
        "model": info.model,
        "parts": info.parts,
        "panel_width": info.panel_width,
        "panel_height": info.panel_height,
        "outputWidth": info.output_width,
        "outputHeight": info.output_height,
        "elapsedMs": info.elapsed_ms,
        "triangles": info.triangles,
        "cached": info.cached,
    })
}

#[cfg(test)]
mod tests;
