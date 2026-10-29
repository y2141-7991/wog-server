use uuid::Uuid;

use crate::{
    errors::DatabaseError,
    models::{
        AdminApiUsage, AdminTotals, AdminUserUsage, ApiKey, ApiUsageByApi, ApiUsageByDay,
        AuditLogEntry, User,
    },
    password::hash_password,
    repos::{DynAdminRepository, DynMarketplaceRepository},
};

/// Built-in admin account created on first start when no admin exists.
/// Dev convenience: sign in as `admin` / `admin`, then change the password.
pub const DEFAULT_ADMIN_USERNAME: &str = "admin";
pub const DEFAULT_ADMIN_EMAIL: &str = "admin@wog.local";
const DEFAULT_ADMIN_PASSWORD: &str = "admin";

const AUDIT_LOG_LIMIT: i64 = 100;

#[derive(Clone)]
pub struct AdminServices {
    repo: DynAdminRepository,
    marketplace: DynMarketplaceRepository,
}

fn clamp_days(days: i32) -> i32 {
    days.clamp(1, 90)
}

impl AdminServices {
    pub fn new(repo: DynAdminRepository, marketplace: DynMarketplaceRepository) -> Self {
        Self { repo, marketplace }
    }

    pub async fn overview(
        &self,
        days: i32,
    ) -> Result<(AdminTotals, Vec<AdminApiUsage>, Vec<ApiUsageByDay>), DatabaseError> {
        let days = clamp_days(days);
        Ok((
            self.repo.totals(days).await?,
            self.repo.api_breakdown(days).await?,
            self.repo.daily_requests(days).await?,
        ))
    }

    pub async fn users(
        &self,
        search: Option<&str>,
        days: i32,
        page: i64,
        per_page: i64,
    ) -> Result<Vec<AdminUserUsage>, DatabaseError> {
        let search = search.map(str::trim).filter(|q| !q.is_empty());
        self.repo
            .list_users_usage(search, clamp_days(days), per_page, page.max(0) * per_page)
            .await
    }

    /// One user's per-API usage, daily requests and keys, as they see them.
    pub async fn user_usage(
        &self,
        user_id: Uuid,
        days: i32,
    ) -> Result<(Vec<ApiUsageByApi>, Vec<ApiUsageByDay>, Vec<ApiKey>), DatabaseError> {
        let days = clamp_days(days);
        Ok((
            self.marketplace.usage_by_api(user_id, days).await?,
            self.marketplace.usage_by_day(user_id, days).await?,
            self.marketplace.list_keys(user_id).await?,
        ))
    }

    pub async fn set_role(
        &self,
        actor_id: Uuid,
        target_id: Uuid,
        role: &str,
    ) -> Result<User, DatabaseError> {
        if role != "user" && role != "admin" {
            return Err(DatabaseError::ValidationError(
                "Role must be 'user' or 'admin'".into(),
            ));
        }
        if actor_id == target_id {
            return Err(DatabaseError::ValidationError(
                "You cannot change your own role".into(),
            ));
        }
        self.repo.set_role(actor_id, target_id, role).await
    }

    pub async fn audit_log(&self) -> Result<Vec<AuditLogEntry>, DatabaseError> {
        self.repo.list_audit_log(AUDIT_LOG_LIMIT).await
    }

    /// Create the built-in `admin` account when no admin exists yet.
    /// `None` means an admin already exists and nothing was changed.
    pub async fn bootstrap_admin(&self) -> Result<Option<User>, DatabaseError> {
        self.repo
            .bootstrap_admin(
                DEFAULT_ADMIN_USERNAME,
                DEFAULT_ADMIN_EMAIL,
                &hash_password(DEFAULT_ADMIN_PASSWORD)?,
            )
            .await
    }
}
