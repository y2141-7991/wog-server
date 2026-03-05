use std::sync::Arc;

use axum::{Json, Router, http::{self, StatusCode}, routing::get};
use bytes::Bytes;
use tower_http::cors::{Any, CorsLayer};
use tracing_subscriber::EnvFilter;
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_scalar::{Scalar, Servable};
use wog_api::{api_routes, routers::oauth_routes};
use wog_config::user::dto::UserResponse;
use wog_infras::{get_config, repos::{oauth::PgOAuthRepo, users::PgUserRepo}, services::{oauth::OAuthServices, users::UserServices}};
use wog_middleware::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,chatbox=debug")),
        )
        .init();

    let app_config = get_config().await?;

    tracing::info!("Running migrations...");
    sqlx::migrate!("../../migrations")
        .run(&app_config.pool)
        .await?;
    tracing::info!("Migrations complete");

    let user_repo = Arc::new(PgUserRepo::new(app_config.pool.clone()));
    let oauth_repo = Arc::new(PgOAuthRepo::new(app_config.pool.clone(), "google"));

    let user_services = UserServices::new(user_repo);
    let oauth_services = OAuthServices::new(oauth_repo);

    let app_state = AppState { user_services, oauth_services, app_config: app_config.clone() };

    let (api_router, openapi) = OpenApiRouter::<AppState>::with_openapi(ApiDoc::openapi())
        .merge(api_routes())
        .split_for_parts();

    let openapi_json: Bytes = openapi
        .to_json()
        .expect("Failed to serialize OPENAPI")
        .into();
    let app = Router::new()
        .route("/", get(hello_world))
        .route(
            "/api-docs/openapi.json",
            get({
                let spec = openapi_json.clone();
                || async move {
                    (
                        [(http::header::CONTENT_TYPE, "application/json")],
                        spec.clone(),
                    )
                }
            }),
        )
        .merge(Scalar::with_url("/scalar", openapi.clone()))
        .merge(oauth_routes())
        .merge(api_router)
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .with_state(app_state);

    let addr = format!(
        "{}:{}",
        app_config.default_config.server_host, app_config.default_config.server_port
    );

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("Listening on http://{}", listener.local_addr()?);
    axum::serve(listener, app).await?;

    Ok(())
}

#[derive(OpenApi)]
#[openapi(
    info(
        title="WoG Api",
        version="0.1.0",
        description="Application API"
    ),
    components(schemas(
        UserResponse
    )),
    tags((name="API", description="Application API")),
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

async fn hello_world() -> Result<(http::StatusCode, axum::Json<String>), http::StatusCode> {
    tracing::info!("Healthcheck!!");
    Ok((StatusCode::OK, Json("Hello world!!!".to_string())))
}
