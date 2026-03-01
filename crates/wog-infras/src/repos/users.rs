use async_trait::async_trait;
use sqlx::PgPool;

use crate::{errors::DatabaseError, models::User, repos::UserRepository};

#[derive(Clone)]
struct PgUserRepo {
    pg_pool: PgPool,
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
}
