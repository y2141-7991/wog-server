use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::PgPool;

use crate::{
    errors::DatabaseError,
    models::{EmailVerificationToken, RefreshOutcome, SessionInfo, User},
    repos::UserRepository,
};
use uuid::Uuid;

#[derive(Clone)]
pub struct PgUserRepo {
    pub pg_pool: PgPool,
}

impl PgUserRepo {
    pub fn new(pg_pool: PgPool) -> Self {
        Self { pg_pool }
    }
}

#[async_trait]
impl UserRepository for PgUserRepo {
    async fn create(
        &self,
        id: uuid::Uuid,
        username: &str,
        email: &str,
        hash_pwd: &str,
    ) -> Result<crate::models::User, crate::errors::DatabaseError> {
        sqlx::query_as::<_, User>(
            r#"
                INSERT INTO users (id, username, email, password)
                VALUES ($1, $2, $3, $4)
                RETURNING *
            "#,
        )
        .bind(id)
        .bind(username)
        .bind(email)
        .bind(hash_pwd)
        .fetch_one(&self.pg_pool)
        .await
        .map_err(|e| match e {
            sqlx::Error::Database(ref db_err) if db_err.constraint().is_some() => {
                DatabaseError::ExistedDataError("Username or email already exists".into())
            }
            _ => DatabaseError::Others(e.to_string()),
        })
    }
    async fn find_by_id(&self, id: uuid::Uuid) -> Result<Option<User>, DatabaseError> {
        Ok(sqlx::query_as::<_, User>(
            r#"
                SELECT * FROM users WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pg_pool)
        .await?)
    }

    async fn find_by_email(&self, email: &str) -> Result<Option<User>, DatabaseError> {
        Ok(sqlx::query_as::<_, User>(
            r#"
                SELECT * FROM users WHERE email = $1
            "#,
        )
        .bind(email)
        .fetch_optional(&self.pg_pool)
        .await?)
    }

    async fn find_by_username(&self, username: &str) -> Result<Option<User>, DatabaseError> {
        Ok(sqlx::query_as::<_, User>(
            r#"
                SELECT * FROM users WHERE username = $1
            "#,
        )
        .bind(username)
        .fetch_optional(&self.pg_pool)
        .await?)
    }

    async fn create_verification_token(
        &self,
        user_id: uuid::Uuid,
        token: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<(), DatabaseError> {
        sqlx::query(
            r#"
                INSERT INTO email_verification_tokens (user_id, token, expires_at)
                VALUES ($1, $2, $3)
            "#,
        )
        .bind(user_id)
        .bind(token)
        .bind(expires_at)
        .execute(&self.pg_pool)
        .await?;
        Ok(())
    }

    async fn find_verification_token(
        &self,
        token: &str,
    ) -> Result<Option<EmailVerificationToken>, DatabaseError> {
        Ok(sqlx::query_as::<_, EmailVerificationToken>(
            r#"
                SELECT * FROM email_verification_tokens WHERE token = $1
            "#,
        )
        .bind(token)
        .fetch_optional(&self.pg_pool)
        .await?)
    }

    async fn delete_verification_token(&self, token: &str) -> Result<(), DatabaseError> {
        sqlx::query("DELETE FROM email_verification_tokens WHERE token = $1")
            .bind(token)
            .execute(&self.pg_pool)
            .await?;
        Ok(())
    }

    async fn mark_email_verified(&self, user_id: uuid::Uuid) -> Result<User, DatabaseError> {
        Ok(sqlx::query_as::<_, User>(
            r#"
                UPDATE users
                SET email_verified = TRUE, updated_at = NOW()
                WHERE id = $1
                RETURNING *
            "#,
        )
        .bind(user_id)
        .fetch_one(&self.pg_pool)
        .await?)
    }

    async fn update_password(
        &self,
        user_id: uuid::Uuid,
        hash_pwd: &str,
    ) -> Result<User, DatabaseError> {
        Ok(sqlx::query_as::<_, User>(
            r#"
                UPDATE users
                SET password = $2, must_change_password = FALSE, updated_at = NOW()
                WHERE id = $1
                RETURNING *
            "#,
        )
        .bind(user_id)
        .bind(hash_pwd)
        .fetch_one(&self.pg_pool)
        .await?)
    }
    async fn create_reset_token(
        &self,
        user_id: uuid::Uuid,
        token_hash: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<(), DatabaseError> {
        sqlx::query(
            r#"
                INSERT INTO password_reset_tokens (user_id, token_hash, expires_at)
                VALUES ($1, $2, $3)
            "#,
        )
        .bind(user_id)
        .bind(token_hash)
        .bind(expires_at)
        .execute(&self.pg_pool)
        .await?;
        Ok(())
    }

    async fn count_reset_tokens_since(
        &self,
        user_id: uuid::Uuid,
        since: DateTime<Utc>,
    ) -> Result<i64, DatabaseError> {
        Ok(sqlx::query_scalar::<_, i64>(
            r#"
                SELECT COUNT(*) FROM password_reset_tokens
                WHERE user_id = $1 AND created_at >= $2
            "#,
        )
        .bind(user_id)
        .bind(since)
        .fetch_one(&self.pg_pool)
        .await?)
    }

    async fn reset_password_with_token(
        &self,
        token_hash: &str,
        new_hash_pwd: &str,
    ) -> Result<Option<User>, DatabaseError> {
        let mut tx = self.pg_pool.begin().await?;

        // Spending the token and checking it are one statement, so two
        // concurrent requests with the same link cannot both succeed.
        let spent: Option<uuid::Uuid> = sqlx::query_scalar(
            r#"
                UPDATE password_reset_tokens
                SET used_at = NOW()
                WHERE token_hash = $1 AND used_at IS NULL AND expires_at > NOW()
                RETURNING user_id
            "#,
        )
        .bind(token_hash)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(user_id) = spent else {
            return Ok(None);
        };

        let user = sqlx::query_as::<_, User>(
            r#"
                UPDATE users
                SET password = $2,
                    must_change_password = FALSE,
                    password_changed_at = NOW(),
                    updated_at = NOW()
                WHERE id = $1
                RETURNING *
            "#,
        )
        .bind(user_id)
        .bind(new_hash_pwd)
        .fetch_one(&mut *tx)
        .await?;

        // Any other link still in the inbox is now stale.
        sqlx::query("DELETE FROM password_reset_tokens WHERE user_id = $1 AND used_at IS NULL")
            .bind(user_id)
            .execute(&mut *tx)
            .await?;

        // Signed out everywhere: no refresh token of this account works any more.
        sqlx::query(
            "UPDATE refresh_sessions SET revoked_at = NOW() WHERE user_id = $1 AND revoked_at IS NULL",
        )
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(Some(user))
    }
    async fn create_session(
        &self,
        id: Uuid,
        user_id: Uuid,
        jti: Uuid,
        expires_at: DateTime<Utc>,
    ) -> Result<(), DatabaseError> {
        sqlx::query(
            "INSERT INTO refresh_sessions (id, user_id, current_jti, expires_at) VALUES ($1, $2, $3, $4)",
        )
        .bind(id)
        .bind(user_id)
        .bind(jti)
        .bind(expires_at)
        .execute(&self.pg_pool)
        .await?;
        Ok(())
    }

    async fn rotate_session(
        &self,
        id: Uuid,
        jti: Uuid,
        new_jti: Uuid,
        new_expires_at: DateTime<Utc>,
        grace_seconds: i64,
    ) -> Result<RefreshOutcome, DatabaseError> {
        let mut tx = self.pg_pool.begin().await?;

        // The row lock serialises concurrent refreshes of one session: the second one
        // waits, then sees the token the first one already spent.
        let row: Option<(Uuid, Uuid, Option<Uuid>, bool, bool)> = sqlx::query_as(
            r#"
                SELECT user_id,
                       current_jti,
                       previous_jti,
                       (revoked_at IS NOT NULL OR expires_at <= NOW()) AS dead,
                       COALESCE(rotated_at > NOW() - make_interval(secs => $2::float8), FALSE) AS fresh
                FROM refresh_sessions
                WHERE id = $1
                FOR UPDATE
            "#,
        )
        .bind(id)
        .bind(grace_seconds as f64)
        .fetch_optional(&mut *tx)
        .await?;

        let Some((user_id, current, previous, dead, within_grace)) = row else {
            return Ok(RefreshOutcome::Invalid);
        };
        if dead {
            return Ok(RefreshOutcome::Invalid);
        }

        let outcome = if current == jti {
            sqlx::query(
                r#"
                    UPDATE refresh_sessions
                    SET previous_jti = current_jti,
                        current_jti = $2,
                        rotated_at = NOW(),
                        last_used_at = NOW(),
                        expires_at = $3
                    WHERE id = $1
                "#,
            )
            .bind(id)
            .bind(new_jti)
            .bind(new_expires_at)
            .execute(&mut *tx)
            .await?;
            RefreshOutcome::Rotated(user_id)
        } else if previous == Some(jti) && within_grace {
            RefreshOutcome::Replayed(user_id)
        } else {
            // A token that was already spent, outside the race window (or one this
            // session never issued): treat the whole sign-in as compromised.
            sqlx::query("UPDATE refresh_sessions SET revoked_at = NOW() WHERE id = $1")
                .bind(id)
                .execute(&mut *tx)
                .await?;
            RefreshOutcome::Reused
        };

        tx.commit().await?;
        Ok(outcome)
    }

    async fn revoke_session(&self, id: Uuid) -> Result<(), DatabaseError> {
        sqlx::query(
            "UPDATE refresh_sessions SET revoked_at = NOW() WHERE id = $1 AND revoked_at IS NULL",
        )
        .bind(id)
        .execute(&self.pg_pool)
        .await?;
        Ok(())
    }

    async fn find_live_session(&self, id: Uuid) -> Result<Option<SessionInfo>, DatabaseError> {
        Ok(sqlx::query_as::<_, SessionInfo>(
            r#"
                SELECT id, user_id, created_at, expires_at
                FROM refresh_sessions
                WHERE id = $1 AND revoked_at IS NULL AND expires_at > NOW()
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pg_pool)
        .await?)
    }

    async fn create_auth_code(
        &self,
        code_hash: &str,
        kind: &str,
        user_id: Uuid,
        session_id: Option<Uuid>,
        expires_at: DateTime<Utc>,
    ) -> Result<(), DatabaseError> {
        // Codes live for seconds; sweep the dead ones whenever a new one is made.
        sqlx::query("DELETE FROM auth_codes WHERE expires_at < NOW()")
            .execute(&self.pg_pool)
            .await?;
        sqlx::query(
            "INSERT INTO auth_codes (code_hash, kind, user_id, session_id, expires_at) VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(code_hash)
        .bind(kind)
        .bind(user_id)
        .bind(session_id)
        .bind(expires_at)
        .execute(&self.pg_pool)
        .await?;
        Ok(())
    }

    async fn redeem_auth_code(
        &self,
        code_hash: &str,
        kind: &str,
    ) -> Result<Option<(Uuid, Option<Uuid>)>, DatabaseError> {
        // Deleting is spending: one statement, so two requests cannot both get it.
        Ok(sqlx::query_as(
            r#"
                DELETE FROM auth_codes
                WHERE code_hash = $1 AND kind = $2 AND expires_at > NOW()
                RETURNING user_id, session_id
            "#,
        )
        .bind(code_hash)
        .bind(kind)
        .fetch_optional(&self.pg_pool)
        .await?)
    }
}
