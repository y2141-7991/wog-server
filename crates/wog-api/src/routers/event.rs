use axum::{
    Json,
    extract::{Path, Query, State},
    response::IntoResponse,
};
use tracing::instrument;
use uuid::Uuid;
use wog_config::{
    event::dto::{
        EventCreateRequest, EventListRequest, EventListResponse, EventRegistrationActionResponse,
        EventRegistrationResponse, EventResponse, create_from_req_to_domain,
        update_from_req_to_domain,
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

#[utoipa::path(
    post,
    path = "/api/v1/events/{id}/join",
    tag = "Event",
    responses(
        (status = 200, description = "Join event", body = EventRegistrationActionResponse),
        (status = 400, description = "Event not open or insufficient balance", body = RestApiResponseError),
        (status = 401, description = "Unauthorized", body = RestApiResponseError),
        (status = 409, description = "Event full or already registered", body = RestApiResponseError),
        (status = 500, description = "Internal error", body = RestApiResponseError),
    ),
    security(("bearer_auth" = []))
)]
#[axum::debug_handler]
#[instrument(skip(state, claims), fields(event_id = %event_id))]
pub async fn join_event(
    State(state): State<AppState>,
    AuthClaims(claims): AuthClaims,
    Path(event_id): Path<Uuid>,
) -> Result<Json<EventRegistrationActionResponse>, RestApiError> {
    let (registration, event, wallet) = state
        .event_registration_services
        .join_event(event_id, claims.sub)
        .await?;
    Ok(Json(EventRegistrationActionResponse::new(
        registration,
        event,
        wallet,
    )))
}

#[utoipa::path(
    post,
    path = "/api/v1/events/{id}/leave",
    tag = "Event",
    responses(
        (status = 200, description = "Leave event", body = EventRegistrationActionResponse),
        (status = 401, description = "Unauthorized", body = RestApiResponseError),
        (status = 404, description = "Registration not found", body = RestApiResponseError),
        (status = 500, description = "Internal error", body = RestApiResponseError),
    ),
    security(("bearer_auth" = []))
)]
#[axum::debug_handler]
#[instrument(skip(state, claims), fields(event_id = %event_id))]
pub async fn leave_event(
    State(state): State<AppState>,
    AuthClaims(claims): AuthClaims,
    Path(event_id): Path<Uuid>,
) -> Result<Json<EventRegistrationActionResponse>, RestApiError> {
    let (registration, event, wallet) = state
        .event_registration_services
        .leave_event(event_id, claims.sub)
        .await?;
    Ok(Json(EventRegistrationActionResponse::new(
        registration,
        event,
        wallet,
    )))
}

#[utoipa::path(
    get,
    path = "/api/v1/events/{id}/registration",
    tag = "Event",
    responses(
        (status = 200, description = "Get my registration for this event", body = Option<EventRegistrationResponse>),
        (status = 401, description = "Unauthorized", body = RestApiResponseError),
        (status = 500, description = "Internal error", body = RestApiResponseError),
    ),
    security(("bearer_auth" = []))
)]
#[axum::debug_handler]
#[instrument(skip(state, claims), fields(event_id = %event_id))]
pub async fn get_my_registration(
    State(state): State<AppState>,
    AuthClaims(claims): AuthClaims,
    Path(event_id): Path<Uuid>,
) -> Result<Json<Option<EventRegistrationResponse>>, RestApiError> {
    let registration = state
        .event_registration_services
        .get_my_registration(event_id, claims.sub)
        .await?;
    Ok(Json(registration.map(Into::into)))
}
