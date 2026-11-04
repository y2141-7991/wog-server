pub mod rate_limit;

use axum::{
    extract::FromRequestParts,
    http::{StatusCode, request::Parts},
    response::{IntoResponse, Response},
};
use axum_extra::extract::CookieJar;
use jsonwebtoken::{DecodingKey, Validation, decode};
use serde_json::json;
use wog_infras::{
    AppConfig, Claims, TokenType,
    email::EmailService,
    models::User,
    services::{
        admin::AdminServices, billing::BillingServices, marketplace::MarketplaceServices,
        oauth::OAuthServices, support::SupportServices, users::UserServices,
    },
};

#[derive(Clone)]
pub struct AppState {
    pub user_services: UserServices,
    pub app_config: AppConfig,
    pub oauth_services: OAuthServices,
    pub email_service: EmailService,
    pub marketplace_services: MarketplaceServices,
    pub admin_services: AdminServices,
    pub billing_services: BillingServices,
    pub support_services: SupportServices,
}

pub struct AuthClaims(pub Claims);

/// A signed-in admin. Unlike `AuthClaims` it re-reads the role from the
/// database on every request, so a demotion takes effect immediately, and it
/// refuses an admin who still has to change the bootstrap password.
pub struct AdminClaims(pub Claims);

/// A signed-in user who may use the app: a valid access token, an account that
/// still exists, a verified email, and no pending forced password change.
/// The user is read from the database, so `user.role` is never taken from the
/// token. Also carries the token's expiry so long-lived connections can end
/// with the session.
pub struct VerifiedUser {
    pub user: User,
    pub token_exp: usize,
    /// When the token was issued (seconds), for `session_is_current`.
    pub token_iat: usize,
}

#[derive(Debug)]
pub enum AdminError {
    Auth(AuthError),
    Forbidden(&'static str),
}

impl IntoResponse for AdminError {
    fn into_response(self) -> Response {
        match self {
            AdminError::Auth(e) => e.into_response(),
            AdminError::Forbidden(message) => (
                StatusCode::FORBIDDEN,
                axum::Json(json!({ "error": true, "message": message })),
            )
                .into_response(),
        }
    }
}

impl FromRequestParts<AppState> for AdminClaims {
    type Rejection = AdminError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let (claims, user) = authenticate(parts, state).await.map_err(AdminError::Auth)?;
        if !user.is_admin() {
            return Err(AdminError::Forbidden("Admin access required"));
        }
        if user.must_change_password {
            return Err(AdminError::Forbidden("Change your password first"));
        }
        Ok(AdminClaims(claims))
    }
}

impl FromRequestParts<AppState> for VerifiedUser {
    type Rejection = AdminError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let (claims, user) = authenticate(parts, state).await.map_err(AdminError::Auth)?;
        if !user.email_verified {
            return Err(AdminError::Forbidden("Verify your email first"));
        }
        if user.must_change_password {
            return Err(AdminError::Forbidden("Change your password first"));
        }
        Ok(VerifiedUser {
            user,
            token_exp: claims.exp,
            token_iat: claims.iat,
        })
    }
}

#[derive(Debug)]
pub struct AuthError(String);

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let body = json!({
            "error": true,
            "message": self.0
        });
        (StatusCode::UNAUTHORIZED, axum::Json(body)).into_response()
    }
}

impl FromRequestParts<AppState> for AuthClaims {
    type Rejection = AuthError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        authenticate(parts, state)
            .await
            .map(|(claims, _user)| AuthClaims(claims))
    }
}

/// Check the token, then load the account and make sure the session still
/// counts: the account exists and the token was not issued before a password
/// reset. One database read, shared by every extractor.
async fn authenticate(parts: &mut Parts, state: &AppState) -> Result<(Claims, User), AuthError> {
    let claims = decode_access_token(parts, state)?;
    let user = state
        .user_services
        .get_user(claims.sub)
        .await
        .map_err(|_| AuthError("Account not found".into()))?;
    if !session_is_current(&user, claims.iat) {
        return Err(AuthError("Session expired, please sign in again".into()));
    }
    Ok((claims, user))
}

/// A token is void when the password was reset after it was issued ("sign out
/// everywhere"). Compared in whole seconds, the resolution of `iat`; an account
/// that never reset its password has no cutoff.
pub fn session_is_current(user: &User, token_iat: usize) -> bool {
    match user.password_changed_at() {
        Some(changed) => (token_iat as i64) >= changed.timestamp(),
        None => true,
    }
}

fn decode_access_token(parts: &Parts, state: &AppState) -> Result<Claims, AuthError> {
    let token = parts
        .headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|t| t.to_string());

    let token = match token {
        Some(t) => t,
        None => {
            let jar = CookieJar::from_headers(&parts.headers);
            jar.get("token")
                .map(|c| c.value().to_string())
                .ok_or_else(|| AuthError("Missing authentication token".into()))?
        }
    };

    let secret = &state.app_config.default_config.jwt_secret;
    let token_data = decode::<Claims>(
        &token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )
    .map_err(|e| {
        tracing::warn!(
            method = %parts.method,
            uri = %parts.uri,
            "JWT validation failed: {}", e);
        AuthError("Invalid or expired token".into())
    })?;

    if token_data.claims.token_type != TokenType::Access {
        return Err(AuthError(
            "Refresh token cannot be used for authentication".into(),
        ));
    }

    Ok(token_data.claims)
}
