use std::sync::Arc;

use axum::{Router, routing::get};
use envconfig::Envconfig;
use sqlx::postgres::PgPoolOptions;
use tower_http::cors::{Any, CorsLayer};
use tracing_subscriber::EnvFilter;
use wog_config::config::AppConfig;
use wog_infras::{repos::users::PgUserRepo, services::users::UserServices};
use wog_middleware::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,chatbox=debug")),
        )
        .init();

    let app_config = AppConfig::init_from_env().expect("Env var not found");

    tracing::info!("Connecting to PostgreSQL...");
    let pool = PgPoolOptions::new()
        .max_connections(20)
        .connect(&app_config.database_url)
        .await?;

    tracing::info!("Running migrations...");
    sqlx::migrate!("../../migrations").run(&pool).await?;
    tracing::info!("Migrations complete");

    let user_repo = Arc::new(PgUserRepo::new(pool.clone()));

    let user_services = UserServices::new(user_repo);

    let app_state = AppState {
        app_config: app_config.clone(),
        user_services,
    };

    let app = Router::new()
        .route(
            "/api/v1/user/{id}",
            get(wog_api::routers::user::get_profile),
        )
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .with_state(app_state);

    let addr = format!("{}:{}", app_config.server_host, app_config.server_port);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("Listening on http://{}", listener.local_addr()?);
    axum::serve(listener, app).await?;

    Ok(())
}
