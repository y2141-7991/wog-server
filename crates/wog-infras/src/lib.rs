pub mod billing;
pub mod crawler;
pub mod email;
pub mod errors;
pub mod models;
pub mod password;
pub mod repos;
pub mod services;
pub mod support;

use anyhow::{Ok, Result};
use chrono::Utc;
use envconfig::Envconfig;
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, postgres::PgPoolOptions};
use uuid::Uuid;

use crate::{errors::DatabaseError, models::User};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum TokenType {
    Access,
    Refresh,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: Uuid,
    pub username: String,
    pub token_type: TokenType,
    pub exp: usize,
    pub iat: usize,
    /// The sign-in (a `refresh_sessions` row) this token belongs to. Every token issued
    /// by this server has one; it is optional only so older tokens still decode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sid: Option<Uuid>,
    /// Refresh tokens only: this token's id. Only the session's current one may be used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jti: Option<Uuid>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TokenPair {
    pub access_token: String,
    pub refresh_token: String,
    pub lang: String,
    pub timestamp: i64,
}

#[derive(Debug, Envconfig, Clone)]
pub struct DefaultConfig {
    #[envconfig(from = "DATABASE_URL")]
    pub database_url: String,
    #[envconfig(from = "SERVER_HOST")]
    pub server_host: String,
    #[envconfig(from = "SERVER_PORT")]
    pub server_port: u16,
    #[envconfig(from = "JWT_SECRET")]
    pub jwt_secret: String,
    #[envconfig(from = "REFRESH_TOKEN_SECRET")]
    pub refresh_token_secret: String,
    #[envconfig(from = "ACCESS_TOKEN_EXPIRE_MINUTES")]
    pub access_token_expire_minutes: i64,
    #[envconfig(from = "REFRESH_TOKEN_EXPIRE_MINUTES")]
    pub refresh_token_expire_minutes: i64,
    #[envconfig(from = "CLIENT_URL")]
    pub client_url: String,
    #[envconfig(from = "REST_API_URL")]
    pub rest_api_url: String,
    #[envconfig(from = "SMTP_HOST", default = "")]
    pub smtp_host: String,
    #[envconfig(from = "SMTP_PORT", default = "587")]
    pub smtp_port: u16,
    #[envconfig(from = "SMTP_USERNAME", default = "")]
    pub smtp_username: String,
    #[envconfig(from = "SMTP_PASSWORD", default = "")]
    pub smtp_password: String,
    #[envconfig(from = "SMTP_FROM_EMAIL", default = "")]
    pub smtp_from_email: String,
    #[envconfig(from = "SMTP_FROM_NAME", default = "WoG")]
    pub smtp_from_name: String,
    #[envconfig(from = "EMAIL_VERIFICATION_EXPIRE_HOURS", default = "24")]
    pub email_verification_expire_hours: i64,
    /// gRPC address of the crawler service the gateway forwards to
    /// (social-track-pipeline's `tiktok_gateway`).
    #[envconfig(from = "UPSTREAM_URL", default = "http://127.0.0.1:50051")]
    pub upstream_url: String,
    /// Shared secret sent as `x-upstream-secret` metadata so only this gateway
    /// can call the crawler. Calls fail until it matches the upstream's.
    #[envconfig(from = "UPSTREAM_SECRET", default = "")]
    pub upstream_secret: String,
    /// SePay billing. Billing endpoints are disabled (503) until the webhook
    /// key, bank name, account number and account holder are all set.
    #[envconfig(from = "SEPAY_WEBHOOK_API_KEY", default = "")]
    pub sepay_webhook_api_key: String,
    /// Bearer token for SePay's transactions API (reconciliation). Optional.
    #[envconfig(from = "SEPAY_API_TOKEN", default = "")]
    pub sepay_api_token: String,
    /// Bank name as the QR image endpoint expects it, e.g. `Vietcombank`.
    #[envconfig(from = "SEPAY_BANK_NAME", default = "")]
    pub sepay_bank_name: String,
    #[envconfig(from = "SEPAY_ACCOUNT_NUMBER", default = "")]
    pub sepay_account_number: String,
    #[envconfig(from = "SEPAY_ACCOUNT_HOLDER", default = "")]
    pub sepay_account_holder: String,
    /// Prefix of the payment code; must match the SePay payment-code setting.
    #[envconfig(from = "SEPAY_PAYMENT_CODE_PREFIX", default = "WOG")]
    pub sepay_payment_code_prefix: String,
    #[envconfig(from = "SEPAY_QR_BASE_URL", default = "https://vietqr.app/img")]
    pub sepay_qr_base_url: String,
    #[envconfig(from = "SEPAY_API_BASE", default = "https://my.sepay.vn/userapi")]
    pub sepay_api_base: String,
    #[envconfig(from = "ORDER_TTL_MINUTES", default = "30")]
    pub order_ttl_minutes: i64,
    /// Extra browser origins allowed to open the support-chat WebSocket, comma
    /// separated (e.g. `http://localhost:5173`). `CLIENT_URL` and
    /// `REST_API_URL` are always allowed.
    #[envconfig(from = "ALLOWED_ORIGINS", default = "")]
    pub allowed_origins: String,
    /// Adds `Secure` to the refresh cookie. Leave on in production; turn off only for
    /// plain-http local development in a browser (Safari) that refuses Secure cookies there.
    #[envconfig(from = "COOKIE_SECURE", default = "true")]
    pub cookie_secure: bool,
}

#[derive(Clone)]
pub struct AppConfig {
    pub pool: PgPool,
    pub default_config: DefaultConfig,
}

impl AppConfig {
    fn new(pool: PgPool, default_config: DefaultConfig) -> anyhow::Result<Self> {
        Ok(Self {
            pool,
            default_config,
        })
    }
    /// A matching access/refresh pair for one session. `jti` identifies this refresh
    /// token; the caller stores it in `refresh_sessions` so only the latest one works.
    pub fn issue_tokens(&self, user: &User, sid: Uuid, jti: Uuid) -> Result<TokenPair> {
        Ok(TokenPair {
            access_token: self.issue_access_token(user, sid)?,
            refresh_token: self.build_token(user, TokenType::Refresh, Some(sid), Some(jti))?,
            lang: "en".to_string(),
            timestamp: Utc::now().timestamp(),
        })
    }

    pub fn issue_access_token(&self, user: &User, sid: Uuid) -> Result<String> {
        Ok(self.build_token(user, TokenType::Access, Some(sid), None)?)
    }

    /// When a refresh token issued now stops working.
    pub fn refresh_expires_at(&self) -> chrono::DateTime<Utc> {
        Utc::now() + chrono::Duration::minutes(self.default_config.refresh_token_expire_minutes)
    }

    fn build_token(
        &self,
        user: &User,
        token_type: TokenType,
        sid: Option<Uuid>,
        jti: Option<Uuid>,
    ) -> Result<String, DatabaseError> {
        let now = Utc::now();
        let (expire_minutes, algorithm, secret) = match token_type {
            TokenType::Access => (
                self.default_config.access_token_expire_minutes,
                Algorithm::HS256,
                &self.default_config.jwt_secret,
            ),
            TokenType::Refresh => (
                self.default_config.refresh_token_expire_minutes,
                Algorithm::HS512,
                &self.default_config.refresh_token_secret,
            ),
        };
        let exp = now + chrono::Duration::minutes(expire_minutes);

        let claims = Claims {
            sub: user.id,
            username: user.username.clone(),
            token_type,
            exp: exp.timestamp() as usize,
            iat: now.timestamp() as usize,
            sid,
            jti,
        };

        let header = Header::new(algorithm);
        encode(
            &header,
            &claims,
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .map_err(|e| DatabaseError::Others(format!("JWT encode error: {}", e)))
    }
}

pub async fn get_config() -> anyhow::Result<AppConfig> {
    let default_config = DefaultConfig::init_from_env().expect("Env var not found");
    tracing::info!("Connecting to PostgreSQL...");
    let pool = PgPoolOptions::new()
        .max_connections(20)
        .connect(&default_config.database_url)
        .await?;

    AppConfig::new(pool, default_config)
}
