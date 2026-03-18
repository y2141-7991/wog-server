use axum::{
    Json,
    extract::{Path, Query, State},
    response::IntoResponse,
};
use tracing::instrument;
use uuid::Uuid;
use wog_config::{
    event::dto::{
        EventCreateRequest, EventListRequest, EventListResponse, EventResponse,
        create_from_req_to_domain, update_from_req_to_domain,
    },
    model::PaginateResponse,
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
#[instrument(skip(state, _claims, payload))]
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
#[instrument(skip(state, _claims, request))]
pub async fn get_events(
    AuthClaims(_claims): AuthClaims,
    Query(request): Query<EventListRequest>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, RestApiError> {
    let events = state
        .event_services
        .find_events(request.pagination.into(), request.event_filter.into())
        .await?;
    let items = events
        .items
        .into_iter()
        .map(|e| e.into())
        .collect::<Vec<EventResponse>>();
    Ok(Json(EventListResponse {
        data: items,
        pagination_meta: PaginateResponse {
            next_page: events.next_page,
            has_more: events.has_more,
        },
    }))
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
#[instrument(skip(state, _claims, payload), fields(event_id = %event_id))]
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
