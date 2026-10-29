use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;

use chrono::{DateTime, Utc};

use crate::{
    errors::DatabaseError,
    models::{
        AdminApiUsage, AdminTotals, AdminUserUsage, Api, ApiKey, ApiUsageByApi, ApiUsageByDay,
        AuditLogEntry, EmailVerificationToken, User,
    },
    repos::oauth::OAuthConnection,
};

pub mod admin;
pub mod marketplace;
pub mod oauth;
pub mod users;

pub type DynUserRepository = Arc<dyn UserRepository + Send + Sync>;
pub type DynOAuthRepository = Arc<dyn OAuthRepository + Send + Sync>;
pub type DynAdminRepository = Arc<dyn AdminRepository + Send + Sync>;
pub type DynMarketplaceRepository = Arc<dyn MarketplaceRepository + Send + Sync>;

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
    async fn find_active_key_by_hash(&self, key_hash: &str) -> Result<Option<ApiKey>, DatabaseError>;

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
    async fn count_usage_this_month(&self, user_id: Uuid, api_id: Uuid) -> Result<i64, DatabaseError>;

    async fn usage_by_api(&self, user_id: Uuid, days: i32) -> Result<Vec<ApiUsageByApi>, DatabaseError>;

    async fn usage_by_day(&self, user_id: Uuid, days: i32) -> Result<Vec<ApiUsageByDay>, DatabaseError>;
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
