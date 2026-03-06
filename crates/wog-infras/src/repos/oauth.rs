use async_trait::async_trait;
use chrono::Utc;
use sqlx::PgPool;
use wog_oauth::{GoogleUser, OAuthService, OAuthServiceError, OAuthTokens};

use crate::{
    errors::DatabaseError,
    models::{OAuthVerifierRow, User},
    repos::OAuthRepository,
};

pub struct OAuthConnection {
    pub user: GoogleUser,
    pub token: OAuthTokens,
}

pub struct PgOAuthRepo {
    pub pg_pool: PgPool,
    pub oauth_service: OAuthService,
}

impl PgOAuthRepo {
    pub fn new(pg_pool: PgPool, provider: &str) -> Self {
        let oauth_service = OAuthService::new(provider);
        Self {
            pg_pool,
            oauth_service,
        }
    }
}

#[async_trait]
impl OAuthRepository for PgOAuthRepo {
    async fn oauth_auth_url(&self) -> Result<String, DatabaseError> {
        let auth_url = self.oauth_service.authenticate_url();
        sqlx::query("INSERT INTO oauth_verifiers (csrf_token, pkce_verifier) VALUES ($1, $2)")
            .bind(auth_url.crsf_token)
            .bind(auth_url.pkce_verifier)
            .execute(&self.pg_pool)
            .await
            .map_err(|e| DatabaseError::Others(format!("Failed to store verifiers: {}", e)))?;

        Ok(auth_url.url)
    }
    async fn exchange_code(
        &self,
        code: String,
        csrf: String,
    ) -> Result<OAuthConnection, DatabaseError> {
        let row = sqlx::query_as::<_, OAuthVerifierRow>(
            "SELECT id, csrf_token, pkce_verifier, created_at FROM oauth_verifiers WHERE csrf_token = $1",
        )
        .bind(csrf)
        .fetch_optional(&self.pg_pool)
        .await
        .map_err(|e| DatabaseError::ValueNotFound(format!("DB error looking up verifier: {}", e)))?
        .ok_or_else(|| DatabaseError::ValidationError("Invalid CSRF token".into()))?;

        let age = Utc::now() - row.created_at;
        if age.num_minutes() > 10 {
            return Err(DatabaseError::Others("CSRF token expired".into()));
        }

        let pool = self.pg_pool.clone();
        tokio::spawn(async move {
            let _ = sqlx::query(
                "DELETE FROM oauth_verifiers WHERE created_at < NOW() - INTERVAL '10 minutes'",
            )
            .execute(&pool)
            .await;
        });

        let token = self
            .oauth_service
            .exchange_code(code, row.pkce_verifier)
            .await
            .map_err(|e| OAuthServiceError::ProviderApi(format!("Token exchange failed: {}", e)))?;

        let google_user = self
            .oauth_service
            .fetch_user_info(token.access_token.clone())
            .await?;

        Ok(OAuthConnection {
            user: google_user,
            token,
        })
    }
    async fn find_by_oauth(
        &self,
        provider: &str,
        sub: &str,
    ) -> Result<Option<User>, DatabaseError> {
        Ok(sqlx::query_as::<_, User>(
            "SELECT * FROM users WHERE oauth_provider = $1 AND oauth_sub = $2",
        )
        .bind(provider)
        .bind(sub)
        .fetch_optional(&self.pg_pool)
        .await?)
    }
    async fn create_oauth_user(
        &self,
        id: uuid::Uuid,
        email: &str,
        username: &str,
        avatar_url: &str,
        provider: &str,
        sub: &str,
    ) -> Result<User, DatabaseError> {
        sqlx::query_as::<_, User>(
            r#"
                INSERT INTO users (id, username, email, avatar_url, oauth_provider, oauth_sub)
                VALUES ($1, $2, $3, $4, $5, $6)
                RETURNING *
            "#,
        )
        .bind(id)
        .bind(username)
        .bind(email)
        .bind(avatar_url)
        .bind(provider)
        .bind(sub)
        .fetch_one(&self.pg_pool)
        .await
        .map_err(|e| match e {
            sqlx::Error::Database(ref db_err) if db_err.constraint().is_some() => {
                DatabaseError::ExistedDataError("Username or email already exists".into())
            }
            _ => DatabaseError::Others(e.to_string()),
        })
    }
}
