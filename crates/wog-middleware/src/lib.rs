use axum::{
    extract::FromRequestParts,
    http::{StatusCode, request::Parts},
    response::{IntoResponse, Response},
};
use axum_extra::extract::CookieJar;
use jsonwebtoken::{DecodingKey, Validation, decode};
use serde_json::json;
use wog_infras::{
    AppConfig, Claims,
    services::{oauth::OAuthServices, users::UserServices},
};

#[derive(Clone)]
pub struct AppState {
    pub user_services: UserServices,
    pub app_config: AppConfig,
    pub oauth_services: OAuthServices,
}

pub struct AuthClaims(pub Claims);

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
            tracing::warn!("JWT validation failed: {}", e);
            AuthError("Invalid or expired token".into())
        })?;

        Ok(AuthClaims(token_data.claims))
    }
}
