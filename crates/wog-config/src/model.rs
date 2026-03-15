use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use wog_infras::models::PaginationRequest;

#[derive(Deserialize, Serialize, ToSchema)]
pub struct PaginateRequest {
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

impl From<PaginateRequest> for PaginationRequest {
    fn from(value: PaginateRequest) -> Self {
        PaginationRequest {
            per_page: value.per_page,
            page: value.page.unwrap_or(0),
        }
    }
}

#[derive(Deserialize, Serialize, ToSchema)]
pub struct PaginateResponse {
    pub next_page: i64,
    pub has_more: bool,
}
