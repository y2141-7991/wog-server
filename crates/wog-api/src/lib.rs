use utoipa_axum::router::OpenApiRouter;

pub mod errors;
mod migration;
pub mod routers;
use routers::*;
use wog_middleware::AppState;

pub fn api_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().merge(user_routes())
}
