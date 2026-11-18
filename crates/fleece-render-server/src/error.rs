use axum::Json;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use fleece_render_core::CoreError;
use serde_json::{Value, json};

#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
    pub details: Value,
}

pub type ApiResult<T> = Result<T, ApiError>;

// An internal marker avoids guessing whether an arbitrary JSON body has our schema.
#[derive(Clone)]
struct ErrorEnvelope;

/// Normalize framework errors without changing status codes or protocol headers.
/// Drop the old body without reading it, including for rejected oversized requests.
pub(crate) fn normalize_http_error(mut response: Response) -> Response {
    let status = response.status();
    if !(status.is_client_error() || status.is_server_error())
        || response.extensions().get::<ErrorEnvelope>().is_some()
    {
        return response;
    }
    let code = match status {
        StatusCode::BAD_REQUEST | StatusCode::UNPROCESSABLE_ENTITY => "invalid_request",
        StatusCode::UNAUTHORIZED => "unauthorized",
        StatusCode::FORBIDDEN => "forbidden",
        StatusCode::NOT_FOUND => "not_found",
        StatusCode::METHOD_NOT_ALLOWED => "method_not_allowed",
        StatusCode::PAYLOAD_TOO_LARGE => "payload_too_large",
        StatusCode::UNSUPPORTED_MEDIA_TYPE => "unsupported_media_type",
        StatusCode::REQUEST_TIMEOUT | StatusCode::GATEWAY_TIMEOUT => "timeout",
        StatusCode::TOO_MANY_REQUESTS => "rate_limited",
        _ if status.is_server_error() => "internal_error",
        _ => "http_error",
    };
    // Do not expose raw framework diagnostics or request content in fallback errors.
    let error = ApiError::new(
        status,
        code,
        status.canonical_reason().unwrap_or("HTTP request failed"),
    );
    let (parts, body) = error.into_response().into_parts();
    // Preserve CORS, authentication, caching, Allow, and other protocol headers,
    // but remove metadata that described the replaced response body.
    for name in [
        header::CONTENT_TYPE,
        header::CONTENT_LENGTH,
        header::CONTENT_ENCODING,
        header::CONTENT_RANGE,
        header::TRANSFER_ENCODING,
        header::ETAG,
        header::LAST_MODIFIED,
    ] {
        response.headers_mut().remove(name);
    }
    response.headers_mut().extend(parts.headers);
    response.extensions_mut().extend(parts.extensions);
    *response.body_mut() = body;
    response
}

impl ApiError {
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
            details: json!({}),
        }
    }

    pub fn with_details(mut self, details: Value) -> Self {
        self.details = details;
        self
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "invalid_request", message)
    }

    pub fn texture(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "texture", message)
    }

    pub fn player_not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, "player_not_found", message)
    }

    pub fn timeout(message: impl Into<String>) -> Self {
        Self::new(StatusCode::GATEWAY_TIMEOUT, "timeout", message)
    }

    pub fn upstream(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_GATEWAY, "upstream", message)
    }

    pub fn unavailable(message: impl Into<String>) -> Self {
        Self::new(StatusCode::SERVICE_UNAVAILABLE, "backend", message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", message)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut response = (
            self.status,
            Json(json!({
                "code": self.code,
                "message": self.message,
                "details": self.details,
            })),
        )
            .into_response();
        response.extensions_mut().insert(ErrorEnvelope);
        response
    }
}

impl From<CoreError> for ApiError {
    fn from(error: CoreError) -> Self {
        match error {
            CoreError::InvalidRequest(message) => Self::invalid(message),
            CoreError::Texture(message) => Self::texture(message),
            CoreError::UnsupportedStyle(message) => {
                Self::new(StatusCode::BAD_REQUEST, "unknown_style", message)
            }
            CoreError::BackendUnavailable(message) => Self::unavailable(message),
            CoreError::Timeout => Self::timeout("render timed out"),
            other => Self::internal(other.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{Body, to_bytes};

    #[tokio::test]
    async fn normalization_preserves_business_error_details() {
        let response = ApiError::upstream("upstream is unavailable")
            .with_details(json!({"retryable": true}))
            .into_response();
        let response = normalize_http_error(response);
        assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
        let body = to_bytes(response.into_body(), 65_536).await.unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&body).unwrap(),
            json!({
                "code": "upstream",
                "message": "upstream is unavailable",
                "details": {"retryable": true},
            })
        );
    }

    #[tokio::test]
    async fn normalization_drops_old_body_metadata_but_keeps_protocol_headers() {
        let response = Response::builder()
            .status(StatusCode::METHOD_NOT_ALLOWED)
            .header(header::CONTENT_TYPE, "text/plain")
            .header(header::CONTENT_LENGTH, "3")
            .header(header::CONTENT_ENCODING, "gzip")
            .header(header::ETAG, "old")
            .header(header::ALLOW, "GET,HEAD")
            .header(header::CACHE_CONTROL, "no-store")
            .body(Body::from("old"))
            .unwrap();
        let response = normalize_http_error(response);
        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(response.headers()[header::CONTENT_TYPE], "application/json");
        assert_eq!(response.headers()[header::ALLOW], "GET,HEAD");
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        for header in [
            header::CONTENT_LENGTH,
            header::CONTENT_ENCODING,
            header::ETAG,
        ] {
            assert!(!response.headers().contains_key(header));
        }
        let body = to_bytes(response.into_body(), 65_536).await.unwrap();
        let value: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["code"], "method_not_allowed");
    }

    #[tokio::test]
    async fn normalization_leaves_successful_responses_untouched() {
        let response = Response::builder()
            .header(header::CONTENT_TYPE, "image/png")
            .header(header::CONTENT_LENGTH, "3")
            .body(Body::from("png"))
            .unwrap();
        let response = normalize_http_error(response);
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CONTENT_TYPE], "image/png");
        assert_eq!(response.headers()[header::CONTENT_LENGTH], "3");
        assert_eq!(to_bytes(response.into_body(), 65_536).await.unwrap(), "png");
    }
}
