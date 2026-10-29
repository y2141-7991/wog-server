use std::sync::Arc;

use axum::{
    Json, Router,
    http::{self, StatusCode},
    routing::get,
};
use bytes::Bytes;
use tower_http::{
    cors::{Any, CorsLayer},
    trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer},
};
use tracing::Level;
use tracing_subscriber::{EnvFilter, fmt};
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_scalar::{Scalar, Servable};
use wog_api::{
    api_routes,
    routers::{gateway_routes, oauth_routes},
};
use wog_config::user::dto::UserResponse;
use wog_infras::{
    crawler::Crawler,
    email::EmailService,
    get_config,
    repos::{
        admin::PgAdminRepo, marketplace::PgMarketplaceRepo, oauth::PgOAuthRepo, users::PgUserRepo,
    },
    services::{
        admin::AdminServices, marketplace::MarketplaceServices, oauth::OAuthServices,
        users::UserServices,
    },
};
use wog_middleware::{
    AppState,
    rate_limit::{RateLimiter, rate_limit_middleware},
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,wog_api=debug,wog_infras=debug"));

    if std::env::var("LOG_FORMAT").as_deref() == Ok("json") {
        fmt().json().with_env_filter(env_filter).init();
    } else {
        fmt().with_env_filter(env_filter).with_target(true).init();
    };

    let app_config = get_config().await?;

    tracing::info!("Running migrations...");
    sqlx::migrate!("../../migrations")
        .run(&app_config.pool)
        .await?;
    tracing::info!("Migrations complete");

    let user_repo = Arc::new(PgUserRepo::new(app_config.pool.clone()));
    let oauth_repo = Arc::new(PgOAuthRepo::new(app_config.pool.clone(), "google"));
    let marketplace_repo = Arc::new(PgMarketplaceRepo::new(app_config.pool.clone()));
    let admin_repo = Arc::new(PgAdminRepo::new(app_config.pool.clone()));

    let user_services = UserServices::new(user_repo);
    let oauth_services = OAuthServices::new(oauth_repo);
    let email_service = EmailService::new(&app_config.default_config);
    let crawler = Crawler::new(
        &app_config.default_config.upstream_url,
        &app_config.default_config.upstream_secret,
    )?;
    let admin_services = AdminServices::new(admin_repo, marketplace_repo.clone());
    let marketplace_services = MarketplaceServices::new(marketplace_repo, crawler);

    match admin_services.bootstrap_admin().await {
        Ok(Some(admin)) => tracing::warn!(
            username = %admin.username,
            "Created the default admin (username 'admin', password 'admin'); change the password after signing in"
        ),
        Ok(None) => tracing::debug!("An admin already exists; skipping the admin bootstrap"),
        Err(err) => tracing::warn!(%err, "Skipped the default admin bootstrap"),
    }

    let app_state = AppState {
        user_services,
        oauth_services,
        app_config: app_config.clone(),
        email_service,
        marketplace_services,
        admin_services,
    };

    let (api_router, openapi) = OpenApiRouter::<AppState>::with_openapi(ApiDoc::openapi())
        .merge(api_routes())
        .split_for_parts();

    let openapi_json: Bytes = openapi
        .to_json()
        .expect("Failed to serialize OPENAPI")
        .into();
    // 100 requests per 60 seconds per client
    let rate_limiter = RateLimiter::new(100, std::time::Duration::from_secs(60));

    let app = Router::new()
        .route("/helloworld", get(hello_world))
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
        .merge(gateway_routes())
        .merge(api_router)
        .layer(axum::middleware::from_fn(rate_limit_middleware))
        .layer(axum::Extension(rate_limiter))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
                .on_response(DefaultOnResponse::new().level(Level::INFO)),
        )
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
