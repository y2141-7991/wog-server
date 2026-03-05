pub mod errors;
pub mod models;
pub mod repos;
pub mod services;

use anyhow::Ok;
use envconfig::Envconfig;
use sqlx::{PgPool, postgres::PgPoolOptions};
use wog_oauth::OAuthServices;

#[derive(Debug, Envconfig, Clone)]
pub struct DefaultConfig {
    #[envconfig(from = "DATABASE_URL")]
    pub database_url: String,
    #[envconfig(from = "SERVER_HOST")]
    pub server_host: String,
    #[envconfig(from = "SERVER_PORT")]
    pub server_port: u16,
}

pub struct AppConfig {
    pub pool: PgPool,
    pub oauth_services: OAuthServices,
    pub default_config: DefaultConfig,
}

impl AppConfig {
    fn new(pool: PgPool, default_config: DefaultConfig) -> anyhow::Result<Self> {
        let oauth_services = OAuthServices::new("google");
        Ok(Self {
            pool,
            oauth_services,
            default_config,
        })
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
