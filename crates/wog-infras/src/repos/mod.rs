use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;

use chrono::{DateTime, Utc};

use crate::{
    billing::Transfer,
    errors::DatabaseError,
    models::{
        ActiveSubscription, AdminApiUsage, AdminTotals, AdminUserUsage, Api, ApiKey, ApiUsageByApi,
        ApiUsageByDay, AuditLogEntry, EmailVerificationToken, PaymentEvent, PaymentOrder, Plan,
        PostedMessage, SettleOutcome, SupportConversationRow, SupportMessage, User,
    },
    repos::oauth::OAuthConnection,
};

pub mod admin;
pub mod billing;
pub mod marketplace;
pub mod oauth;
pub mod support;
pub mod users;

pub type DynUserRepository = Arc<dyn UserRepository + Send + Sync>;
pub type DynOAuthRepository = Arc<dyn OAuthRepository + Send + Sync>;
pub type DynAdminRepository = Arc<dyn AdminRepository + Send + Sync>;
pub type DynMarketplaceRepository = Arc<dyn MarketplaceRepository + Send + Sync>;
pub type DynBillingRepository = Arc<dyn BillingRepository + Send + Sync>;
pub type DynSupportRepository = Arc<dyn SupportRepository + Send + Sync>;

#[async_trait]
pub trait UserRepository {
    async fn create(
        &self,
        id: Uuid,
        username: &str,
        email: &str,
        hash_pwd: &str,
    ) -> Result<User, DatabaseError>;

    async fn find_by_id(&self, id: Uuid) -> Result<Option<User>, DatabaseError>;

    async fn find_by_email(&self, email: &str) -> Result<Option<User>, DatabaseError>;

    async fn find_by_username(&self, username: &str) -> Result<Option<User>, DatabaseError>;

    async fn create_verification_token(
        &self,
        user_id: Uuid,
        token: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<(), DatabaseError>;

    async fn find_verification_token(
        &self,
        token: &str,
    ) -> Result<Option<EmailVerificationToken>, DatabaseError>;

    async fn delete_verification_token(&self, token: &str) -> Result<(), DatabaseError>;

    async fn mark_email_verified(&self, user_id: Uuid) -> Result<User, DatabaseError>;

    /// Store a new password hash and clear the forced-change flag.
    async fn update_password(&self, user_id: Uuid, hash_pwd: &str) -> Result<User, DatabaseError>;
}

#[async_trait]
pub trait OAuthRepository {
    async fn oauth_auth_url(&self) -> Result<String, DatabaseError>;
    async fn exchange_code(
        &self,
        code: String,
        csrf: String,
    ) -> Result<OAuthConnection, DatabaseError>;

    async fn find_by_oauth(&self, provider: &str, sub: &str)
    -> Result<Option<User>, DatabaseError>;

    async fn create_oauth_user(
        &self,
        id: Uuid,
        email: &str,
        username: &str,
        avatar_url: &str,
        provider: &str,
        sub: &str,
    ) -> Result<User, DatabaseError>;
}

#[async_trait]
pub trait MarketplaceRepository {
    async fn list_apis(&self) -> Result<Vec<Api>, DatabaseError>;

    async fn find_api_by_slug(&self, slug: &str) -> Result<Option<Api>, DatabaseError>;

    async fn find_api_by_gateway_path(&self, path: &str) -> Result<Option<Api>, DatabaseError>;

    async fn create_key(
        &self,
        user_id: Uuid,
        name: &str,
        key_prefix: &str,
        key_hash: &str,
    ) -> Result<ApiKey, DatabaseError>;

    async fn list_keys(&self, user_id: Uuid) -> Result<Vec<ApiKey>, DatabaseError>;

    /// Revoke one of `user_id`'s own keys. `ValueNotFound` if it isn't theirs
    /// or is already revoked.
    async fn revoke_key(&self, user_id: Uuid, key_id: Uuid) -> Result<(), DatabaseError>;

    /// The key with this hash, only if it has not been revoked.
    async fn find_active_key_by_hash(
        &self,
        key_hash: &str,
    ) -> Result<Option<ApiKey>, DatabaseError>;

    /// Log a request forwarded upstream and stamp the key's `last_used_at`.
    async fn record_usage(
        &self,
        key: &ApiKey,
        api_id: Uuid,
        status_code: i16,
        latency_ms: i32,
    ) -> Result<(), DatabaseError>;

    /// Billable requests (upstream did not 5xx) `user_id` has made to `api_id`
    /// since the start of this UTC month.
    async fn count_usage_this_month(
        &self,
        user_id: Uuid,
        api_id: Uuid,
    ) -> Result<i64, DatabaseError>;

    /// The user's paid subscription, if one is running right now.
    async fn find_active_subscription(
        &self,
        user_id: Uuid,
    ) -> Result<Option<ActiveSubscription>, DatabaseError>;

    /// Billable requests `user_id` made across all APIs since `since`.
    async fn count_usage_since(
        &self,
        user_id: Uuid,
        since: DateTime<Utc>,
    ) -> Result<i64, DatabaseError>;

    async fn usage_by_api(
        &self,
        user_id: Uuid,
        days: i32,
    ) -> Result<Vec<ApiUsageByApi>, DatabaseError>;

    async fn usage_by_day(
        &self,
        user_id: Uuid,
        days: i32,
    ) -> Result<Vec<ApiUsageByDay>, DatabaseError>;
}

#[async_trait]
pub trait AdminRepository {
    /// Platform-wide totals over the last `days` days.
    async fn totals(&self, days: i32) -> Result<AdminTotals, DatabaseError>;

    /// Traffic per catalog API over the last `days` days.
    async fn api_breakdown(&self, days: i32) -> Result<Vec<AdminApiUsage>, DatabaseError>;

    /// Requests per UTC day over the last `days` days, zero-filled.
    async fn daily_requests(&self, days: i32) -> Result<Vec<ApiUsageByDay>, DatabaseError>;

    /// Every user with their usage over the last `days` days, busiest first.
    /// `search` matches username or email, case-insensitively.
    async fn list_users_usage(
        &self,
        search: Option<&str>,
        days: i32,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<AdminUserUsage>, DatabaseError>;

    /// Change a user's role and write the audit row in one transaction.
    /// Refuses to demote the last remaining admin.
    async fn set_role(
        &self,
        actor_id: Uuid,
        target_id: Uuid,
        role: &str,
    ) -> Result<User, DatabaseError>;

    async fn list_audit_log(&self, limit: i64) -> Result<Vec<AuditLogEntry>, DatabaseError>;

    /// Create the built-in admin. Does nothing (returns `None`) when an admin
    /// already exists; errors when the username or email is already taken by
    /// a regular account (it is never promoted, its password is unknown).
    async fn bootstrap_admin(
        &self,
        username: &str,
        email: &str,
        hash_pwd: &str,
    ) -> Result<Option<User>, DatabaseError>;
}

#[async_trait]
pub trait BillingRepository {
    /// Enabled plans, cheapest first.
    async fn list_plans(&self) -> Result<Vec<Plan>, DatabaseError>;

    async fn find_plan(&self, slug: &str) -> Result<Option<Plan>, DatabaseError>;

    async fn find_active_subscription(
        &self,
        user_id: Uuid,
    ) -> Result<Option<ActiveSubscription>, DatabaseError>;

    async fn count_usage_since(
        &self,
        user_id: Uuid,
        since: DateTime<Utc>,
    ) -> Result<i64, DatabaseError>;

    /// Mark this user's pending orders past their expiry as expired.
    async fn expire_user_pending(&self, user_id: Uuid) -> Result<u64, DatabaseError>;

    /// The user's unexpired pending order for a plan.
    async fn find_pending_order(
        &self,
        user_id: Uuid,
        plan_id: Uuid,
    ) -> Result<Option<PaymentOrder>, DatabaseError>;

    /// Insert a pending order at the plan's current price. `UniqueViolation`
    /// when the payment code is taken or the user already has a pending order
    /// for the plan.
    async fn create_order(
        &self,
        user_id: Uuid,
        plan: &Plan,
        payment_code: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<PaymentOrder, DatabaseError>;

    async fn find_order(&self, order_id: Uuid) -> Result<Option<PaymentOrder>, DatabaseError>;

    async fn list_orders(
        &self,
        user_id: Uuid,
        limit: i64,
    ) -> Result<Vec<PaymentOrder>, DatabaseError>;

    /// Cancel one of the user's own pending orders.
    async fn cancel_order(
        &self,
        user_id: Uuid,
        order_id: Uuid,
    ) -> Result<PaymentOrder, DatabaseError>;

    /// Expire pending orders and subscriptions past their time. Returns how
    /// many of each.
    async fn expire_stale(&self) -> Result<(u64, u64), DatabaseError>;

    /// Which of these provider transaction ids already have a recorded event.
    async fn seen_transaction_ids(&self, ids: &[i64]) -> Result<Vec<i64>, DatabaseError>;

    /// Creation time of the oldest unexpired pending order, if any.
    async fn oldest_pending_order_time(&self) -> Result<Option<DateTime<Utc>>, DatabaseError>;

    /// Apply one transfer: match it to an order by `codes`, and on a
    /// sufficient payment mark the order paid and create or extend the
    /// subscription, all in one transaction. Every call is recorded in
    /// `payment_events`. `source` is `webhook` or `reconcile`.
    async fn settle_transfer(
        &self,
        source: &str,
        transfer: &Transfer,
        codes: &[String],
    ) -> Result<SettleOutcome, DatabaseError>;

    async fn admin_list_orders(
        &self,
        status: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<PaymentOrder>, DatabaseError>;

    async fn admin_list_events(
        &self,
        outcome: Option<&str>,
        needs_review_only: bool,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<PaymentEvent>, DatabaseError>;
}

#[async_trait]
pub trait SupportRepository {
    async fn find_conversation(
        &self,
        id: Uuid,
    ) -> Result<Option<SupportConversationRow>, DatabaseError>;

    /// The user's open conversation, else their latest closed one.
    async fn find_user_conversation(
        &self,
        user_id: Uuid,
    ) -> Result<Option<SupportConversationRow>, DatabaseError>;

    /// Store a message from `user_id` in their conversation, creating it or
    /// reopening the closed one as needed, and count it unread for admins.
    /// A repeated `client_msg_id` returns the stored message with
    /// `duplicate: true` and writes nothing.
    async fn post_user_message(
        &self,
        user_id: Uuid,
        body: &str,
        preview: &str,
        client_msg_id: &str,
    ) -> Result<PostedMessage, DatabaseError>;

    /// Store an admin reply in an open conversation and count it unread for
    /// the user. `ValidationError` when the conversation is closed.
    async fn post_admin_message(
        &self,
        admin_id: Uuid,
        conversation_id: Uuid,
        body: &str,
        preview: &str,
        client_msg_id: &str,
    ) -> Result<PostedMessage, DatabaseError>;

    /// Mark everything the other side sent as read and zero the reader's
    /// counter. `None` when there was nothing to mark.
    async fn mark_read(
        &self,
        conversation_id: Uuid,
        reader_is_admin: bool,
    ) -> Result<Option<(SupportConversationRow, DateTime<Utc>)>, DatabaseError>;

    /// Open or close a conversation. The flag says whether anything changed.
    async fn set_status(
        &self,
        conversation_id: Uuid,
        open: bool,
    ) -> Result<(SupportConversationRow, bool), DatabaseError>;

    async fn assign(
        &self,
        conversation_id: Uuid,
        admin_id: Uuid,
    ) -> Result<SupportConversationRow, DatabaseError>;

    /// A page of messages, oldest first. `before` pages back from a message,
    /// `after` returns what came after one; without either, the newest
    /// `limit`. The flag says whether more exist in that direction.
    async fn list_messages(
        &self,
        conversation_id: Uuid,
        before: Option<Uuid>,
        after: Option<Uuid>,
        limit: i64,
    ) -> Result<(Vec<SupportMessage>, bool), DatabaseError>;

    /// Inbox: newest activity first. `search` is plain text matched against
    /// username and email; the first row's `total` is the match count.
    async fn list_conversations(
        &self,
        status: Option<&str>,
        search: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<SupportConversationRow>, DatabaseError>;

    async fn total_admin_unread(&self) -> Result<i64, DatabaseError>;
}
