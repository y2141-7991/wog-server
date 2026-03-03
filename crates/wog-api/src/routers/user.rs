use axum::{
    Json,
    extract::{Path, State},
};
use uuid::Uuid;
use wog_config::user::dto::UserResponse;
use wog_middleware::AppState;

use crate::errors::RestApiError;

pub async fn get_profile(
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
) -> Result<Json<UserResponse>, RestApiError> {
    let user = state.user_services.get_user(user_id).await?;
    Ok(Json(user.into()))
}
