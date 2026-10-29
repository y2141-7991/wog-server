use std::collections::HashMap;

use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use uuid::Uuid;
use wog_config::marketplace::dto::{
    ApiKeyResponse, ApiResponse, CreateApiKeyRequest, CreateApiKeyResponse, UsageQuery,
    UsageResponse,
};
use wog_middleware::{AppState, AuthClaims};

use crate::errors::{RestApiError, RestApiResponseError};

#[utoipa::path(
    get,
    path = "/api/v1/marketplace/apis",
    tag = "Marketplace",
    responses(
        (status = 200, description = "The API catalog", body = Vec<ApiResponse>),
        (status = 500, description = "Internal error", body = RestApiResponseError),
    )
)]
#[axum::debug_handler]
pub async fn list_apis(State(state): State<AppState>) -> Result<Json<Vec<ApiResponse>>, RestApiError> {
    let apis = state.marketplace_services.list_apis().await?;
    Ok(Json(apis.into_iter().map(Into::into).collect()))
}

#[utoipa::path(
    get,
    path = "/api/v1/marketplace/apis/{slug}",
    tag = "Marketplace",
    params(("slug" = String, Path, description = "API slug, e.g. tiktok-post")),
    responses(
        (status = 200, description = "One catalog API", body = ApiResponse),
        (status = 404, description = "API not found", body = RestApiResponseError),
        (status = 500, description = "Internal error", body = RestApiResponseError),
    )
)]
#[axum::debug_handler]
pub async fn get_api(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<ApiResponse>, RestApiError> {
    Ok(Json(state.marketplace_services.get_api(&slug).await?.into()))
}

#[utoipa::path(
    post,
    path = "/api/v1/marketplace/keys",
    tag = "Marketplace",
    request_body(content = CreateApiKeyRequest, content_type = "application/json"),
    responses(
        (status = 200, description = "Key created; the secret is shown only this once", body = CreateApiKeyResponse),
        (status = 400, description = "Invalid name or too many keys", body = RestApiResponseError),
        (status = 401, description = "Unauthorized", body = RestApiResponseError),
        (status = 500, description = "Internal error", body = RestApiResponseError),
    ),
    security(("bearer_auth" = []))
)]
#[axum::debug_handler]
pub async fn create_key(
    State(state): State<AppState>,
    AuthClaims(claims): AuthClaims,
    Json(payload): Json<CreateApiKeyRequest>,
) -> Result<Json<CreateApiKeyResponse>, RestApiError> {
    let (key, secret) = state
        .marketplace_services
        .create_key(claims.sub, &payload.name)
        .await?;
    Ok(Json(CreateApiKeyResponse {
        key: key.into(),
        secret,
    }))
}

#[utoipa::path(
    get,
    path = "/api/v1/marketplace/keys",
    tag = "Marketplace",
    responses(
        (status = 200, description = "Your API keys", body = Vec<ApiKeyResponse>),
        (status = 401, description = "Unauthorized", body = RestApiResponseError),
        (status = 500, description = "Internal error", body = RestApiResponseError),
    ),
    security(("bearer_auth" = []))
)]
#[axum::debug_handler]
pub async fn list_keys(
    State(state): State<AppState>,
    AuthClaims(claims): AuthClaims,
) -> Result<Json<Vec<ApiKeyResponse>>, RestApiError> {
    let keys = state.marketplace_services.list_keys(claims.sub).await?;
    Ok(Json(keys.into_iter().map(Into::into).collect()))
}

#[utoipa::path(
    delete,
    path = "/api/v1/marketplace/keys/{key_id}",
    tag = "Marketplace",
    params(("key_id" = Uuid, Path, description = "API key ID")),
    responses(
        (status = 204, description = "Key revoked"),
        (status = 401, description = "Unauthorized", body = RestApiResponseError),
        (status = 404, description = "Key not found", body = RestApiResponseError),
        (status = 500, description = "Internal error", body = RestApiResponseError),
    ),
    security(("bearer_auth" = []))
)]
#[axum::debug_handler]
pub async fn revoke_key(
    State(state): State<AppState>,
    AuthClaims(claims): AuthClaims,
    Path(key_id): Path<Uuid>,
) -> Result<StatusCode, RestApiError> {
    state
        .marketplace_services
        .revoke_key(claims.sub, key_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/api/v1/marketplace/usage",
    tag = "Marketplace",
    params(UsageQuery),
    responses(
        (status = 200, description = "Your API usage", body = UsageResponse),
        (status = 401, description = "Unauthorized", body = RestApiResponseError),
        (status = 500, description = "Internal error", body = RestApiResponseError),
    ),
    security(("bearer_auth" = []))
)]
#[axum::debug_handler]
pub async fn get_usage(
    State(state): State<AppState>,
    AuthClaims(claims): AuthClaims,
    Query(query): Query<UsageQuery>,
) -> Result<Json<UsageResponse>, RestApiError> {
    let days = query.days.unwrap_or(30).clamp(1, 90);
    let (by_api, by_day) = state.marketplace_services.usage(claims.sub, days).await?;
    Ok(Json(UsageResponse {
        days,
        by_api: by_api.into_iter().map(Into::into).collect(),
        by_day: by_day.into_iter().map(Into::into).collect(),
    }))
}

/// The metered gateway: `GET /gateway/v1/{provider}/{endpoint}` with an
/// `X-API-Key` header. Authenticated by API key, not by a user session.
pub async fn gateway(
    State(state): State<AppState>,
    Path((provider, endpoint)): Path<(String, String)>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Response, RestApiError> {
    let api_key = headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| RestApiError::Unauthorized("Missing X-API-Key header".into()))?;

    let result = state
        .marketplace_services
        .call_api(
            api_key,
            &format!("/gateway/v1/{provider}/{endpoint}"),
            &query,
        )
        .await?;

    let status = StatusCode::from_u16(result.status).unwrap_or(StatusCode::BAD_GATEWAY);
    Ok((status, Json(result.body)).into_response())
}
