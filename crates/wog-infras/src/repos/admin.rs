use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    errors::DatabaseError,
    models::{AdminApiUsage, AdminTotals, AdminUserUsage, ApiUsageByDay, AuditLogEntry, User},
    repos::AdminRepository,
};

/// Serializes role changes and the bootstrap, so the "last admin" check and
/// the write that follows it cannot interleave with another admin change.
const ADMIN_LOCK: i64 = 7001;

#[derive(Clone)]
pub struct PgAdminRepo {
    pub pg_pool: PgPool,
}

impl PgAdminRepo {
    pub fn new(pg_pool: PgPool) -> Self {
        Self { pg_pool }
    }
}

#[async_trait]
impl AdminRepository for PgAdminRepo {
    async fn totals(&self, days: i32) -> Result<AdminTotals, DatabaseError> {
        Ok(sqlx::query_as::<_, AdminTotals>(
            r#"
                SELECT (SELECT COUNT(*) FROM users) AS total_users,
                       COUNT(DISTINCT user_id) AS active_users,
                       COUNT(*) AS requests,
                       COUNT(*) FILTER (WHERE status_code >= 400) AS errors,
                       AVG(latency_ms)::int AS avg_latency_ms
                FROM api_usage
                WHERE created_at >= NOW() - make_interval(days => $1)
            "#,
        )
        .bind(days)
        .fetch_one(&self.pg_pool)
        .await?)
    }

    async fn api_breakdown(&self, days: i32) -> Result<Vec<AdminApiUsage>, DatabaseError> {
        Ok(sqlx::query_as::<_, AdminApiUsage>(
            r#"
                SELECT a.slug, a.name,
                       COUNT(u.id) AS requests,
                       COUNT(u.id) FILTER (WHERE u.status_code >= 400) AS errors,
                       AVG(u.latency_ms)::int AS avg_latency_ms,
                       COUNT(DISTINCT u.user_id) AS users
                FROM apis a
                LEFT JOIN api_usage u
                       ON u.api_id = a.id
                      AND u.created_at >= NOW() - make_interval(days => $1)
                WHERE a.enabled
                GROUP BY a.id
                ORDER BY requests DESC, a.name
            "#,
        )
        .bind(days)
        .fetch_all(&self.pg_pool)
        .await?)
    }

    async fn daily_requests(&self, days: i32) -> Result<Vec<ApiUsageByDay>, DatabaseError> {
        Ok(sqlx::query_as::<_, ApiUsageByDay>(
            r#"
                SELECT d::date AS day, COALESCE(c.requests, 0) AS requests
                FROM generate_series(
                         ((NOW() AT TIME ZONE 'UTC')::date - ($1 - 1))::timestamp,
                         (NOW() AT TIME ZONE 'UTC')::date::timestamp,
                         interval '1 day') AS d
                LEFT JOIN (
                    SELECT (created_at AT TIME ZONE 'UTC')::date AS day, COUNT(*) AS requests
                    FROM api_usage
                    WHERE created_at >= ((NOW() AT TIME ZONE 'UTC')::date - ($1 - 1))::timestamp
                                        AT TIME ZONE 'UTC'
                    GROUP BY 1
                ) c ON c.day = d::date
                ORDER BY d
            "#,
        )
        .bind(days)
        .fetch_all(&self.pg_pool)
        .await?)
    }

    async fn list_users_usage(
        &self,
        search: Option<&str>,
        days: i32,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<AdminUserUsage>, DatabaseError> {
        Ok(sqlx::query_as::<_, AdminUserUsage>(
            r#"
                SELECT u.id, u.username, u.email, u.role, u.created_at,
                       COALESCE(s.requests, 0) AS requests,
                       COALESCE(s.errors, 0) AS errors,
                       (SELECT MAX(created_at) FROM api_usage WHERE user_id = u.id) AS last_active,
                       (SELECT COUNT(*) FROM api_keys k
                         WHERE k.user_id = u.id AND k.revoked_at IS NULL) AS active_keys,
                       COUNT(*) OVER () AS total
                FROM users u
                LEFT JOIN (
                    SELECT user_id,
                           COUNT(*) AS requests,
                           COUNT(*) FILTER (WHERE status_code >= 400) AS errors
                    FROM api_usage
                    WHERE created_at >= NOW() - make_interval(days => $2)
                    GROUP BY user_id
                ) s ON s.user_id = u.id
                WHERE $1::text IS NULL
                   OR u.username ILIKE $1 ESCAPE '\'
                   OR u.email ILIKE $1 ESCAPE '\'
                ORDER BY requests DESC, u.created_at DESC
                LIMIT $3 OFFSET $4
            "#,
        )
        .bind(search.map(|q| {
            let escaped = q.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
            format!("%{escaped}%")
        }))
        .bind(days)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pg_pool)
        .await?)
    }

    async fn set_role(
        &self,
        actor_id: Uuid,
        target_id: Uuid,
        role: &str,
    ) -> Result<User, DatabaseError> {
        let mut tx = self.pg_pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(ADMIN_LOCK)
            .execute(&mut *tx)
            .await?;

        let current: String = sqlx::query_scalar("SELECT role FROM users WHERE id = $1 FOR UPDATE")
            .bind(target_id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| DatabaseError::ValueNotFound("User not found".into()))?;

        if current != role && current == "admin" {
            let admins: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE role = 'admin'")
                .fetch_one(&mut *tx)
                .await?;
            if admins <= 1 {
                return Err(DatabaseError::ValidationError(
                    "You cannot demote the last admin".into(),
                ));
            }
        }

        let user = sqlx::query_as::<_, User>(
            "UPDATE users SET role = $2, updated_at = NOW() WHERE id = $1 RETURNING *",
        )
        .bind(target_id)
        .bind(role)
        .fetch_one(&mut *tx)
        .await?;

        if current != role {
            sqlx::query(
                r#"
                    INSERT INTO admin_audit_log (actor_id, target_user_id, action, old_value, new_value)
                    VALUES ($1, $2, 'role_change', $3, $4)
                "#,
            )
            .bind(actor_id)
            .bind(target_id)
            .bind(&current)
            .bind(role)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(user)
    }

    async fn list_audit_log(&self, limit: i64) -> Result<Vec<AuditLogEntry>, DatabaseError> {
        Ok(sqlx::query_as::<_, AuditLogEntry>(
            r#"
                SELECT l.id, actor.username AS actor_username, l.target_user_id,
                       target.username AS target_username,
                       l.action, l.old_value, l.new_value, l.created_at
                FROM admin_audit_log l
                LEFT JOIN users actor  ON actor.id  = l.actor_id
                LEFT JOIN users target ON target.id = l.target_user_id
                ORDER BY l.created_at DESC
                LIMIT $1
            "#,
        )
        .bind(limit)
        .fetch_all(&self.pg_pool)
        .await?)
    }

    async fn bootstrap_admin(
        &self,
        username: &str,
        email: &str,
        hash_pwd: &str,
    ) -> Result<Option<User>, DatabaseError> {
        let mut tx = self.pg_pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(ADMIN_LOCK)
            .execute(&mut *tx)
            .await?;

        let has_admin: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM users WHERE role = 'admin')")
                .fetch_one(&mut *tx)
                .await?;
        if has_admin {
            return Ok(None);
        }

        let taken: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM users WHERE username = $1 OR email = $2)",
        )
        .bind(username)
        .bind(email)
        .fetch_one(&mut *tx)
        .await?;
        if taken {
            return Err(DatabaseError::ValidationError(format!(
                "cannot create the default admin: username '{username}' or email '{email}' is already used by a regular account"
            )));
        }

        let admin = sqlx::query_as::<_, User>(
            r#"
                INSERT INTO users (id, username, email, password, email_verified, role)
                VALUES ($1, $2, $3, $4, TRUE, 'admin')
                RETURNING *
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(username)
        .bind(email)
        .bind(hash_pwd)
        .fetch_one(&mut *tx)
        .await?;

        sqlx::query(
            r#"
                INSERT INTO admin_audit_log (actor_id, target_user_id, action, old_value, new_value)
                VALUES (NULL, $1, 'bootstrap_create', NULL, 'admin')
            "#,
        )
        .bind(admin.id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(Some(admin))
    }
}
