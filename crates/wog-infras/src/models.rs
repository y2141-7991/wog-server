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

// ── Billing ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Plan {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub price_vnd: i32,
    pub period_days: i32,
    pub monthly_quota: i32,
    pub rate_limit_per_minute: i32,
    pub features: sqlx::types::Json<Vec<String>>,
    pub sort_order: i32,
    pub enabled: bool,
}

/// A user's live subscription together with the plan it grants.
#[derive(Debug, Clone, FromRow)]
pub struct ActiveSubscription {
    pub id: Uuid,
    pub user_id: Uuid,
    pub plan_id: Uuid,
    pub plan_slug: String,
    pub plan_name: String,
    pub plan_sort_order: i32,
    pub monthly_quota: i32,
    pub rate_limit_per_minute: i32,
    pub current_period_start: DateTime<Utc>,
    pub current_period_end: DateTime<Utc>,
}

/// An order with its plan's slug and name; `username` is only filled by the
/// admin queries.
#[derive(Debug, Clone, FromRow)]
pub struct PaymentOrder {
    pub id: Uuid,
    pub user_id: Uuid,
    pub plan_id: Uuid,
    pub plan_slug: String,
    pub plan_name: String,
    pub amount_vnd: i32,
    pub status: String,
    pub payment_code: String,
    pub sepay_transaction_id: Option<i64>,
    pub paid_amount_vnd: Option<i64>,
    pub paid_at: Option<DateTime<Utc>>,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    #[sqlx(default)]
    pub username: Option<String>,
    /// Row count of the whole filtered result; only filled by paged queries.
    #[sqlx(default)]
    pub total: i64,
}

#[derive(Debug, Clone, FromRow)]
pub struct PaymentEvent {
    pub id: Uuid,
    pub source: String,
    pub sepay_transaction_id: Option<i64>,
    pub transfer_type: Option<String>,
    pub amount_vnd: Option<i64>,
    pub payment_code: Option<String>,
    pub content: Option<String>,
    pub matched_order_id: Option<Uuid>,
    pub outcome: String,
    pub note: Option<String>,
    pub needs_review: bool,
    pub created_at: DateTime<Utc>,
    #[sqlx(default)]
    pub total: i64,
}

/// What the billing core did with one transfer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettleOutcome {
    /// Not money in (or not for us); recorded and otherwise ignored.
    Ignored,
    /// No order carries any code found in the transfer.
    Unmatched,
    /// The same transfer was already settled; nothing changed.
    Duplicate,
    /// A second transfer for an order that is already paid.
    AlreadyPaid,
    /// The order was canceled or failed, so the money was not applied.
    CanceledOrder,
    /// Less than the order amount; the order stays pending.
    Underpaid,
    /// The order is paid and the subscription is active or extended.
    Activated {
        order_id: Uuid,
        user_id: Uuid,
        plan_slug: String,
        period_end: DateTime<Utc>,
        late: bool,
        overpaid_vnd: i64,
    },
    /// Paid, but the user already holds a higher plan; flagged for review.
    NotApplied,
}

// ══════════════════════ Support chat ══════════════════════

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, PartialEq)]
pub struct SupportMessage {
    pub id: Uuid,
    pub conversation_id: Uuid,
    pub sender_id: Uuid,
    /// `user` or `admin`.
    pub sender_role: String,
    pub body: String,
    pub client_msg_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub read_at: Option<DateTime<Utc>>,
}

/// A support conversation joined with the user it belongs to: one inbox row.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize, PartialEq)]
pub struct SupportConversationRow {
    pub id: Uuid,
    pub user_id: Uuid,
    /// `open` or `closed`.
    pub status: String,
    pub subject: Option<String>,
    pub last_message_at: DateTime<Utc>,
    pub last_message_preview: String,
    pub user_unread_count: i32,
    pub admin_unread_count: i32,
    pub assigned_admin_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
    pub username: String,
    pub email: String,
    pub avatar_url: Option<String>,
    /// Name of the user's running paid plan, if any.
    pub plan_name: Option<String>,
    /// Whether the user has a live chat connection. Filled in by the service,
    /// not stored.
    #[sqlx(default)]
    pub online: bool,
    /// Matching rows before paging; only filled by the inbox query.
    #[sqlx(default)]
    #[serde(skip)]
    pub total: i64,
}

/// What storing one message did.
#[derive(Debug, Clone)]
pub struct PostedMessage {
    pub message: SupportMessage,
    pub conversation: SupportConversationRow,
    /// The `client_msg_id` was already stored: nothing new was written.
    pub duplicate: bool,
}
