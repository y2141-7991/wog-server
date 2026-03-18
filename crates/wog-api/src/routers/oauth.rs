use std::collections::HashMap;

use axum::{
    Json,
    extract::{Query, State},
    http::{HeaderValue, header},
    response::{IntoResponse, Redirect},
};
use serde_json::json;
use wog_config::user::dto::UserResponse;
use wog_middleware::{AppState, AuthClaims};

use axum_extra::extract::CookieJar;
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};
use wog_infras::{Claims, TokenType};

use crate::errors::RestApiError;

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
    let token_pair = state.app_config.generate_token_pair(&user).map_err(|e| {
        tracing::error!("OAuth callback error: {}", e);
        Redirect::to(&error_redirect)
    })?;

    let access_cookie = format!(
        "token={}; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age={}",
        token_pair.access_token,
        state.app_config.default_config.access_token_expire_minutes * 60
    );
    let refresh_cookie = format!(
        "refresh_token={}; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age={}",
        token_pair.refresh_token,
        state.app_config.default_config.refresh_token_expire_minutes * 60
    );

    let lang_cookie = format!(
        "lang={}; SameSite=Lax; Path=/; Max-Age={}",
        token_pair.lang,
        state.app_config.default_config.refresh_token_expire_minutes * 60
    );
    let timestamp_cookie = format!(
        "timestamp={}; SameSite=Lax; Path=/; Max-Age={}",
        token_pair.timestamp,
        state.app_config.default_config.access_token_expire_minutes * 60
    );

    let mut response = Redirect::to(&client_url).into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&access_cookie).unwrap(),
    );
    headers.append(
        header::SET_COOKIE,
        HeaderValue::from_str(&refresh_cookie).unwrap(),
    );
    headers.append(
        header::SET_COOKIE,
        HeaderValue::from_str(&lang_cookie).unwrap(),
    );
    headers.append(
        header::SET_COOKIE,
        HeaderValue::from_str(&timestamp_cookie).unwrap(),
    );

    Ok(response)
}

pub async fn auth_me(
    State(state): State<AppState>,
    AuthClaims(claims): AuthClaims,
) -> Result<Json<UserResponse>, RestApiError> {
    let user = state.user_services.get_user(claims.sub).await?;
    Ok(Json(user.into()))
}

pub async fn refresh_token(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<impl IntoResponse, RestApiError> {
    let refresh_token = jar
        .get("refresh_token")
        .map(|c| c.value().to_string())
        .ok_or_else(|| RestApiError::Unauthorized("Missing refresh token".into()))?;

    let secret = &state.app_config.default_config.refresh_token_secret;
    let mut validation = Validation::new(Algorithm::HS512);
    validation.validate_exp = true;
    let token_data = decode::<Claims>(
        &refresh_token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .map_err(|e| {
        tracing::warn!("Refresh token validation failed: {}", e);
        RestApiError::Unauthorized("Invalid or expired refresh token".into())
    })?;

    if token_data.claims.token_type != TokenType::Refresh {
        return Err(RestApiError::Unauthorized("Invalid token type".into()));
    }

    let token_pair = state
        .app_config
        .refresh_access_token(&token_data.claims)
        .map_err(|e| RestApiError::Internal(e.to_string()))?;

    let access_cookie = format!(
        "token={}; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age={}",
        token_pair.access_token,
        state.app_config.default_config.access_token_expire_minutes * 60
    );
    let refresh_cookie = format!(
        "refresh_token={}; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age={}",
        token_pair.refresh_token,
        state.app_config.default_config.refresh_token_expire_minutes * 60
    );

    let lang_cookie = format!(
        "lang={}; SameSite=Lax; Path=/; Max-Age={}",
        token_pair.lang,
        state.app_config.default_config.refresh_token_expire_minutes * 60
    );
    let timestamp_cookie = format!(
        "timestamp={}; SameSite=Lax; Path=/; Max-Age={}",
        token_pair.timestamp,
        state.app_config.default_config.access_token_expire_minutes * 60
    );

    let mut response = Json(json!({
        "access_token": token_pair.access_token,
        "refresh_token": token_pair.refresh_token,
        "lang": token_pair.lang,
        "timestamp": token_pair.timestamp,
    }))
    .into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&access_cookie).unwrap(),
    );
    headers.append(
        header::SET_COOKIE,
        HeaderValue::from_str(&refresh_cookie).unwrap(),
    );
    headers.append(
        header::SET_COOKIE,
        HeaderValue::from_str(&lang_cookie).unwrap(),
    );
    headers.append(
        header::SET_COOKIE,
        HeaderValue::from_str(&timestamp_cookie).unwrap(),
    );

    Ok(response)
}

pub async fn logout() -> impl IntoResponse {
    let mut response = Json(json!({ "message": "Logged out" })).into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::SET_COOKIE,
        HeaderValue::from_static("token=; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=0"),
    );
    headers.append(
        header::SET_COOKIE,
        HeaderValue::from_static(
            "refresh_token=; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=0",
        ),
    );
    headers.append(
        header::SET_COOKIE,
        HeaderValue::from_static("lang=; SameSite=Lax; Path=/; Max-Age=0"),
    );
    headers.append(
        header::SET_COOKIE,
        HeaderValue::from_static("timestamp=; SameSite=Lax; Path=/; Max-Age=0"),
    );
    response
}
