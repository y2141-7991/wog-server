use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::PgPool;

use crate::{
    errors::DatabaseError,
    models::{EmailVerificationToken, User},
    repos::UserRepository,
};

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

    async fn update_password(&self, user_id: uuid::Uuid, hash_pwd: &str) -> Result<User, DatabaseError> {
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
}
