use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;
use wog_infras::errors::DatabaseError;

#[derive(Debug, thiserror::Error)]
pub enum RestApiError {
    #[error("{0}")]
    BadRequest(String),

    #[error("{0}")]
    Unauthorized(String),

    #[error("{0}")]
    Forbidden(String),

    #[error("{0}")]
    NotFound(String),

    #[error("{0}")]
    Conflict(String),

    #[error("Database error: {0}")]
    Database(String),

    #[error("{0}")]
    Internal(String),
}

impl IntoResponse for RestApiError {
    fn into_response(self) -> Response {
        let (status_code, message) = match &self {
            RestApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            RestApiError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg.clone()),
            RestApiError::Forbidden(msg) => (StatusCode::FORBIDDEN, msg.clone()),
            RestApiError::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            RestApiError::Conflict(msg) => (StatusCode::CONFLICT, msg.clone()),
            RestApiError::Database(e) => {
                tracing::error!("Database error: {:?}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".into(),
                )
            }
            RestApiError::Internal(msg) => {
                tracing::error!("Internal error: {}", msg);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".into(),
                )
            }
        };

        let body = json!({
            "error": true,
            "message": message
        });
        (status_code, axum::Json(body)).into_response()
    }
}

impl From<DatabaseError> for RestApiError {
    fn from(value: DatabaseError) -> Self {
        match value {
            DatabaseError::ValueNotFound(value) => RestApiError::NotFound(value),
            DatabaseError::UniqueViolation => {
                RestApiError::Conflict("Resource already exists".into())
            }
            _ => RestApiError::Database(value.to_string()),
        }
    }
}
