use axum::body::Body;
use axum::http::{Request, StatusCode};
use base64::Engine as _;
use fleece_render_server::{AppConfig, AppState, build_router};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

const SKIN: &[u8] = include_bytes!("fixtures/source_skin.png");

fn app() -> axum::Router {
    build_router(AppState::new(config("cpu")).unwrap())
}

fn config(backend: &str) -> AppConfig {
    let mut config = AppConfig::default();
    config.render.definition = 0.5;
    config.render.supersample = 1;
    config.render.backend = backend.to_owned();
    config
}

async fn body_bytes(response: axum::response::Response) -> Vec<u8> {
    response
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes()
        .to_vec()
}

#[tokio::test]
async fn health_and_styles_are_available() {
    let response = app()
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let value: Value = serde_json::from_slice(&body_bytes(response).await).unwrap();
    assert_eq!(value["ok"], true);
    assert_eq!(value["package"]["name"], "fleece-render");
    assert_eq!(value["package"]["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(value["backend"], "cpu");
    assert_eq!(
        value["availableBackends"],
        json!(fleece_render_core::available_backends())
    );
    assert_eq!(value["styles"], json!(["3d", "avatar"]));

    let response = app()
        .oneshot(
            Request::builder()
                .uri("/api/v1/styles")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let value: Value = serde_json::from_slice(&body_bytes(response).await).unwrap();
    assert_eq!(value["styles"][0]["id"], "3d");
    assert_eq!(value["styles"][1]["id"], "avatar");
}

#[tokio::test]
async fn raw_png_endpoint_renders_png_with_metadata_headers() {
    let response = app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/raw/image/both?definition=0.5&showName=false")
                .header("content-type", "image/png")
                .body(Body::from(SKIN.to_vec()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = body_bytes(response).await;
    if status != StatusCode::OK {
        eprintln!(
            "raw status={status} body={}",
            String::from_utf8_lossy(&body)
        );
    }
    assert_eq!(status, StatusCode::OK);
    let response = app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/raw/image/both?definition=0.5&showName=false")
                .header("content-type", "image/png")
                .body(Body::from(SKIN.to_vec()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        response.headers().get("x-fleece-render-model").unwrap(),
        "classic"
    );
    assert_eq!(
        response.headers().get("x-fleece-render-size").unwrap(),
        "350x335"
    );
    let bytes = body_bytes(response).await;
    let image = image::load_from_memory(&bytes).unwrap();
    assert_eq!((image.width(), image.height()), (350, 335));
}

#[tokio::test]
async fn v1_json_endpoint_accepts_inline_base64_texture() {
    let encoded = base64::engine::general_purpose::STANDARD.encode(SKIN);
    let request = json!({
        "skin": {"data": encoded},
        "definition": 0.25,
        "showName": false,
        "transparent": true,
        "model": "auto-detect"
    });
    let response = app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/render/front?format=json")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let value: Value = serde_json::from_slice(&body_bytes(response).await).unwrap();
    assert_eq!(value["ok"], true);
    let image_bytes = base64::engine::general_purpose::STANDARD
        .decode(value["front_b64"].as_str().unwrap())
        .unwrap();
    let image = image::load_from_memory(&image_bytes).unwrap();
    assert_eq!((image.width(), image.height()), (88, 168));
}

#[tokio::test]
async fn unknown_style_returns_structured_400() {
    let response = app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/raw/image/front?definition=0.5&style=missing")
                .header("content-type", "image/png")
                .body(Body::from(SKIN.to_vec()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let value: Value = serde_json::from_slice(&body_bytes(response).await).unwrap();
    assert_eq!(value["code"], "unknown_style");
}

#[tokio::test]
async fn each_skin_layer_mode_is_byte_identical_across_input_sources() {
    check_input_equivalence(app(), "cpu").await;
}

#[cfg(feature = "gpu")]
#[tokio::test]
async fn gpu_outputs_are_byte_identical_across_input_sources() {
    match AppState::new(config("gpu")) {
        Ok(state) => check_input_equivalence(build_router(state), "gpu").await,
        Err(error)
            if std::env::var_os("FLEECE_RENDER_REQUIRE_GPU").is_none()
                && format!("{error:#}").contains("no Vulkan hardware adapter found") =>
        {
            eprintln!("Skipping GPU HTTP test: {error:#}");
        }
        Err(error) => panic!("GPU initialization failed: {error:#}"),
    }
}

async fn check_input_equivalence(app: axum::Router, backend: &str) {
    // A real, local HTTP server exercises the texture downloader without internet access.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let skin_url = format!("http://{}/skin.png", listener.local_addr().unwrap());
    let fixture = axum::Router::new().route(
        "/skin.png",
        axum::routing::get(|| async { ([("content-type", "image/png")], SKIN) }),
    );
    let server = tokio::spawn(async move { axum::serve(listener, fixture).await.unwrap() });
    let tmp = tempfile::tempdir().unwrap();
    let skin_path = tmp.path().join("skin fixture.png");
    std::fs::write(&skin_path, SKIN).unwrap();
    let skin_path = skin_path.to_str().unwrap();
    let encoded = base64::engine::general_purpose::STANDARD.encode(SKIN);
    let data_url = format!("data:image/png;base64,{encoded}");

    for (style, transparent) in [("avatar", true), ("avatar", false), ("3d", true)] {
        let avatar = style == "avatar";
        let size = if avatar { "64x64" } else { "175x335" };
        let mut outputs = Vec::new();
        for show_outer_layer in [true, false] {
            let common = format!("style={style}&transparent={transparent}&showName=false");
            let options = format!("{common}&showOuterLayer={show_outer_layer}");
            let mut inputs = vec![
                (
                    "raw PNG",
                    Request::post(format!("/raw/image/front?{options}"))
                        .header("content-type", "image/png")
                        .body(Body::from(SKIN))
                        .unwrap(),
                ),
                (
                    "raw data URL",
                    Request::post(format!("/raw/image/front?{options}"))
                        .header("content-type", "text/plain")
                        .body(Body::from(data_url.clone()))
                        .unwrap(),
                ),
            ];
            if show_outer_layer {
                inputs.push((
                    "default layer",
                    Request::post(format!("/raw/image/front?{common}"))
                        .body(Body::from(SKIN))
                        .unwrap(),
                ));
            }
            for (label, key, value) in [
                ("HTTP URL", "skinUrl", skin_url.as_str()),
                ("local path", "skinUrl", skin_path),
                ("Base64", "skinData", encoded.as_str()),
                ("data URL", "skinData", data_url.as_str()),
            ] {
                let query = url::form_urlencoded::Serializer::new(String::new())
                    .extend_pairs(url::form_urlencoded::parse(options.as_bytes()))
                    .append_pair(key, value)
                    .finish();
                inputs.push((
                    label,
                    Request::get(format!("/url/image/front?{query}"))
                        .body(Body::empty())
                        .unwrap(),
                ));
            }
            for (label, skin) in [
                ("v1 URL", json!({"url": skin_url})),
                ("v1 path", json!({"path": skin_path})),
                ("v1 Base64", json!({"data": encoded})),
                ("v1 data URL", json!({"data": data_url})),
            ] {
                let mut request = json!({
                    "style": style,
                    "transparent": transparent,
                    "showOuterLayer": show_outer_layer,
                    "showName": false,
                    "skin": skin,
                });
                if avatar {
                    request["definition"] = json!(5);
                    request["showName"] = json!(true);
                    request["nameTag"] = json!("Ignored");
                    request["cape"] = json!({"path": "not-a-real-cape.png"});
                }
                inputs.push((
                    label,
                    Request::post("/api/v1/render/front?format=png")
                        .header("content-type", "application/json")
                        .body(Body::from(request.to_string()))
                        .unwrap(),
                ));
            }

            let mut expected = None;
            for (label, request) in inputs {
                let started = std::time::Instant::now();
                let response = app.clone().oneshot(request).await.unwrap();
                assert_eq!(response.status(), StatusCode::OK, "{label}");
                assert_eq!(response.headers()["content-type"], "image/png", "{label}");
                assert_eq!(response.headers()["x-fleece-render-size"], size, "{label}");
                assert_eq!(
                    response.headers()["x-fleece-render-style"],
                    style,
                    "{label}"
                );
                assert_eq!(
                    response.headers()["x-fleece-render-backend"],
                    if avatar { "cpu" } else { backend },
                    "{label}"
                );
                let bytes = body_bytes(response).await;
                assert!(
                    started.elapsed() < std::time::Duration::from_secs(5),
                    "{label}"
                );
                if let Some(expected) = &expected {
                    assert_eq!(&bytes, expected, "{label}");
                } else {
                    let image = image::load_from_memory(&bytes).unwrap().to_rgba8();
                    assert_eq!(
                        image.dimensions(),
                        if avatar { (64, 64) } else { (175, 335) }
                    );
                    let alpha = if transparent { 0 } else { 255 };
                    assert_eq!(image.get_pixel(0, 0)[3], alpha);
                    assert!(image.pixels().any(|pixel| pixel[3] == 255));
                    if !transparent {
                        assert_eq!(image.get_pixel(0, 0).0, [255; 4]);
                        assert!(image.pixels().all(|pixel| pixel[3] == 255));
                    }
                    expected = Some(bytes);
                }
            }
            outputs.push(expected.unwrap());
        }
        assert_ne!(outputs[0], outputs[1], "{style}, transparent={transparent}");
    }
    server.abort();
}

#[tokio::test]
async fn avatar_supports_v1_json_and_requires_front() {
    let request = |path| {
        Request::post(path)
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "style": "avatar",
                    "skin": {"data": base64::engine::general_purpose::STANDARD.encode(SKIN)}
                })
                .to_string(),
            ))
            .unwrap()
    };
    let app = app();
    let response = app
        .clone()
        .oneshot(request("/api/v1/render/front"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let value: Value = serde_json::from_slice(&body_bytes(response).await).unwrap();
    assert_eq!(value["backendInfo"]["outputWidth"], 64);
    assert_eq!(value["backendInfo"]["outputHeight"], 64);
    assert_eq!(value["back_b64"], "");
    assert_eq!(value["both_b64"], "");
    let png = base64::engine::general_purpose::STANDARD
        .decode(value["front_b64"].as_str().unwrap())
        .unwrap();
    let image = image::load_from_memory(&png).unwrap().to_rgba8();
    assert_eq!(image.get_pixel(0, 0).0, [255; 4]);

    for path in [
        "/api/v1/render/back",
        "/api/v1/render/both",
        "/api/v1/render/all",
    ] {
        let response = app.clone().oneshot(request(path)).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let value: Value = serde_json::from_slice(&body_bytes(response).await).unwrap();
        assert_eq!(value["code"], "invalid_request");
    }
}

const AUTH_TOKEN: &str = "test-Token_123";

fn authenticated_app(allow_cors: bool) -> axum::Router {
    let mut config = config("cpu");
    config.server.auth_token = AUTH_TOKEN.to_owned();
    config.server.allow_cors = allow_cors;
    build_router(AppState::new(config).unwrap())
}

#[tokio::test]
async fn bearer_auth_protects_all_paths_except_health() {
    use axum::http::header;

    let app = authenticated_app(true);
    for (method, path) in [
        ("GET", "/"),
        ("HEAD", "/"),
        ("GET", "/url/image/front"),
        ("GET", "/url/json/all"),
        ("GET", "/mojang/image/front"),
        ("GET", "/littleskin/image/front"),
        ("GET", "/api/v1"),
        ("GET", "/api/v1/styles"),
        ("POST", "/api/v1/render/front"),
        ("POST", "/raw/image/front"),
        ("PUT", "/"),
        ("GET", "/unknown"),
        ("GET", "/health/"),
        ("OPTIONS", "/api/v1/styles"),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .header(header::ORIGIN, "https://client.example")
                    .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
                    .header(header::ACCESS_CONTROL_REQUEST_HEADERS, "authorization")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "{method} {path}"
        );
        assert_eq!(
            response.headers()[header::WWW_AUTHENTICATE],
            "Bearer realm=\"fleece-render\""
        );
        if method != "HEAD" {
            let body: Value = serde_json::from_slice(&body_bytes(response).await).unwrap();
            assert_eq!(body["code"], "unauthorized");
        }
    }

    for (path, authorization, expected) in [
        ("/health", None, StatusCode::OK),
        ("/health?probe=true", Some("Bearer wrong"), StatusCode::OK),
        (
            "/api/v1/styles",
            Some("bEaReR  test-Token_123"),
            StatusCode::OK,
        ),
        (
            "/unknown",
            Some("Bearer test-Token_123"),
            StatusCode::NOT_FOUND,
        ),
    ] {
        let mut request = Request::builder().uri(path);
        if let Some(value) = authorization {
            request = request.header(header::AUTHORIZATION, value);
        }
        let response = app
            .clone()
            .oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), expected, "{path}");
    }
}

#[tokio::test]
async fn bearer_auth_rejects_bad_credentials_before_processing_the_body() {
    use axum::http::{HeaderValue, header};

    let app = authenticated_app(false);
    for values in [
        vec![],
        vec![HeaderValue::from_static("Bearer")],
        vec![HeaderValue::from_static("Bearer wrong")],
        vec![HeaderValue::from_static("Basic test-Token_123")],
        vec![HeaderValue::from_static("Bearer test-token_123")],
        vec![HeaderValue::from_static("Bearer test-Token_123 extra")],
        vec![HeaderValue::from_bytes(b"Bearer \xff").unwrap()],
        vec![HeaderValue::from_static("Bearer test-Token_123"); 2],
    ] {
        let mut request = Request::builder()
            .method("POST")
            .uri("/raw/image/front?token=test-Token_123")
            .header(header::CONTENT_LENGTH, "16777217");
        for value in values {
            request = request.header(header::AUTHORIZATION, value);
        }
        let response = app
            .clone()
            .oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}

#[tokio::test]
async fn bearer_auth_preserves_rendered_image_bytes() {
    use axum::http::header;

    let request = || {
        Request::builder()
            .method("POST")
            .uri("/raw/image/front?style=avatar&transparent=true")
            .header(header::CONTENT_TYPE, "image/png")
    };
    let without_auth = app()
        .oneshot(request().body(Body::from(SKIN)).unwrap())
        .await
        .unwrap();
    let with_auth = authenticated_app(false)
        .oneshot(
            request()
                .header(header::AUTHORIZATION, format!("Bearer {AUTH_TOKEN}"))
                .body(Body::from(SKIN))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(without_auth.status(), StatusCode::OK);
    assert_eq!(with_auth.status(), StatusCode::OK);
    assert_eq!(
        without_auth.headers()[header::CACHE_CONTROL],
        "public, max-age=300"
    );
    assert_eq!(with_auth.headers()[header::CACHE_CONTROL], "no-store");
    assert_eq!(body_bytes(with_auth).await, body_bytes(without_auth).await);
}

#[tokio::test]
async fn cross_origin_access_requires_opt_in() {
    use axum::http::header::{ACCESS_CONTROL_ALLOW_ORIGIN, ORIGIN};

    assert_eq!(AppConfig::default().server.host, "127.0.0.1");
    for allow_cors in [false, true] {
        let mut config = config("cpu");
        config.server.allow_cors = allow_cors;
        let response = build_router(AppState::new(config).unwrap())
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .header(ORIGIN, "https://client.example")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().contains_key(ACCESS_CONTROL_ALLOW_ORIGIN),
            allow_cors
        );
    }
}
