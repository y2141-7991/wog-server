use axum::{
    Json,
    extract::{Path, State},
};
use uuid::Uuid;
use wog_config::user::dto::UserResponse;
use wog_middleware::{AppState, AuthClaims};

use crate::errors::{RestApiError, RestApiResponseError};

#[utoipa::path(
    get,
    path = "/api/v1/user/{id}",
    tag = "User",
    responses(
        (status = 200, description = "Get single user", body = UserResponse),
        (status = 401, description = "Unauthorized", body = RestApiResponseError),
        (status = 500, description = "Internal error", body = RestApiResponseError),
    ),
    security(("bearer_auth" = []))
)]
#[axum::debug_handler]
pub async fn get_profile(
    State(state): State<AppState>,
    AuthClaims(_claims): AuthClaims,
    Path(user_id): Path<Uuid>,
) -> Result<Json<UserResponse>, RestApiError> {
    let user = state.user_services.get_user(user_id).await?;
    Ok(Json(user.into()))
}
