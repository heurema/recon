use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("configuration error: {0}")]
    ConfigError(String),

    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("parse error: {0}")]
    ParseError(String),

    #[error("validation error: {0}")]
    ValidationError(String),
}
