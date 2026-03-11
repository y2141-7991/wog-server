use axum::{
    Json,
    extract::{Path, State},
};
use uuid::Uuid;
use wog_config::event::dto::{
    EventCreateRequest, EventListResponse, EventResponse, create_from_req_to_domain,
    update_from_req_to_domain,
};
use wog_middleware::{AppState, AuthClaims};

use crate::errors::{RestApiError, RestApiResponseError};

#[utoipa::path(
    post,
    path = "/api/v1/events",
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
        .create_new_event(create_from_req_to_domain(_claims.sub, payload))
        .await?;
    Ok(Json(event.into()))
}

#[utoipa::path(
    get,
    path = "/api/v1/events",
    tag = "Event",
    responses(
        (status = 200, description = "Get events", body = EventResponse),
        (status = 401, description = "Unauthorized", body = RestApiResponseError),
        (status = 500, description = "Internal error", body = RestApiResponseError),
    ),
    security(("bearer_auth" = []))
)]
#[axum::debug_handler]
pub async fn get_events_current_id(
    State(state): State<AppState>,
    AuthClaims(_claims): AuthClaims,
) -> Result<Json<EventListResponse>, RestApiError> {
    let events = state
        .event_services
        .find_events_by_current_id(_claims.sub)
        .await?;
    let items = events
        .into_iter()
        .map(|e| e.into())
        .collect::<Vec<EventResponse>>();
    Ok(Json(EventListResponse { data: items }))
}

#[utoipa::path(
    patch,
    path = "/api/v1/events/{id}",
    tag = "Event",
    responses(
        (status = 200, description = "Update events", body = EventResponse), 
        (status = 401, description = "Unauthorized", body = RestApiResponseError),
        (status = 500, description = "Internal error", body = RestApiResponseError),
    ),
    security(("bearer_auth" = []))
)]
#[axum::debug_handler]
pub async fn update_event(
    State(state): State<AppState>,
    AuthClaims(_claims): AuthClaims,
    Path(event_id): Path<Uuid>,
    Json(payload): Json<EventCreateRequest>,
) -> Result<Json<EventResponse>, RestApiError> {
    let event = state
        .event_services
        .update_event(update_from_req_to_domain(payload), event_id)
        .await?;
    Ok(Json(event.into()))
}
