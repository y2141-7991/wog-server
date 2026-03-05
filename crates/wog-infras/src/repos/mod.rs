use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;

use crate::{errors::DatabaseError, models::User, repos::oauth::OAuthConnection};

pub mod oauth;
pub mod users;

pub type DynUserRepository = Arc<dyn UserRepository + Send + Sync>;
pub type DynOAuthRepository = Arc<dyn OAuthRepository + Send + Sync>;

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
