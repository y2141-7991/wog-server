use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use serde_json::json;
use utoipa::ToSchema;
use wog_infras::errors::DatabaseError;

#[derive(ToSchema, Serialize)]
pub struct RestApiResponseError {
    pub code: ErrorCode,
    pub message: String,
}

#[derive(ToSchema, Serialize)]
pub enum ErrorCode {
    #[serde(rename = "BAD_REQUEST")]
    BadRequest,
    #[serde(rename = "NOT_FOUND")]
    NotFound,
    #[serde(rename = "CONFLICT")]
    Conflict,
    #[serde(rename = "FORBIDDEN")]
    Forbidden,
    #[serde(rename = "UNAUTHORIZED")]
    Unauthorized,
    #[serde(rename = "INTERNAL_SERVER_ERROR")]
    InternalServerError,
}

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
            DatabaseError::ExistedDataError(msg) => RestApiError::Conflict(msg),
            DatabaseError::InvalidCredentials => {
                RestApiError::Unauthorized("Invalid email or password".into())
            }
            DatabaseError::InvalidVerificationToken => {
                RestApiError::BadRequest("Invalid or expired verification link".into())
            }
            DatabaseError::ValidationError(msg) => RestApiError::BadRequest(msg),
            DatabaseError::EventNotOpen => {
                RestApiError::BadRequest("This event is not open for registration".into())
            }
            DatabaseError::EventFull => {
                RestApiError::Conflict("This event has reached full capacity".into())
            }
            DatabaseError::InsufficientBalance => {
                RestApiError::BadRequest("Insufficient wallet balance".into())
            }
            DatabaseError::AlreadyRegistered => {
                RestApiError::Conflict("You are already registered for this event".into())
            }
            DatabaseError::RegistrationNotFound => {
                RestApiError::NotFound("Registration not found".into())
            }
            DatabaseError::CannotJoinOwnEvent => {
                RestApiError::BadRequest("You cannot join your own event".into())
            }
            _ => RestApiError::Database(value.to_string()),
        }
    }
}
