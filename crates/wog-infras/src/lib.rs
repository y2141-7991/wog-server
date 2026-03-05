pub mod errors;
pub mod models;
pub mod repos;
pub mod services;

use anyhow::Ok;
use chrono::Utc;
use envconfig::Envconfig;
use jsonwebtoken::{EncodingKey, Header, encode};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, postgres::PgPoolOptions};
use uuid::Uuid;

use crate::{errors::DatabaseError, models::User};


#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: Uuid,
    pub username: String,
    pub exp: usize,
    pub iat: usize,
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
    #[envconfig(from = "JWT_EXPIRATION_HOURS")]
    pub jwt_expiration_hours: i64,
    #[envconfig(from = "CLIENT_URL")]
    pub client_url: String,
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
    pub fn generate_token(&self, user: User) -> Result<String, DatabaseError> {
        let now = Utc::now();
        let exp = now + chrono::Duration::hours(self.default_config.jwt_expiration_hours);

        let claims = Claims {
            sub: user.id,
            username: user.username.clone(),
            exp: exp.timestamp() as usize,
            iat: now.timestamp() as usize,
        };

        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(self.default_config.jwt_secret.as_bytes()),
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
