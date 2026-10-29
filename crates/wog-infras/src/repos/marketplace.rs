use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    errors::DatabaseError,
    models::{Api, ApiKey, ApiUsageByApi, ApiUsageByDay},
    repos::MarketplaceRepository,
};

/// Catalog row plus live metrics from the last 30 days of metered calls.
const CATALOG_SELECT: &str = r#"
    SELECT a.*,
        (SELECT AVG(latency_ms)::int FROM api_usage u
            WHERE u.api_id = a.id AND u.created_at > NOW() - INTERVAL '30 days') AS avg_latency_ms,
        (SELECT (100.0 * COUNT(*) FILTER (WHERE status_code < 500) / NULLIF(COUNT(*), 0))::float8
            FROM api_usage u
            WHERE u.api_id = a.id AND u.created_at > NOW() - INTERVAL '30 days') AS success_rate,
        (SELECT COUNT(*) FROM api_usage u WHERE u.api_id = a.id) AS total_calls
    FROM apis a
"#;

#[derive(Clone)]
pub struct PgMarketplaceRepo {
    pub pg_pool: PgPool,
}

impl PgMarketplaceRepo {
    pub fn new(pg_pool: PgPool) -> Self {
        Self { pg_pool }
    }
}

#[async_trait]
impl MarketplaceRepository for PgMarketplaceRepo {
    async fn list_apis(&self) -> Result<Vec<Api>, DatabaseError> {
        Ok(
            sqlx::query_as::<_, Api>(&format!("{CATALOG_SELECT} WHERE a.enabled ORDER BY a.name"))
                .fetch_all(&self.pg_pool)
                .await?,
        )
    }

    async fn find_api_by_slug(&self, slug: &str) -> Result<Option<Api>, DatabaseError> {
        Ok(
            sqlx::query_as::<_, Api>(&format!("{CATALOG_SELECT} WHERE a.slug = $1 AND a.enabled"))
                .bind(slug)
                .fetch_optional(&self.pg_pool)
                .await?,
        )
    }

    async fn find_api_by_gateway_path(&self, path: &str) -> Result<Option<Api>, DatabaseError> {
        Ok(
            sqlx::query_as::<_, Api>("SELECT * FROM apis WHERE gateway_path = $1 AND enabled")
                .bind(path)
                .fetch_optional(&self.pg_pool)
                .await?,
        )
    }

    async fn create_key(
        &self,
        user_id: Uuid,
        name: &str,
        key_prefix: &str,
        key_hash: &str,
    ) -> Result<ApiKey, DatabaseError> {
        Ok(sqlx::query_as::<_, ApiKey>(
            r#"
                INSERT INTO api_keys (user_id, name, key_prefix, key_hash)
                VALUES ($1, $2, $3, $4)
                RETURNING *
            "#,
        )
        .bind(user_id)
        .bind(name)
        .bind(key_prefix)
        .bind(key_hash)
        .fetch_one(&self.pg_pool)
        .await?)
    }

    async fn list_keys(&self, user_id: Uuid) -> Result<Vec<ApiKey>, DatabaseError> {
        Ok(sqlx::query_as::<_, ApiKey>(
            "SELECT * FROM api_keys WHERE user_id = $1 ORDER BY created_at DESC",
        )
        .bind(user_id)
        .fetch_all(&self.pg_pool)
        .await?)
    }

    async fn revoke_key(&self, user_id: Uuid, key_id: Uuid) -> Result<(), DatabaseError> {
        let revoked = sqlx::query(
            r#"
                UPDATE api_keys SET revoked_at = NOW()
                WHERE id = $1 AND user_id = $2 AND revoked_at IS NULL
            "#,
        )
        .bind(key_id)
        .bind(user_id)
        .execute(&self.pg_pool)
        .await?
        .rows_affected();

        if revoked == 0 {
            return Err(DatabaseError::ValueNotFound("API key not found".into()));
        }
        Ok(())
    }

    async fn find_active_key_by_hash(&self, key_hash: &str) -> Result<Option<ApiKey>, DatabaseError> {
        Ok(sqlx::query_as::<_, ApiKey>(
            "SELECT * FROM api_keys WHERE key_hash = $1 AND revoked_at IS NULL",
        )
        .bind(key_hash)
        .fetch_optional(&self.pg_pool)
        .await?)
    }

    async fn record_usage(
        &self,
        key: &ApiKey,
        api_id: Uuid,
        status_code: i16,
        latency_ms: i32,
    ) -> Result<(), DatabaseError> {
        let mut tx = self.pg_pool.begin().await?;
        sqlx::query(
            r#"
                INSERT INTO api_usage (api_key_id, user_id, api_id, status_code, latency_ms)
                VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(key.id)
        .bind(key.user_id)
        .bind(api_id)
        .bind(status_code)
        .bind(latency_ms)
        .execute(&mut *tx)
        .await?;
        sqlx::query("UPDATE api_keys SET last_used_at = NOW() WHERE id = $1")
            .bind(key.id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    async fn count_usage_this_month(&self, user_id: Uuid, api_id: Uuid) -> Result<i64, DatabaseError> {
        let (count,): (i64,) = sqlx::query_as(
            r#"
                SELECT COUNT(*) FROM api_usage
                WHERE user_id = $1 AND api_id = $2 AND status_code < 500
                  AND created_at >= date_trunc('month', NOW() AT TIME ZONE 'UTC') AT TIME ZONE 'UTC'
            "#,
        )
        .bind(user_id)
        .bind(api_id)
        .fetch_one(&self.pg_pool)
        .await?;
        Ok(count)
    }

    async fn usage_by_api(&self, user_id: Uuid, days: i32) -> Result<Vec<ApiUsageByApi>, DatabaseError> {
        Ok(sqlx::query_as::<_, ApiUsageByApi>(
            r#"
                SELECT a.slug, a.name, a.monthly_quota,
                       COUNT(u.id) FILTER (WHERE u.created_at >= NOW() - make_interval(days => $2)) AS requests,
                       COUNT(u.id) FILTER (WHERE u.created_at >= NOW() - make_interval(days => $2)
                                             AND u.status_code >= 400) AS errors,
                       COUNT(u.id) FILTER (WHERE u.created_at >=
                           date_trunc('month', NOW() AT TIME ZONE 'UTC') AT TIME ZONE 'UTC') AS used_this_month
                FROM apis a
                LEFT JOIN api_usage u ON u.api_id = a.id AND u.user_id = $1
                WHERE a.enabled
                GROUP BY a.id
                ORDER BY a.name
            "#,
        )
        .bind(user_id)
        .bind(days)
        .fetch_all(&self.pg_pool)
        .await?)
    }

    async fn usage_by_day(&self, user_id: Uuid, days: i32) -> Result<Vec<ApiUsageByDay>, DatabaseError> {
        Ok(sqlx::query_as::<_, ApiUsageByDay>(
            r#"
                SELECT d::date AS day, COUNT(u.id) AS requests
                FROM generate_series(
                         (NOW() AT TIME ZONE 'UTC')::date - ($2 - 1),
                         (NOW() AT TIME ZONE 'UTC')::date,
                         interval '1 day') AS d
                LEFT JOIN api_usage u
                       ON u.user_id = $1
                      AND (u.created_at AT TIME ZONE 'UTC')::date = d::date
                GROUP BY d
                ORDER BY d
            "#,
        )
        .bind(user_id)
        .bind(days)
        .fetch_all(&self.pg_pool)
        .await?)
    }
}
