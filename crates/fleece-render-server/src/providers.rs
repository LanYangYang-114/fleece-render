use std::time::Duration;

use base64::Engine as _;
use serde_json::{Value, json};
use url::form_urlencoded;

use crate::config::AppConfig;
use crate::error::{ApiError, ApiResult};

#[derive(Clone, Debug)]
pub struct PlayerProfile {
    pub uuid: String,
    pub name: String,
    pub skin_url: String,
    pub cape_url: Option<String>,
    pub slim: bool,
    pub source: String,
    pub raw: Value,
}

impl PlayerProfile {
    pub fn model(&self) -> &'static str {
        if self.slim { "slim" } else { "classic" }
    }

    pub fn summary(&self) -> Value {
        json!({
            "uuid": self.uuid,
            "uuidDashed": dashed_uuid(&self.uuid),
            "name": self.name,
            "source": self.source,
            "slim": self.slim,
            "model": self.model(),
            "skinUrl": self.skin_url,
            "capeUrl": self.cape_url,
            "hasCape": self.cape_url.is_some(),
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProfileSource {
    Mojang,
    LittleSkin,
}

impl ProfileSource {
    pub fn parse(value: &str) -> ApiResult<Self> {
        match value.to_ascii_lowercase().as_str() {
            "mojang" | "official" => Ok(Self::Mojang),
            "littleskin" | "little-skin" => Ok(Self::LittleSkin),
            other => Err(ApiError::invalid(format!(
                "unknown profile source {other:?}; expected mojang or littleskin"
            ))),
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::Mojang => "mojang",
            Self::LittleSkin => "littleskin",
        }
    }
}

pub async fn fetch_profile(
    client: &reqwest::Client,
    config: &AppConfig,
    source: ProfileSource,
    player: &str,
) -> ApiResult<PlayerProfile> {
    let uuid = normalize_uuid(player);
    let uuid = match uuid {
        Some(uuid) => uuid,
        None => {
            let name = player.trim();
            validate_player_name(name)?;
            match source {
                ProfileSource::Mojang => mojang_name_to_uuid(client, config, name).await?,
                ProfileSource::LittleSkin => littleskin_name_to_uuid(client, config, name).await?,
            }
        }
    };
    let response = match source {
        ProfileSource::Mojang => {
            get_json(
                client,
                config,
                &format!("https://sessionserver.mojang.com/session/minecraft/profile/{uuid}"),
            )
            .await?
        }
        ProfileSource::LittleSkin => get_json(
            client,
            config,
            &format!(
                "https://littleskin.cn/api/yggdrasil/sessionserver/session/minecraft/profile/{uuid}"
            ),
        )
        .await?,
    };
    let response = response.ok_or_else(|| {
        ApiError::player_not_found(format!("{} has no profile for UUID {uuid}", source.id()))
    })?;
    profile_from_response(&response, source, player)
}

async fn mojang_name_to_uuid(
    client: &reqwest::Client,
    config: &AppConfig,
    name: &str,
) -> ApiResult<String> {
    let encoded: String = form_urlencoded::byte_serialize(name.as_bytes()).collect();
    let response = get_json(
        client,
        config,
        &format!("https://api.mojang.com/users/profiles/minecraft/{encoded}"),
    )
    .await?;
    let value = response.ok_or_else(|| {
        ApiError::player_not_found(format!("Mojang has no player named {name:?}"))
    })?;
    let uuid = value
        .get("id")
        .and_then(Value::as_str)
        .and_then(normalize_uuid)
        .ok_or_else(|| ApiError::upstream("Mojang returned a malformed UUID"))?;
    Ok(uuid)
}

async fn littleskin_name_to_uuid(
    client: &reqwest::Client,
    config: &AppConfig,
    name: &str,
) -> ApiResult<String> {
    let response = client
        .post("https://littleskin.cn/api/yggdrasil/api/profiles/minecraft")
        .json(&json!([name]))
        .timeout(profile_timeout(config))
        .send()
        .await
        .map_err(|error| ApiError::upstream(format!("LittleSkin lookup failed: {error}")))?;
    if response.status() == reqwest::StatusCode::NO_CONTENT
        || response.status() == reqwest::StatusCode::NOT_FOUND
    {
        return Err(ApiError::player_not_found(format!(
            "LittleSkin has no player named {name:?}"
        )));
    }
    if !response.status().is_success() {
        return Err(ApiError::upstream(format!(
            "LittleSkin lookup failed: HTTP {}",
            response.status()
        )));
    }
    let value: Value =
        serde_json::from_slice(&read_profile_body(response).await?).map_err(|error| {
            ApiError::upstream(format!("LittleSkin returned invalid JSON: {error}"))
        })?;
    let entry = value
        .as_array()
        .and_then(|items| items.first())
        .or_else(|| value.as_object().map(|_| &value))
        .ok_or_else(|| {
            ApiError::player_not_found(format!("LittleSkin has no player named {name:?}"))
        })?;
    entry
        .get("id")
        .and_then(Value::as_str)
        .and_then(normalize_uuid)
        .ok_or_else(|| ApiError::upstream("LittleSkin returned a malformed UUID"))
}

const MAX_PROFILE_BYTES: usize = 1024 * 1024;

async fn read_profile_body(response: reqwest::Response) -> ApiResult<Vec<u8>> {
    crate::input::read_response_limited(response, "profile response", MAX_PROFILE_BYTES, || {
        ApiError::upstream("profile response exceeds the 1 MiB limit")
    })
    .await
}

async fn get_json(
    client: &reqwest::Client,
    config: &AppConfig,
    url: &str,
) -> ApiResult<Option<Value>> {
    let response = client
        .get(url)
        .header("Accept", "application/json")
        .timeout(profile_timeout(config))
        .send()
        .await
        .map_err(|error| ApiError::upstream(format!("profile request failed: {error}")))?;
    if response.status() == reqwest::StatusCode::NO_CONTENT
        || response.status() == reqwest::StatusCode::NOT_FOUND
        || response.status() == reqwest::StatusCode::BAD_REQUEST
    {
        return Ok(None);
    }
    if !response.status().is_success() {
        return Err(ApiError::upstream(format!(
            "profile request returned HTTP {}",
            response.status()
        )));
    }
    let bytes = read_profile_body(response).await?;
    if bytes.is_empty() {
        return Ok(None);
    }
    let value = serde_json::from_slice(&bytes)
        .map_err(|error| ApiError::upstream(format!("profile response is not JSON: {error}")))?;
    Ok(Some(value))
}

fn profile_from_response(
    response: &Value,
    source: ProfileSource,
    fallback_name: &str,
) -> ApiResult<PlayerProfile> {
    let uuid = response
        .get("id")
        .and_then(Value::as_str)
        .and_then(normalize_uuid)
        .ok_or_else(|| ApiError::upstream("profile response has no valid UUID"))?;
    let name = response
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or(fallback_name)
        .to_owned();
    let textures_property = response
        .get("properties")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|property| property.get("name").and_then(Value::as_str) == Some("textures"))
        .and_then(|property| property.get("value"))
        .and_then(Value::as_str)
        .ok_or_else(|| ApiError::upstream("profile has no textures property"))?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(textures_property.as_bytes())
        .map_err(|error| {
            ApiError::upstream(format!("textures property is invalid base64: {error}"))
        })?;
    let textures_root: Value = serde_json::from_slice(&decoded).map_err(|error| {
        ApiError::upstream(format!("textures property is invalid JSON: {error}"))
    })?;
    let textures = textures_root
        .get("textures")
        .and_then(Value::as_object)
        .ok_or_else(|| ApiError::upstream("textures property has no textures object"))?;
    let skin = textures
        .get("SKIN")
        .and_then(Value::as_object)
        .ok_or_else(|| ApiError::upstream("profile has no SKIN texture"))?;
    let skin_url = skin
        .get("url")
        .and_then(Value::as_str)
        .ok_or_else(|| ApiError::upstream("SKIN texture has no URL"))?
        .to_owned();
    let cape_url = textures
        .get("CAPE")
        .and_then(Value::as_object)
        .and_then(|cape| cape.get("url"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    let slim = skin
        .get("metadata")
        .and_then(Value::as_object)
        .and_then(|metadata| metadata.get("model"))
        .and_then(Value::as_str)
        == Some("slim");
    Ok(PlayerProfile {
        uuid,
        name,
        skin_url,
        cape_url,
        slim,
        source: source.id().to_owned(),
        raw: textures_root,
    })
}

fn profile_timeout(config: &AppConfig) -> Duration {
    Duration::from_secs_f64(config.network.profile_timeout_seconds)
}

pub fn normalize_uuid(value: &str) -> Option<String> {
    let compact: String = value
        .trim()
        .chars()
        .filter(|character| *character != '-')
        .collect::<String>()
        .to_ascii_lowercase();
    if compact.len() == 32
        && compact
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        Some(compact)
    } else {
        None
    }
}

pub fn dashed_uuid(uuid: &str) -> String {
    if uuid.len() == 32 {
        format!(
            "{}-{}-{}-{}-{}",
            &uuid[0..8],
            &uuid[8..12],
            &uuid[12..16],
            &uuid[16..20],
            &uuid[20..32]
        )
    } else {
        uuid.to_owned()
    }
}

pub fn validate_player_name(name: &str) -> ApiResult<()> {
    let length = name.chars().count();
    if (3..=16).contains(&length)
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        Ok(())
    } else {
        Err(ApiError::invalid(format!(
            "invalid player name {name:?}; expected 3-16 ASCII letters, digits, or underscores"
        )))
    }
}
