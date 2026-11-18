use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use base64::Engine as _;
use fleece_render_server::{AppConfig, AppState, build_router};
use serde_json::{Value, json};
use tower::ServiceExt;

const SKIN: &[u8] = include_bytes!("fixtures/source_skin.png");

fn app() -> axum::Router {
    let mut config = AppConfig::default();
    config.render.definition = 0.25;
    config.render.supersample = 1;
    config.render.backend = "cpu".to_owned();
    build_router(AppState::new(config).unwrap())
}

async fn png(app: &axum::Router, request: Request<Body>) -> Vec<u8> {
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["content-type"], "image/png");
    to_bytes(response.into_body(), 1_048_576)
        .await
        .unwrap()
        .to_vec()
}

#[tokio::test]
async fn boolean_query_aliases_preserve_rendered_png_bytes() {
    let app = app();
    let encoded = base64::engine::general_purpose::STANDARD.encode(SKIN);
    let skin_query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("skinData", &encoded)
        .finish();
    for field in ["transparent", "showOuterLayer", "showName"] {
        let params = |value: &str| format!("nameTag=QueryTest&{field}={value}");
        let raw_request = |value: &str| {
            Request::post(format!("/raw/image/front?{}", params(value)))
                .body(Body::from(SKIN))
                .unwrap()
        };
        let expected_true = png(&app, raw_request("true")).await;
        let expected_false = png(&app, raw_request("false")).await;
        assert_ne!(
            expected_true, expected_false,
            "{field} must affect the image"
        );
        for (value, expected) in [
            ("true", true),
            ("True", true),
            ("TRUE", true),
            ("1", true),
            ("yes", true),
            ("YeS", true),
            ("on", true),
            ("ON", true),
            ("t", true),
            ("T", true),
            ("y", true),
            ("Y", true),
            ("false", false),
            ("False", false),
            ("FALSE", false),
            ("0", false),
            ("no", false),
            ("nO", false),
            ("off", false),
            ("OFF", false),
            ("f", false),
            ("F", false),
            ("n", false),
            ("N", false),
        ] {
            let expected = if expected {
                &expected_true
            } else {
                &expected_false
            };
            assert_eq!(
                &png(&app, raw_request(value)).await,
                expected,
                "raw {field}={value}"
            );
            let request = Request::get(format!("/url/image/front?{skin_query}&{}", params(value)))
                .body(Body::empty())
                .unwrap();
            assert_eq!(&png(&app, request).await, expected, "url {field}={value}");
        }
    }
}

#[tokio::test]
async fn query_rejections_are_structured_400_on_all_render_routes() {
    let app = app();
    for (method, path) in [
        ("GET", "/url/image/front?transparent=maybe"),
        ("GET", "/url/image/front?showOuterLayer=2"),
        ("GET", "/url/image/front?showName="),
        ("GET", "/url/image/front?definition=oops"),
        ("GET", "/url/image/front?timeout=oops"),
        ("GET", "/url/image/front?transparent=true&transparent=false"),
        ("GET", "/url/json/all?transparent=maybe"),
        ("POST", "/raw/image/front?showName=maybe"),
        ("GET", "/mojang/image/front"),
        ("GET", "/littleskin/image/front"),
        ("GET", "/mojang/image/front?name=Test&useCape=maybe"),
        ("GET", "/littleskin/image/front?name=Test&useCape=2"),
        ("GET", "/mojang/image/front?name=Test&name=Other"),
        ("POST", "/api/v1/render/front?format=png&format=json"),
        // Business validation already used JSON; its contract must also remain unchanged.
        ("GET", "/url/image/front"),
        ("GET", "/url/image/invalid"),
        ("GET", "/url/image/front?definition=6"),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .header("content-type", "application/json")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{path}");
        assert_eq!(
            response.headers()["content-type"],
            "application/json",
            "{path}"
        );
        let body = to_bytes(response.into_body(), 65_536).await.unwrap();
        let value: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value.as_object().unwrap().len(), 3, "{path}");
        assert_eq!(value["code"], "invalid_request", "{path}");
        assert!(!value["message"].as_str().unwrap().is_empty(), "{path}");
        assert_eq!(value["details"], json!({}), "{path}");
    }
}

#[tokio::test]
async fn legacy_json_response_keeps_its_current_metadata() {
    let app = app();
    let encoded = base64::engine::general_purpose::STANDARD.encode(SKIN);
    let query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("skinData", &encoded)
        .append_pair("transparent", "YES")
        .append_pair("showOuterLayer", "0")
        .append_pair("showName", "OFF")
        .finish();
    let response = app
        .oneshot(
            Request::get(format!("/url/json/all?{query}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 1_048_576).await.unwrap();
    let value: Value = serde_json::from_slice(&body).unwrap();
    for field in ["front_b64", "back_b64", "both_b64"] {
        let image = base64::engine::general_purpose::STANDARD
            .decode(value[field].as_str().unwrap())
            .unwrap();
        assert!(image.starts_with(b"\x89PNG\r\n\x1a\n"));
    }
    assert_eq!(value["backendInfo"]["model"], "classic");
    assert!(value["backendInfo"].get("slim").is_none());
    assert!(value["backendInfo"].get("serverTimeJs").is_none());
}
