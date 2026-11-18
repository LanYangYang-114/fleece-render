use axum::extract::{FromRequestParts, Query};
use axum::http::request::Parts;
use serde::de::{self, DeserializeOwned};
use serde::{Deserialize, Deserializer};

use crate::error::ApiError;

/// Keep query extraction failures in the same JSON envelope as business errors.
pub(super) struct ApiQuery<T>(pub T);

impl<T, S> FromRequestParts<S> for ApiQuery<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        Query::<T>::from_request_parts(parts, state)
            .await
            .map(|Query(value)| Self(value))
            .map_err(|rejection| {
                ApiError::new(rejection.status(), "invalid_request", rejection.body_text())
            })
    }
}

/// Match the legacy API's case-insensitive boolean spellings in query strings.
/// JSON request fields intentionally continue to require JSON booleans.
pub(super) fn optional_bool<'de, D>(deserializer: D) -> Result<Option<bool>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    match value.to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" | "t" | "y" => Ok(Some(true)),
        "false" | "0" | "no" | "off" | "f" | "n" => Ok(Some(false)),
        // Do not echo arbitrary query values into the error response.
        _ => Err(de::Error::custom(
            "expected a boolean: true/false, 1/0, yes/no, on/off, t/f, or y/n (case-insensitive)",
        )),
    }
}
