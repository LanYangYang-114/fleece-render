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
                let profile_query = format!(
                    "style=avatar&transparent={}&showOuterLayer={}&showName=YES&useCape=OFF",
                    if transparent { "YeS" } else { "0" },
                    if show_outer_layer { "on" } else { "N" },
                );
                let request = Request::get(format!(
                    "/{source}/image/front?{profile_query}&name={lookup}"
                ))
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

    let baseline = app
        .clone()
        .oneshot(
            Request::post("/raw/image/front?definition=0.25&showName=false")
                .body(Body::from(SKIN))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(baseline.status(), StatusCode::OK);
    let expected = to_bytes(baseline.into_body(), 65_536).await.unwrap();
    for source in ["mojang", "littleskin"] {
        for value in ["0", "NO", "Off", "F", "n"] {
            // The cached cape is deliberately unavailable: a false alias must skip it.
            let response = app.clone().oneshot(
                Request::get(format!(
                    "/{source}/image/front?name={NAME}&definition=0.25&showName=0&useCape={value}"
                )).body(Body::empty()).unwrap(),
            ).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK, "{source} {value}");
            assert_eq!(
                to_bytes(response.into_body(), 65_536).await.unwrap(),
                expected
            );
        }
    }
}

#[test]
fn boolean_query_fields_accept_legacy_spellings_and_preserve_missing_values() {
    use super::{ProfileQuery, UrlQuery};
    use axum::extract::Query;

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
        let uri = format!(
            "/?name=Test&transparent={value}&showOuterLayer={value}&showName={value}&useCape={value}"
        ).parse().unwrap();
        let Query(url) = Query::<UrlQuery>::try_from_uri(&uri).unwrap();
        assert_eq!(url.transparent, Some(expected), "{value}");
        assert_eq!(url.show_outer_layer, Some(expected), "{value}");
        assert_eq!(url.show_name, Some(expected), "{value}");
        let Query(profile) = Query::<ProfileQuery>::try_from_uri(&uri).unwrap();
        assert_eq!(profile.transparent, Some(expected), "{value}");
        assert_eq!(profile.show_outer_layer, Some(expected), "{value}");
        assert_eq!(profile.show_name, Some(expected), "{value}");
        assert_eq!(profile.use_cape, Some(expected), "{value}");
    }
    let uri = "/?name=Test".parse().unwrap();
    let Query(url) = Query::<UrlQuery>::try_from_uri(&uri).unwrap();
    let Query(profile) = Query::<ProfileQuery>::try_from_uri(&uri).unwrap();
    assert!(url.transparent.is_none());
    assert!(url.show_outer_layer.is_none());
    assert!(url.show_name.is_none());
    assert!(profile.transparent.is_none());
    assert!(profile.show_outer_layer.is_none());
    assert!(profile.show_name.is_none());
    assert!(profile.use_cape.is_none());

    for value in ["", "2", "-1", "maybe", "null", "true%20", "%20yes"] {
        for field in ["transparent", "showOuterLayer", "showName", "useCape"] {
            let uri = format!("/?name=Test&{field}={value}").parse().unwrap();
            assert!(
                Query::<ProfileQuery>::try_from_uri(&uri).is_err(),
                "{field}={value}"
            );
        }
    }
}

#[test]
fn query_boolean_aliases_do_not_change_v1_json_types() {
    use super::V1Request;

    for field in ["transparent", "showOuterLayer", "showName"] {
        for value in [json!("yes"), json!("false"), json!(1), json!(0)] {
            let mut request = json!({});
            request[field] = value;
            assert!(
                serde_json::from_value::<V1Request>(request).is_err(),
                "{field}"
            );
        }
    }
}
