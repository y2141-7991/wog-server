use axum::{
    Router,
    routing::{get, post},
};
use utoipa_axum::{router::OpenApiRouter, routes};
use wog_middleware::AppState;

use crate::routers::oauth::{auth_me, google_callback, google_login, logout};

pub mod oauth;
pub mod user;

pub fn user_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(user::get_profile))
}

pub fn oauth_routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/auth/oauth/google", get(google_login))
        .route("/api/v1/auth/oauth/google/callback", get(google_callback))
        .route("/api/v1/auth/me", get(auth_me))
        .route("/api/v1/auth/logout", post(logout))
}
