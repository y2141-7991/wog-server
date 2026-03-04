use std::sync::Arc;

use axum::{Router, http, routing::get};
use bytes::Bytes;
use envconfig::Envconfig;
use sqlx::postgres::PgPoolOptions;
use tower_http::cors::{Any, CorsLayer};
use tracing_subscriber::EnvFilter;
use utoipa::{OpenApi, openapi};
use utoipa_axum::router::OpenApiRouter;
use utoipa_scalar::{Scalar, Servable};
use wog_api::api_routes;
use wog_config::{config::AppConfig, user::dto::UserResponse};
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

    let (api_router, openapi) = OpenApiRouter::<AppState>::with_openapi(ApiDoc::openapi())
        .merge(api_routes())
        .split_for_parts();

    let openapi_json: Bytes = openapi.to_json().expect("Failed to serialize OPENAPI").into();
    let app = Router::new()
        .route("/api-docs/openapi.json", get({
            let spec = openapi_json.clone();
            || async move {
                (
                    [(http::header::CONTENT_TYPE, "application/json")],
                    spec.clone(),
                )
            }
        }))
        .merge(Scalar::with_url("/scalar", openapi.clone()))
        .merge(api_router)
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

#[derive(OpenApi)]
#[openapi(
    info(
        title="",
        version="",
        description=""
    ),
    components(schemas(
        UserResponse
    )),
    tags((name="", description="")),
    modifiers(&SecurityAddon)
)]
struct ApiDoc;

struct SecurityAddon;
impl utoipa::Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "bearer_auth",
            utoipa::openapi::security::SecurityScheme::Http(utoipa::openapi::security::Http::new(
                utoipa::openapi::security::HttpAuthScheme::Bearer,
            )),
        );
    }
}
