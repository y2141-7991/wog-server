use axum::{Json, extract::State};
use wog_config::event::dto::{EventCreateRequest, EventResponse, mapping_from_req_to_domain};
use wog_middleware::{AppState, AuthClaims};

use crate::errors::{RestApiError, RestApiResponseError};

#[utoipa::path(
    post,
    path = "/api/v1/event",
    tag = "Event",
    request_body(content = EventCreateRequest, content_type = "application/json"),
    responses(
        (status = 201, description = "Create event", body = EventResponse),
        (status = 401, description = "Unauthorized", body = RestApiResponseError),
        (status = 500, description = "Internal error", body = RestApiResponseError),
    ),
    security(("bearer_auth" = []))
)]
#[axum::debug_handler]
pub async fn create_event(
    State(state): State<AppState>,
    AuthClaims(_claims): AuthClaims,
    Json(payload): Json<EventCreateRequest>,
) -> Result<Json<EventResponse>, RestApiError> {
    let event = state
        .event_services
        .create_new_event(mapping_from_req_to_domain(_claims.sub, payload))
        .await?;
    Ok(Json(event.into()))
}
