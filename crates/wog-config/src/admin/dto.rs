use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;
use wog_infras::models::{AdminApiUsage, AdminTotals, AdminUserUsage, AuditLogEntry};

use crate::{
    marketplace::dto::{ApiKeyResponse, UsageByApiResponse, UsageByDayResponse},
    user::dto::UserResponse,
};

#[derive(Debug, Deserialize, IntoParams)]
pub struct OverviewQuery {
    /// Look-back window in days (1-90, default 30)
    pub days: Option<i32>,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct UsersQuery {
    /// Matches username or email
    pub q: Option<String>,
    /// Look-back window in days (1-90, default 30)
    pub days: Option<i32>,
    /// Zero-based page
    pub page: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TotalsResponse {
    pub total_users: i64,
    /// Users who made at least one request in the window
    pub active_users: i64,
    pub requests: i64,
    pub errors: i64,
    pub avg_latency_ms: Option<i32>,
}

impl From<AdminTotals> for TotalsResponse {
    fn from(v: AdminTotals) -> Self {
        Self {
            total_users: v.total_users,
            active_users: v.active_users,
            requests: v.requests,
            errors: v.errors,
            avg_latency_ms: v.avg_latency_ms,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ApiTrafficResponse {
    pub slug: String,
    pub name: String,
    pub requests: i64,
    pub errors: i64,
    pub avg_latency_ms: Option<i32>,
    pub users: i64,
}

impl From<AdminApiUsage> for ApiTrafficResponse {
    fn from(v: AdminApiUsage) -> Self {
        Self {
            slug: v.slug,
            name: v.name,
            requests: v.requests,
            errors: v.errors,
            avg_latency_ms: v.avg_latency_ms,
            users: v.users,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct OverviewResponse {
    pub days: i32,
    pub totals: TotalsResponse,
    pub by_api: Vec<ApiTrafficResponse>,
    pub by_day: Vec<UsageByDayResponse>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UserUsageRow {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub role: String,
    pub created_at: DateTime<Utc>,
    pub requests: i64,
    pub errors: i64,
    pub last_active: Option<DateTime<Utc>>,
    pub active_keys: i64,
}

impl From<AdminUserUsage> for UserUsageRow {
    fn from(v: AdminUserUsage) -> Self {
        Self {
            id: v.id,
            username: v.username,
            email: v.email,
            role: v.role,
            created_at: v.created_at,
            requests: v.requests,
            errors: v.errors,
            last_active: v.last_active,
            active_keys: v.active_keys,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UsersResponse {
    pub days: i32,
    pub page: i64,
    pub per_page: i64,
    pub total: i64,
    pub users: Vec<UserUsageRow>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UserDetailResponse {
    pub user: UserResponse,
    pub days: i32,
    pub by_api: Vec<UsageByApiResponse>,
    pub by_day: Vec<UsageByDayResponse>,
    pub keys: Vec<ApiKeyResponse>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SetRoleRequest {
    pub role: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AuditLogResponse {
    pub id: Uuid,
    pub actor_username: Option<String>,
    pub target_user_id: Option<Uuid>,
    pub target_username: Option<String>,
    pub action: String,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl From<AuditLogEntry> for AuditLogResponse {
    fn from(v: AuditLogEntry) -> Self {
        Self {
            id: v.id,
            actor_username: v.actor_username,
            target_user_id: v.target_user_id,
            target_username: v.target_username,
            action: v.action,
            old_value: v.old_value,
            new_value: v.new_value,
            created_at: v.created_at,
        }
    }
}
