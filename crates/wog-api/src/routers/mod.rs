use utoipa_axum::{router::OpenApiRouter, routes};
use wog_middleware::AppState;

pub mod user;

pub fn user_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(user::get_profile))
}
