use axum::Json;
use axum::http::StatusCode;
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
        (
            self.status,
            Json(json!({
                "code": self.code,
                "message": self.message,
                "details": self.details,
            })),
        )
            .into_response()
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
