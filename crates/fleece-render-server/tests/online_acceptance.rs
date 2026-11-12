use fleece_render_server::{AppState, ProfileSource, fetch_profile};

#[tokio::test]
#[ignore = "requires direct access to Mojang APIs; run with cargo test -- --ignored"]
async fn mojang_name_and_uuid_resolve_to_same_player() {
    let state = AppState::new(fleece_render_server::AppConfig::default()).unwrap();
    let by_name = fetch_profile(
        &state.client,
        &state.config,
        ProfileSource::Mojang,
        "LanYangYang_CN",
    )
    .await
    .unwrap();
    let by_uuid = fetch_profile(
        &state.client,
        &state.config,
        ProfileSource::Mojang,
        "cd78ace69f64479182d3718446d09157",
    )
    .await
    .unwrap();
    assert_eq!(by_name.uuid, "cd78ace69f64479182d3718446d09157");
    assert_eq!(by_uuid.uuid, by_name.uuid);
    assert_eq!(by_uuid.name, by_name.name);
    assert!(!by_name.skin_url.is_empty());
}
