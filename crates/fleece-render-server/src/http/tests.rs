use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use base64::Engine as _;
use serde_json::json;
use std::time::Instant;
use tower::ServiceExt;

use super::build_router;
use crate::{AppConfig, AppState, CachedProfile, PlayerProfile};

#[tokio::test]
async fn avatar_player_sources_match_the_same_raw_skin() {
    const NAME: &str = "AvatarTest";
    const UUID: &str = "1234567890abcdef1234567890abcdef";
    const SKIN: &[u8] = include_bytes!("../../tests/fixtures/source_skin.png");

    let mut config = AppConfig::default();
    config.render.backend = "cpu".to_owned();
    let state = AppState::new(config).unwrap();
    let encoded = base64::engine::general_purpose::STANDARD.encode(SKIN);
    // Stand in for successful upstream lookups without depending on live accounts or networks.
    for source in ["mojang", "littleskin"] {
        let profile = PlayerProfile {
            uuid: UUID.to_owned(),
            name: NAME.to_owned(),
            skin_url: format!("data:image/png;base64,{encoded}"),
            cape_url: Some("unavailable-cape.png".to_owned()),
            slim: false,
            source: source.to_owned(),
            raw: json!({}),
        };
        for lookup in [NAME, UUID] {
            state.profile_cache.lock().unwrap().insert(
                format!("{source}:{}", lookup.to_ascii_lowercase()),
                CachedProfile {
                    fetched_at: Instant::now(),
                    profile: profile.clone(),
                },
            );
        }
    }
    let app = build_router(state);
    let png = |request: Request<Body>| {
        let app = app.clone();
        async move {
            let response = app.oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(response.headers()["x-fleece-render-size"], "64x64");
            to_bytes(response.into_body(), 65_536).await.unwrap()
        }
    };
    for (transparent, show_outer_layer) in
        [(true, true), (false, true), (true, false), (false, false)]
    {
        let query =
            format!("style=avatar&transparent={transparent}&showOuterLayer={show_outer_layer}");
        let expected = png(Request::post(format!("/raw/image/front?{query}"))
            .body(Body::from(SKIN))
            .unwrap())
        .await;
        for source in ["mojang", "littleskin"] {
            for lookup in [NAME, UUID] {
                let request = Request::get(format!("/{source}/image/front?{query}&name={lookup}"))
                    .body(Body::empty())
                    .unwrap();
                assert_eq!(png(request).await, expected, "{source} {lookup}");
            }
            let request = Request::post("/api/v1/render/front?format=png")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "style": "avatar",
                        "transparent": transparent,
                        "showOuterLayer": show_outer_layer,
                        "player": {"uuid": UUID, "source": source}
                    })
                    .to_string(),
                ))
                .unwrap();
            assert_eq!(png(request).await, expected, "v1 {source}");
        }
    }
}
