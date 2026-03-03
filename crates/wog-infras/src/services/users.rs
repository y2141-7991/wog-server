use uuid::Uuid;

use crate::{
    errors::DatabaseError,
    models::User,
    repos::{DynUserRepository, UserRepository},
};

#[derive(Clone)]
pub struct UserServices {
    user_repo: DynUserRepository,
}

impl UserServices {
    pub fn new(user_repo: std::sync::Arc<dyn UserRepository + Send + Sync>) -> Self {
        Self { user_repo }
    }
}

impl UserServices {
    pub async fn get_user(&self, id: Uuid) -> Result<User, DatabaseError> {
        self.user_repo
            .find_by_id(id)
            .await?
            .ok_or_else(|| DatabaseError::ValueNotFound("User not found".into()))
    }
}
