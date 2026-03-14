use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error(transparent)]
    Storage(#[from] opendal::Error),

    #[error(transparent)]
    Database(#[from] sqlx::Error),

    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match &self {
            AppError::BadRequest(msg) => tracing::warn!("Bad request: {}", msg),
            AppError::NotFound(msg) => tracing::debug!("Resource not found: {}", msg),
            AppError::Forbidden(msg) => tracing::warn!("Access forbidden: {}", msg),
            AppError::Storage(err) => tracing::error!("Storage error: {:?}", err),
            AppError::Database(err) => tracing::error!("Database error: {:?}", err),
            AppError::Internal(err) => tracing::error!("Internal server error: {:?}", err),
        }

        let (status, error_message) = match self {
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            AppError::Forbidden(msg) => (StatusCode::FORBIDDEN, msg),
            AppError::Storage(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Storage operation failed".to_string(),
            ),
            AppError::Database(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Database operation failed".to_string(),
            ),
            AppError::Internal(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal server error".to_string(),
            ),
        };

        let body = Json(json!({
            "error": error_message
        }));

        (status, body).into_response()
    }
}
