use thiserror::Error;

/// Errors produced by the rendering core.
#[derive(Debug, Error)]
pub enum CoreError {
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error("invalid texture: {0}")]
    Texture(String),
    #[error("render failed: {0}")]
    Render(String),
    #[error("unsupported style: {0}")]
    UnsupportedStyle(String),
    #[error("render backend unavailable: {0}")]
    BackendUnavailable(String),
    #[error("render timed out")]
    Timeout,
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("image error: {0}")]
    Image(#[from] image::ImageError),
}

pub type Result<T> = std::result::Result<T, CoreError>;
