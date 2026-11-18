use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use fleece_render_server::{AppConfig, AppState, build_router};
use serde_json::{Value, json};
use tower::ServiceExt;

fn app(config: AppConfig) -> axum::Router {
    build_router(AppState::new(config).unwrap())
}

async fn assert_error(response: axum::response::Response, status: StatusCode, code: &str) -> Value {
    assert_eq!(response.status(), status);
    assert_eq!(response.headers()[header::CONTENT_TYPE], "application/json");
    let bytes = to_bytes(response.into_body(), 65_536).await.unwrap();
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value.as_object().unwrap().len(), 3);
    assert_eq!(value["code"], code);
    assert!(!value["message"].as_str().unwrap().is_empty());
    assert!(value["details"].is_object());
    value
}

#[tokio::test]
async fn framework_rejections_keep_distinct_statuses_and_one_error_schema() {
    let app = app(AppConfig::default());
    for (method, path, content_type, body, status, code) in [
        (
            "GET",
            "/missing",
            None,
            "",
            StatusCode::NOT_FOUND,
            "not_found",
        ),
        ("GET", "/docs", None, "", StatusCode::NOT_FOUND, "not_found"),
        (
            "POST",
            "/",
            None,
            "",
            StatusCode::METHOD_NOT_ALLOWED,
            "method_not_allowed",
        ),
        (
            "GET",
            "/url/image/%FF",
            None,
            "",
            StatusCode::BAD_REQUEST,
            "invalid_request",
        ),
        (
            "POST",
            "/api/v1/render/front",
            Some("application/json"),
            "{",
            StatusCode::BAD_REQUEST,
            "invalid_request",
        ),
        (
            "POST",
            "/api/v1/render/front",
            Some("application/json"),
            r#"{"transparent":"yes"}"#,
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_request",
        ),
        (
            "POST",
            "/api/v1/render/front",
            Some("application/json"),
            r#"{"definition":"secret-value"}"#,
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_request",
        ),
        (
            "POST",
            "/api/v1/render/front",
            None,
            "{}",
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "unsupported_media_type",
        ),
    ] {
        let mut builder = Request::builder().method(method).uri(path);
        if let Some(content_type) = content_type {
            builder = builder.header(header::CONTENT_TYPE, content_type);
        }
        let response = app
            .clone()
            .oneshot(builder.body(Body::from(body)).unwrap())
            .await
            .unwrap();
        if status == StatusCode::METHOD_NOT_ALLOWED {
            assert!(
                response.headers()[header::ALLOW]
                    .to_str()
                    .unwrap()
                    .contains("GET")
            );
        }
        let value = assert_error(response, status, code).await;
        assert_eq!(value["details"], json!({}));
        assert!(!value.to_string().contains("secret-value"));
    }
}

#[tokio::test]
async fn both_body_limits_return_json_413() {
    // The outer limit can reject Content-Length without reading the body.
    let mut config = AppConfig::default();
    config.server.body_limit_bytes = 32;
    let limited = app(config);
    for path in ["/raw/image/front", "/api/v1/render/front"] {
        let response = limited
            .clone()
            .oneshot(
                Request::post(path)
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(header::CONTENT_LENGTH, "33")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_error(response, StatusCode::PAYLOAD_TOO_LARGE, "payload_too_large").await;
    }
    // Axum's default body limit is stricter than the configured global default.
    let default_app = app(AppConfig::default());
    for path in ["/raw/image/front", "/api/v1/render/front"] {
        let response = default_app
            .clone()
            .oneshot(
                Request::post(path)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(vec![b'x'; 2 * 1024 * 1024 + 1]))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_error(response, StatusCode::PAYLOAD_TOO_LARGE, "payload_too_large").await;
    }
}

#[tokio::test]
async fn error_normalization_preserves_auth_cors_and_cache_headers() {
    let mut config = AppConfig::default();
    config.server.auth_token = "test-Token_123".to_owned();
    config.server.allow_cors = true;
    config.server.body_limit_bytes = 32;
    let app = app(config);
    for (path, oversized, status, code) in [
        ("/missing", false, StatusCode::NOT_FOUND, "not_found"),
        (
            "/raw/image/front",
            true,
            StatusCode::PAYLOAD_TOO_LARGE,
            "payload_too_large",
        ),
        (
            "/url/image/front?transparent=maybe",
            false,
            StatusCode::BAD_REQUEST,
            "invalid_request",
        ),
    ] {
        let mut request = Request::builder()
            .method(if oversized { "POST" } else { "GET" })
            .uri(path)
            .header(header::AUTHORIZATION, "Bearer test-Token_123")
            .header(header::ORIGIN, "https://client.example");
        if oversized {
            request = request.header(header::CONTENT_LENGTH, "33");
        }
        let response = app
            .clone()
            .oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        assert_eq!(response.headers()[header::ACCESS_CONTROL_ALLOW_ORIGIN], "*");
        assert_error(response, status, code).await;
    }
    // Authentication must still run before parameter parsing and body limits.
    let response = app
        .oneshot(
            Request::post("/raw/image/front?transparent=maybe")
                .header(header::CONTENT_LENGTH, "33")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    assert_eq!(
        response.headers()[header::WWW_AUTHENTICATE],
        "Bearer realm=\"fleece-render\""
    );
    assert_error(response, StatusCode::UNAUTHORIZED, "unauthorized").await;
}
