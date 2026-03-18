pub mod errors;
pub mod models;
pub mod repos;
pub mod services;

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
    pub fn generate_token_pair(&self, user: &User) -> Result<TokenPair> {
        let access_token = self.build_token(user.id, &user.username, TokenType::Access)?;
        let refresh_token = self.build_token(user.id, &user.username, TokenType::Refresh)?;
        Ok(TokenPair {
            access_token,
            refresh_token,
            lang: "en".to_string(),
            timestamp: Utc::now().timestamp(),
        })
    }

    pub fn refresh_access_token(&self, refresh_claims: &Claims) -> Result<TokenPair> {
        if refresh_claims.token_type != TokenType::Refresh {
            anyhow::bail!("Invalid token type: expected refresh token");
        }
        let access_token = self.build_token(
            refresh_claims.sub,
            &refresh_claims.username,
            TokenType::Access,
        )?;
        let refresh_token = self.build_token(
            refresh_claims.sub,
            &refresh_claims.username,
            TokenType::Refresh,
        )?;
        Ok(TokenPair {
            access_token,
            refresh_token,
            lang: "en".to_string(),
            timestamp: Utc::now().timestamp(),
        })
    }

    fn build_token(
        &self,
        sub: Uuid,
        username: &str,
        token_type: TokenType,
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
            sub,
            username: username.to_string(),
            token_type,
            exp: exp.timestamp() as usize,
            iat: now.timestamp() as usize,
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
