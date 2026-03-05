use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;

use crate::{errors::DatabaseError, models::User};

pub mod users;

pub type DynUserRepository = Arc<dyn UserRepository + Send + Sync>;

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
