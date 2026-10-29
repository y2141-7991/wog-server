use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, FromRow, Deserialize, Serialize)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    #[serde(skip_serializing)]
    password: Option<String>,
    pub avatar_url: Option<String>,
    pub is_online: bool,
    pub email_verified: bool,
    pub role: String,
    pub must_change_password: bool,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl User {
    pub fn is_admin(&self) -> bool {
        self.role == "admin"
    }

    /// The stored Argon2 password hash, if this account has one set.
    /// `None` for accounts created via OAuth that never set a password.
    pub fn password_hash(&self) -> Option<&str> {
        self.password.as_deref()
    }
}

#[derive(Debug, Clone, FromRow, Deserialize, Serialize)]
pub struct EmailVerificationToken {
    pub id: Uuid,
    pub user_id: Uuid,
    pub token: String,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Deserialize, Serialize)]
pub struct UserProfile {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub avatar_url: Option<String>,
    pub is_online: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(FromRow)]
pub struct OAuthVerifierRow {
    pub id: Uuid,
    pub csrf_token: String,
    pub pkce_verifier: String,
    pub created_at: DateTime<Utc>,
}

// ══════════════════════ API marketplace ══════════════════════

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Api {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub description: String,
    pub provider: String,
    pub category: String,
    pub gateway_path: String,
    /// Name of the crawler method this API calls.
    pub upstream_method: String,
    pub params: sqlx::types::Json<Vec<ApiParam>>,
    pub example_response: Option<sqlx::types::Json<serde_json::Value>>,
    pub rate_limit_per_minute: i32,
    pub monthly_quota: i32,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Live metrics over the last 30 days; only filled by the catalog queries.
    #[sqlx(default)]
    pub avg_latency_ms: Option<i32>,
    #[sqlx(default)]
    pub success_rate: Option<f64>,
    #[sqlx(default)]
    pub total_calls: i64,
}

/// One documented query parameter of a catalog API. Only declared
/// parameters are forwarded upstream.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiParam {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub required: bool,
    pub description: String,
    pub example: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ApiKey {
    pub id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    pub key_prefix: String,
    #[serde(skip_serializing)]
    pub key_hash: String,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, FromRow)]
pub struct ApiUsageByApi {
    pub slug: String,
    pub name: String,
    pub monthly_quota: i32,
    pub requests: i64,
    pub errors: i64,
    pub used_this_month: i64,
}

#[derive(Debug, Clone, FromRow)]
pub struct ApiUsageByDay {
    pub day: chrono::NaiveDate,
    pub requests: i64,
}

// ══════════════════════ Admin ══════════════════════

/// One user's metered usage over a look-back window, for the admin table.
#[derive(Debug, Clone, FromRow)]
pub struct AdminUserUsage {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub role: String,
    pub created_at: DateTime<Utc>,
    pub requests: i64,
    pub errors: i64,
    pub last_active: Option<DateTime<Utc>>,
    pub active_keys: i64,
    /// Users matching the search, across all pages.
    pub total: i64,
}

#[derive(Debug, Clone, FromRow)]
pub struct AdminTotals {
    pub total_users: i64,
    pub active_users: i64,
    pub requests: i64,
    pub errors: i64,
    pub avg_latency_ms: Option<i32>,
}

/// One catalog API's traffic across all users.
#[derive(Debug, Clone, FromRow)]
pub struct AdminApiUsage {
    pub slug: String,
    pub name: String,
    pub requests: i64,
    pub errors: i64,
    pub avg_latency_ms: Option<i32>,
    pub users: i64,
}

#[derive(Debug, Clone, FromRow)]
pub struct AuditLogEntry {
    pub id: Uuid,
    pub actor_username: Option<String>,
    pub target_user_id: Option<Uuid>,
    pub target_username: Option<String>,
    pub action: String,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
    pub created_at: DateTime<Utc>,
}
