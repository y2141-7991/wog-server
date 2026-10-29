use axum::{
    Router,
    routing::{get, post},
};
use utoipa_axum::{router::OpenApiRouter, routes};
use wog_middleware::AppState;

use crate::routers::oauth::{
    auth_me, change_password, google_callback, google_login, login, logout, refresh_token, register,
    resend_verification, verify_email,
};

pub mod admin;
pub mod marketplace;
pub mod oauth;
pub mod user;

pub fn user_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(user::get_profile))
}

pub fn oauth_routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/auth/oauth/google", get(google_login))
        .route("/api/v1/auth/oauth/google/callback", get(google_callback))
        .route("/api/v1/auth/register", post(register))
        .route("/api/v1/auth/login", post(login))
        .route("/api/v1/auth/verify-email", post(verify_email))
        .route("/api/v1/auth/resend-verification", post(resend_verification))
        .route("/api/v1/auth/me", get(auth_me))
        .route("/api/v1/auth/refresh", post(refresh_token))
        .route("/api/v1/auth/change-password", post(change_password))
        .route("/api/v1/auth/logout", post(logout))
}

pub fn marketplace_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(marketplace::list_apis))
        .routes(routes!(marketplace::get_api))
        .routes(routes!(marketplace::create_key, marketplace::list_keys))
        .routes(routes!(marketplace::revoke_key))
        .routes(routes!(marketplace::get_usage))
}

/// The metered gateway, authenticated by `X-API-Key` instead of a session.
pub fn gateway_routes() -> Router<AppState> {
    Router::new().route("/gateway/v1/{provider}/{endpoint}", get(marketplace::gateway))
}

pub fn admin_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(admin::get_overview))
        .routes(routes!(admin::list_users))
        .routes(routes!(admin::get_user_usage))
        .routes(routes!(admin::set_role))
        .routes(routes!(admin::get_audit_log))
}
