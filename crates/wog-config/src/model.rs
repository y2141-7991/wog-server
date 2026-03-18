use serde::{Deserialize, Serialize};
use serde_with::{DisplayFromStr, serde_as};
use utoipa::{IntoParams, ToSchema};
use wog_infras::models::PaginationRequest;

#[serde_as]
#[derive(ToSchema, serde::Serialize, serde::Deserialize, Copy, Clone, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct PaginateRequest {
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub page: Option<i64>,
    #[serde_as(as = "Option<DisplayFromStr>")]
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
