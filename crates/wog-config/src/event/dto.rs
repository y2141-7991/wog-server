use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;
use wog_infras::models::{Event, EventFilter, EventNew, EventRegistration, EventUpdate, Wallet};

use crate::{model::{PaginateRequest, PaginateResponse}, wallet::dto::WalletResponse};

#[derive(Deserialize, Serialize, ToSchema)]
pub struct EventCreateRequest {
    pub title: String,
    pub description: Option<String>,
    pub price: Decimal,
    pub capacity: i32,
    pub status: String,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub location: Option<String>,
}

#[derive(Deserialize, Serialize, ToSchema)]
pub struct EventResponse {
    pub id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub organizer_id: Uuid,
    pub price: Decimal,
    pub capacity: i32,
    pub registered_count: i32,
    pub status: String,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub location: Option<String>,
}
#[derive(Deserialize, Serialize, ToSchema)]
pub struct EventListResponse {
    pub data: Vec<EventResponse>,
    pub pagination_meta: PaginateResponse,
}

#[derive(Deserialize, Serialize, ToSchema, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct EventListRequest {
    #[serde(flatten)]
    pub pagination: PaginateRequest,
    #[serde(flatten)]
    pub event_filter: EventFilterRequest,
}

#[serde_as]
#[derive(ToSchema, IntoParams, serde::Serialize, serde::Deserialize)]
#[into_params(parameter_in = Query)]
pub struct EventFilterRequest {
    pub status: Option<String>,
    pub location: Option<String>,
}

impl From<EventFilterRequest> for EventFilter {
    fn from(value: EventFilterRequest) -> Self {
        EventFilter {
            status: value.status,
            location: value.location,
        }
    }
}

impl From<Event> for EventResponse {
    fn from(value: Event) -> Self {
        EventResponse {
            id: value.id,
            title: value.title,
            description: value.description,
            organizer_id: value.organizer_id,
            price: value.price,
            capacity: value.capacity,
            registered_count: value.registered_count,
            status: value.status,
            start_time: value.start_time,
            end_time: value.end_time,
            location: value.location,
        }
    }
}

pub fn create_from_req_to_domain(creator_id: Uuid, event: EventCreateRequest) -> EventNew {
    EventNew {
        title: event.title,
        description: event.description,
        organizer_id: creator_id,
        price: event.price,
        capacity: event.capacity,
        registered_count: 0,
        status: event.status,
        start_time: event.start_time,
        end_time: event.end_time,
        location: event.location,
    }
}

pub fn update_from_req_to_domain(event: EventCreateRequest) -> EventUpdate {
    EventUpdate {
        title: event.title,
        description: event.description,
        price: event.price,
        capacity: event.capacity,
        status: event.status,
        start_time: event.start_time,
        end_time: event.end_time,
        location: event.location,
    }
}

#[derive(Deserialize, Serialize, ToSchema)]
pub struct EventRegistrationResponse {
    pub id: Uuid,
    pub event_id: Uuid,
    pub user_id: Uuid,
    pub amount_paid: Decimal,
    pub status: String,
    pub registered_at: DateTime<Utc>,
    pub cancelled_at: Option<DateTime<Utc>>,
}

impl From<EventRegistration> for EventRegistrationResponse {
    fn from(value: EventRegistration) -> Self {
        Self {
            id: value.id,
            event_id: value.event_id,
            user_id: value.user_id,
            amount_paid: value.amount_paid,
            status: value.status,
            registered_at: value.registered_at,
            cancelled_at: value.cancelled_at,
        }
    }
}

#[derive(Deserialize, Serialize, ToSchema)]
pub struct EventRegistrationActionResponse {
    pub registration: EventRegistrationResponse,
    pub event: EventResponse,
    pub wallet: Option<WalletResponse>,
}

impl EventRegistrationActionResponse {
    pub fn new(registration: EventRegistration, event: Event, wallet: Option<Wallet>) -> Self {
        Self {
            registration: registration.into(),
            event: event.into(),
            wallet: wallet.map(Into::into),
        }
    }
}
