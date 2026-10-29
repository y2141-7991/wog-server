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
    services::{
        admin::AdminServices, marketplace::MarketplaceServices, oauth::OAuthServices,
        users::UserServices,
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
}

pub struct AuthClaims(pub Claims);

/// A signed-in admin. Unlike `AuthClaims` it re-reads the role from the
/// database on every request, so a demotion takes effect immediately, and it
/// refuses an admin who still has to change the bootstrap password.
pub struct AdminClaims(pub Claims);

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
        let AuthClaims(claims) = AuthClaims::from_request_parts(parts, state)
            .await
            .map_err(AdminError::Auth)?;
        let user = state
            .user_services
            .get_user(claims.sub)
            .await
            .map_err(|_| AdminError::Forbidden("Admin access required"))?;
        if !user.is_admin() {
            return Err(AdminError::Forbidden("Admin access required"));
        }
        if user.must_change_password {
            return Err(AdminError::Forbidden("Change your password first"));
        }
        Ok(AdminClaims(claims))
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

        Ok(AuthClaims(token_data.claims))
    }
}
