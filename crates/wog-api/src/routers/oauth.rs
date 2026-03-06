use std::collections::HashMap;

use axum::{
    Json,
    extract::{Query, State},
    http::{HeaderValue, header},
    response::{IntoResponse, Redirect},
};
use serde_json::json;
use wog_middleware::{AppState, AuthClaims};

pub async fn google_login(State(state): State<AppState>) -> Result<Redirect, Redirect> {
    let error_redirect = format!(
        "{}/?error=oauth_failed",
        state.app_config.default_config.client_url
    );

    let auth_url = state.oauth_services.auth_url().await.map_err(|e| {
        tracing::error!("OAuth login error: {}", e);
        Redirect::to(&error_redirect)
    })?;

    Ok(Redirect::to(&auth_url))
}

pub async fn google_callback(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<impl IntoResponse, Redirect> {
    let client_url = state.app_config.default_config.client_url.clone();
    let error_redirect = format!("{}/?error=oauth_failed", client_url);

    let code = params.get("code").ok_or_else(|| {
        tracing::error!("Missing code parameter");
        Redirect::to(&error_redirect)
    })?;

    let csrf = params.get("state").ok_or_else(|| {
        tracing::error!("Missing state/CSRF parameter");
        Redirect::to(&error_redirect)
    })?;

    let user = state
        .oauth_services
        .callback(code.to_string(), csrf.to_string())
        .await
        .map_err(|e| {
            tracing::error!("OAuth callback error: {}", e);
            Redirect::to(&error_redirect)
        })?;
    let token = state.app_config.generate_token(user).map_err(|e| {
        tracing::error!("OAuth callback error: {}", e);
        Redirect::to(&error_redirect)
    })?;

    let cookie = format!(
        "token={}; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=86400",
        token
    );

    let mut response = Redirect::to(&client_url).into_response();
    response
        .headers_mut()
        .insert(header::SET_COOKIE, HeaderValue::from_str(&cookie).unwrap());

    Ok(response)
}

pub async fn auth_me(AuthClaims(claims): AuthClaims) -> Json<serde_json::Value> {
    Json(json!({ "id": claims.sub, "username": claims.username }))
}

pub async fn logout() -> impl IntoResponse {
    let cookie = "token=; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=0";
    let mut response = Json(json!({ "message": "Logged out" })).into_response();
    response
        .headers_mut()
        .insert(header::SET_COOKIE, HeaderValue::from_static(cookie));
    response
}
