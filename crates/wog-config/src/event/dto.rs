use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;
use wog_infras::models::{Event, EventNew, EventUpdate, PaginationRequest};

use crate::model::PaginateResponse;

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

#[derive(Deserialize, Serialize, ToSchema)]
pub struct EventListRequest {
    pub page: Option<i64>,
    pub per_page: Option<i64>,
    pub status: Option<String>,
    pub search: Option<String>,
}

impl EventListRequest {
    pub fn pagination(&self) -> PaginationRequest {
        PaginationRequest {
            page: self.page.unwrap_or(0),
            per_page: self.per_page,
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
