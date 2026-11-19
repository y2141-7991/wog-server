use std::collections::HashMap;

use axum::{
    Json,
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Redirect, Response},
};
use serde_json::json;
use wog_config::user::dto::{
    ChangePasswordRequest, ExchangeCodeRequest, ForgotPasswordRequest, LoginRequest,
    RegisterRequest, ResetPasswordRequest, UserResponse, VerifyEmailRequest,
};
use wog_middleware::{AppState, AuthClaims};

use axum_extra::extract::CookieJar;
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};
use wog_infras::{
    Claims, DefaultConfig, TokenPair, TokenType,
    services::users::{
        CODE_KIND_OAUTH, OAUTH_CODE_TTL_SECONDS, RESET_TOKEN_TTL_MINUTES, Refreshed,
    },
};

use crate::{errors::RestApiError, routers::support::origin_allowed};

/// The refresh token's cookie. It is the only cookie this API sets: the access token
/// travels in `Authorization: Bearer`, held in the page's memory.
///
/// - `HttpOnly`: scripts cannot read it.
/// - `Path=/api/v1/auth`: the browser sends it only to the endpoints that use it
///   (refresh, logout), not with every request.
/// - `SameSite=Strict`: never sent on a request another site starts.
/// - `Secure`: https only (`COOKIE_SECURE=false` for plain-http local development).
fn refresh_cookie(value: &str, max_age_seconds: i64, cfg: &DefaultConfig) -> String {
    format!(
        "refresh_token={value}; HttpOnly;{} SameSite=Strict; Path=/api/v1/auth; Max-Age={max_age_seconds}",
        if cfg.cookie_secure { " Secure;" } else { "" },
    )
}

/// Put the refresh token on a response as its cookie, and retire the cookies older
/// versions of this API set (`token`, `lang`, `timestamp` and the site-wide
/// `refresh_token`): nothing reads them any more.
pub(crate) fn apply_auth_cookies(
    response: &mut Response,
    token_pair: &TokenPair,
    cfg: &DefaultConfig,
) {
    let cookie = refresh_cookie(
        &token_pair.refresh_token,
        cfg.refresh_token_expire_minutes * 60,
        cfg,
    );
    let headers = response.headers_mut();
    headers.append(header::SET_COOKIE, HeaderValue::from_str(&cookie).unwrap());
    expire_legacy_cookies(headers);
}

fn expire_legacy_cookies(headers: &mut HeaderMap) {
    for cookie in [
        "token=; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=0",
        "refresh_token=; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=0",
        "lang=; SameSite=Lax; Path=/; Max-Age=0",
        "timestamp=; SameSite=Lax; Path=/; Max-Age=0",
    ] {
        headers.append(header::SET_COOKIE, HeaderValue::from_static(cookie));
    }
}

fn clear_refresh_cookie(response: &mut Response, cfg: &DefaultConfig) {
    let cookie = refresh_cookie("", 0, cfg);
    let headers = response.headers_mut();
    headers.append(header::SET_COOKIE, HeaderValue::from_str(&cookie).unwrap());
    expire_legacy_cookies(headers);
}

/// Browsers send `Origin` on a cross-origin POST. The endpoints that act on the refresh
/// cookie refuse one from a site that is not ours; a request without one (curl, tests)
/// is not a browser attack.
fn check_origin(headers: &HeaderMap, cfg: &DefaultConfig) -> Result<(), RestApiError> {
    let origin = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok());
    if origin_allowed(cfg, origin) {
        Ok(())
    } else {
        tracing::warn!(origin = ?origin, "Cookie endpoint refused: origin not allowed");
        Err(RestApiError::Forbidden("Origin not allowed".into()))
    }
}

/// Start a session for `user` and build the standard sign-in response: the user and
/// access token in the body, the refresh token as a cookie.
async fn sign_in_response(
    state: &AppState,
    user: wog_infras::models::User,
    status: StatusCode,
) -> Result<Response, RestApiError> {
    let token_pair = state
        .user_services
        .start_session(&state.app_config, &user)
        .await
        .map_err(|e| RestApiError::Internal(e.to_string()))?;
    let mut response = (
        status,
        Json(json!({
            "user": UserResponse::from(user),
            "access_token": token_pair.access_token,
            "refresh_token": token_pair.refresh_token,
        })),
    )
        .into_response();
    apply_auth_cookies(&mut response, &token_pair, &state.app_config.default_config);
    Ok(response)
}

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
    // No token in the URL: a one-time code, which the page swaps for tokens at once.
    let code = state
        .user_services
        .issue_code(CODE_KIND_OAUTH, user.id, None, OAUTH_CODE_TTL_SECONDS)
        .await
        .map_err(|e| {
            tracing::error!("OAuth callback error: {}", e);
            Redirect::to(&error_redirect)
        })?;

    Ok(Redirect::to(&format!("{client_url}/?code={code}")).into_response())
}

/// Swap the one-time code from the Google sign-in redirect for a session.
pub async fn exchange_code(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<ExchangeCodeRequest>,
) -> Result<Response, RestApiError> {
    check_origin(&headers, &state.app_config.default_config)?;
    let code = payload.code.trim();
    if code.is_empty() || code.len() > 128 {
        return Err(RestApiError::BadRequest("Code is required".into()));
    }
    let (user_id, _) = state
        .user_services
        .redeem_code(CODE_KIND_OAUTH, code)
        .await?
        .ok_or_else(|| RestApiError::Unauthorized("This sign-in link has expired".into()))?;
    let user = state.user_services.get_user(user_id).await?;
    sign_in_response(&state, user, StatusCode::OK).await
}

/// Register a new account with an email and password, then log it in
/// immediately (same cookie/token shape as every other login path).
pub async fn register(
    State(state): State<AppState>,
    Json(payload): Json<RegisterRequest>,
) -> Result<impl IntoResponse, RestApiError> {
    let username = payload.username.trim();
    let email = payload.email.trim();

    if username.is_empty() || email.is_empty() {
        return Err(RestApiError::BadRequest(
            "Username and email are required".into(),
        ));
    }
    if payload.password.len() < 8 {
        return Err(RestApiError::BadRequest(
            "Password must be at least 8 characters".into(),
        ));
    }

    let (user, verification_token) = state
        .user_services
        .register(username, email, &payload.password)
        .await?;

    let verify_url = format!(
        "{}/verify-email?token={}",
        state.app_config.default_config.client_url, verification_token
    );
    if let Err(e) = state
        .email_service
        .send_verification_email(&user.email, &user.username, &verify_url)
        .await
    {
        tracing::warn!(user_id = %user.id, error = %e, "Failed to send verification email");
    }

    sign_in_response(&state, user, StatusCode::CREATED).await
}

/// Verify an email address using the token from the verification link.
pub async fn verify_email(
    State(state): State<AppState>,
    Json(payload): Json<VerifyEmailRequest>,
) -> Result<impl IntoResponse, RestApiError> {
    if payload.token.trim().is_empty() {
        return Err(RestApiError::BadRequest("Token is required".into()));
    }

    let user = state.user_services.verify_email(&payload.token).await?;

    Ok(Json(json!({ "user": UserResponse::from(user) })))
}

/// Send a fresh verification email to the signed-in (not-yet-verified) user.
pub async fn resend_verification(
    State(state): State<AppState>,
    AuthClaims(claims): AuthClaims,
) -> Result<impl IntoResponse, RestApiError> {
    let (user, verification_token) = state.user_services.resend_verification(claims.sub).await?;

    let verify_url = format!(
        "{}/verify-email?token={}",
        state.app_config.default_config.client_url, verification_token
    );
    state
        .email_service
        .send_verification_email(&user.email, &user.username, &verify_url)
        .await
        .map_err(|e| {
            tracing::error!(user_id = %user.id, error = %e, "Failed to resend verification email");
            RestApiError::Internal("Could not send verification email".into())
        })?;

    Ok(Json(json!({ "message": "Verification email sent" })))
}

/// Log in with an email and password.
pub async fn login(
    State(state): State<AppState>,
    Json(payload): Json<LoginRequest>,
) -> Result<impl IntoResponse, RestApiError> {
    let email = payload.email.trim();

    if email.is_empty() || payload.password.is_empty() {
        return Err(RestApiError::BadRequest(
            "Email and password are required".into(),
        ));
    }

    let user = state.user_services.login(email, &payload.password).await?;

    sign_in_response(&state, user, StatusCode::OK).await
}

/// Change the signed-in user's password. This is also how a bootstrapped
/// admin clears `must_change_password`.
pub async fn change_password(
    State(state): State<AppState>,
    AuthClaims(claims): AuthClaims,
    Json(payload): Json<ChangePasswordRequest>,
) -> Result<Json<UserResponse>, RestApiError> {
    let user = state
        .user_services
        .change_password(claims.sub, &payload.current_password, &payload.new_password)
        .await?;
    Ok(Json(user.into()))
}

pub async fn auth_me(
    State(state): State<AppState>,
    AuthClaims(claims): AuthClaims,
) -> Result<Json<UserResponse>, RestApiError> {
    let user = state.user_services.get_user(claims.sub).await?;
    Ok(Json(user.into()))
}

/// Check a refresh token's signature (and, unless told otherwise, its expiry) and
/// return its claims.
fn decode_refresh_token(
    token: &str,
    cfg: &DefaultConfig,
    check_expiry: bool,
) -> Result<Claims, RestApiError> {
    let mut validation = Validation::new(Algorithm::HS512);
    validation.validate_exp = check_expiry;
    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(cfg.refresh_token_secret.as_bytes()),
        &validation,
    )
    .map_err(|e| {
        tracing::warn!("Refresh token validation failed: {}", e);
        RestApiError::Unauthorized("Invalid or expired refresh token".into())
    })?;
    if data.claims.token_type != TokenType::Refresh {
        return Err(RestApiError::Unauthorized("Invalid token type".into()));
    }
    Ok(data.claims)
}

/// Trade the refresh cookie for a new access token. The refresh token is single-use:
/// the response carries its replacement. A token that was already spent, outside the
/// short window in which two tabs can race, ends the whole session.
pub async fn refresh_token(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
) -> Result<Response, Response> {
    let cfg = &state.app_config.default_config;
    // Any failure also clears the cookie, so the browser stops sending a dead one.
    let fail = |e: RestApiError| {
        let mut response = e.into_response();
        clear_refresh_cookie(&mut response, cfg);
        response
    };

    check_origin(&headers, cfg).map_err(fail)?;
    let token = jar
        .get("refresh_token")
        .map(|c| c.value().to_string())
        .ok_or_else(|| fail(RestApiError::Unauthorized("Missing refresh token".into())))?;
    let claims = decode_refresh_token(&token, cfg, true).map_err(fail)?;

    let refreshed = state
        .user_services
        .refresh_session(&state.app_config, &claims)
        .await
        .map_err(|e| fail(e.into()))?;

    match refreshed {
        Refreshed::Rotated(pair) => {
            let mut response = Json(json!({
                "access_token": pair.access_token,
                "refresh_token": pair.refresh_token,
                "lang": pair.lang,
                "timestamp": pair.timestamp,
            }))
            .into_response();
            apply_auth_cookies(&mut response, &pair, cfg);
            Ok(response)
        }
        // A racing tab: the cookie it needs was already set by the tab that won.
        Refreshed::Replayed { access_token } => {
            Ok(Json(json!({ "access_token": access_token })).into_response())
        }
    }
}

/// Same answer whether or not the email is registered, so the form cannot be
/// used to find out who has an account.
const FORGOT_PASSWORD_MESSAGE: &str =
    "If an account with that email exists, we have sent a link to reset its password.";

/// Email a password-reset link. Always answers 200 with the same message; the
/// email is sent in the background so the response time does not give the
/// answer away either.
pub async fn forgot_password(
    State(state): State<AppState>,
    Json(payload): Json<ForgotPasswordRequest>,
) -> Result<impl IntoResponse, RestApiError> {
    let email = payload.email.trim();
    if email.is_empty() || email.len() > 254 {
        return Err(RestApiError::BadRequest("Email is required".into()));
    }

    match state.user_services.request_password_reset(email).await {
        Ok(Some((user, token))) => {
            let reset_url = format!(
                "{}/reset-password?token={}",
                state.app_config.default_config.client_url, token
            );
            let email_service = state.email_service.clone();
            tokio::spawn(async move {
                if let Err(e) = email_service
                    .send_password_reset_email(
                        &user.email,
                        &user.username,
                        &reset_url,
                        RESET_TOKEN_TTL_MINUTES,
                    )
                    .await
                {
                    tracing::warn!(user_id = %user.id, error = %e, "Failed to send password reset email");
                }
            });
        }
        Ok(None) => {}
        // Not the caller's business: answering differently would leak.
        Err(e) => tracing::error!(error = %e, "Password reset request failed"),
    }

    Ok(Json(json!({ "message": FORGOT_PASSWORD_MESSAGE })))
}

/// Set a new password with the token from the emailed link. The link works
/// once; every session issued before this moment stops working.
pub async fn reset_password(
    State(state): State<AppState>,
    Json(payload): Json<ResetPasswordRequest>,
) -> Result<impl IntoResponse, RestApiError> {
    let user = state
        .user_services
        .reset_password(&payload.token, &payload.new_password)
        .await?;

    let email_service = state.email_service.clone();
    tokio::spawn(async move {
        if let Err(e) = email_service
            .send_password_changed_email(&user.email, &user.username)
            .await
        {
            tracing::warn!(user_id = %user.id, error = %e, "Failed to send password changed email");
        }
    });

    Ok(Json(
        json!({ "message": "Password updated. Sign in with your new password." }),
    ))
}

/// End this sign-in: the session behind the refresh cookie is revoked, so that token
/// cannot be used again. Always succeeds and always clears the cookie.
pub async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
) -> Result<Response, RestApiError> {
    let cfg = &state.app_config.default_config;
    check_origin(&headers, cfg)?;

    // An expired token still names its session, so expiry is not checked here.
    let sid = jar
        .get("refresh_token")
        .and_then(|c| decode_refresh_token(c.value(), cfg, false).ok())
        .and_then(|claims| claims.sid);
    if let Some(sid) = sid
        && let Err(e) = state.user_services.end_session(sid).await
    {
        tracing::warn!(session_id = %sid, error = %e, "Could not revoke the session on logout");
    }

    let mut response = Json(json!({ "message": "Logged out" })).into_response();
    clear_refresh_cookie(&mut response, cfg);
    Ok(response)
}
