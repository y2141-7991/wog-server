use axum::{
    Json,
    extract::{Path, Query, State},
};
use uuid::Uuid;
use wog_config::{
    admin::dto::{
        AuditLogResponse, OverviewQuery, OverviewResponse, SetRoleRequest, UserDetailResponse,
        UsersQuery, UsersResponse,
    },
    user::dto::UserResponse,
};
use wog_middleware::{AdminClaims, AppState};

use crate::errors::{RestApiError, RestApiResponseError};

const USERS_PER_PAGE: i64 = 25;

#[utoipa::path(
    get,
    path = "/api/v1/admin/overview",
    tag = "Admin",
    params(OverviewQuery),
    responses(
        (status = 200, description = "Platform-wide usage", body = OverviewResponse),
        (status = 401, description = "Unauthorized", body = RestApiResponseError),
        (status = 403, description = "Not an admin", body = RestApiResponseError),
        (status = 500, description = "Internal error", body = RestApiResponseError),
    ),
    security(("bearer_auth" = []))
)]
#[axum::debug_handler]
pub async fn get_overview(
    State(state): State<AppState>,
    AdminClaims(_claims): AdminClaims,
    Query(query): Query<OverviewQuery>,
) -> Result<Json<OverviewResponse>, RestApiError> {
    let days = query.days.unwrap_or(30).clamp(1, 90);
    let (totals, by_api, by_day) = state.admin_services.overview(days).await?;
    Ok(Json(OverviewResponse {
        days,
        totals: totals.into(),
        by_api: by_api.into_iter().map(Into::into).collect(),
        by_day: by_day.into_iter().map(Into::into).collect(),
    }))
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/users",
    tag = "Admin",
    params(UsersQuery),
    responses(
        (status = 200, description = "Every user with their usage, busiest first", body = UsersResponse),
        (status = 401, description = "Unauthorized", body = RestApiResponseError),
        (status = 403, description = "Not an admin", body = RestApiResponseError),
        (status = 500, description = "Internal error", body = RestApiResponseError),
    ),
    security(("bearer_auth" = []))
)]
#[axum::debug_handler]
pub async fn list_users(
    State(state): State<AppState>,
    AdminClaims(_claims): AdminClaims,
    Query(query): Query<UsersQuery>,
) -> Result<Json<UsersResponse>, RestApiError> {
    let days = query.days.unwrap_or(30).clamp(1, 90);
    let page = query.page.unwrap_or(0).max(0);
    let rows = state
        .admin_services
        .users(query.q.as_deref(), days, page, USERS_PER_PAGE)
        .await?;
    Ok(Json(UsersResponse {
        days,
        page,
        per_page: USERS_PER_PAGE,
        total: rows.first().map_or(0, |r| r.total),
        users: rows.into_iter().map(Into::into).collect(),
    }))
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/users/{id}/usage",
    tag = "Admin",
    params(("id" = Uuid, Path, description = "User ID"), OverviewQuery),
    responses(
        (status = 200, description = "One user's usage and keys", body = UserDetailResponse),
        (status = 401, description = "Unauthorized", body = RestApiResponseError),
        (status = 403, description = "Not an admin", body = RestApiResponseError),
        (status = 404, description = "User not found", body = RestApiResponseError),
        (status = 500, description = "Internal error", body = RestApiResponseError),
    ),
    security(("bearer_auth" = []))
)]
#[axum::debug_handler]
pub async fn get_user_usage(
    State(state): State<AppState>,
    AdminClaims(_claims): AdminClaims,
    Path(user_id): Path<Uuid>,
    Query(query): Query<OverviewQuery>,
) -> Result<Json<UserDetailResponse>, RestApiError> {
    let days = query.days.unwrap_or(30).clamp(1, 90);
    let user = state.user_services.get_user(user_id).await?;
    let (by_api, by_day, keys) = state.admin_services.user_usage(user_id, days).await?;
    Ok(Json(UserDetailResponse {
        user: user.into(),
        days,
        by_api: by_api.into_iter().map(Into::into).collect(),
        by_day: by_day.into_iter().map(Into::into).collect(),
        keys: keys.into_iter().map(Into::into).collect(),
    }))
}

#[utoipa::path(
    put,
    path = "/api/v1/admin/users/{id}/role",
    tag = "Admin",
    params(("id" = Uuid, Path, description = "User ID")),
    request_body(content = SetRoleRequest, content_type = "application/json"),
    responses(
        (status = 200, description = "Role changed (and audited)", body = UserResponse),
        (status = 400, description = "Invalid role, own account, or last admin", body = RestApiResponseError),
        (status = 401, description = "Unauthorized", body = RestApiResponseError),
        (status = 403, description = "Not an admin", body = RestApiResponseError),
        (status = 404, description = "User not found", body = RestApiResponseError),
        (status = 500, description = "Internal error", body = RestApiResponseError),
    ),
    security(("bearer_auth" = []))
)]
#[axum::debug_handler]
pub async fn set_role(
    State(state): State<AppState>,
    AdminClaims(claims): AdminClaims,
    Path(user_id): Path<Uuid>,
    Json(payload): Json<SetRoleRequest>,
) -> Result<Json<UserResponse>, RestApiError> {
    let user = state
        .admin_services
        .set_role(claims.sub, user_id, &payload.role)
        .await?;
    Ok(Json(user.into()))
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/audit-log",
    tag = "Admin",
    responses(
        (status = 200, description = "Most recent admin actions", body = Vec<AuditLogResponse>),
        (status = 401, description = "Unauthorized", body = RestApiResponseError),
        (status = 403, description = "Not an admin", body = RestApiResponseError),
        (status = 500, description = "Internal error", body = RestApiResponseError),
    ),
    security(("bearer_auth" = []))
)]
#[axum::debug_handler]
pub async fn get_audit_log(
    State(state): State<AppState>,
    AdminClaims(_claims): AdminClaims,
) -> Result<Json<Vec<AuditLogResponse>>, RestApiError> {
    let entries = state.admin_services.audit_log().await?;
    Ok(Json(entries.into_iter().map(Into::into).collect()))
}
