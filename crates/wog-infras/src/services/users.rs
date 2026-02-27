use uuid::Uuid;

use crate::{
    errors::DatabaseError,
    models::User,
    repos::{DynUserRepository, UserRepository},
};

struct UserServices {
    user_repo: DynUserRepository,
}

impl UserServices {
    async fn get_user(&self, id: Uuid) -> Result<User, DatabaseError> {
        self.user_repo
            .find_by_id(id)
            .await?
            .ok_or_else(|| DatabaseError::NotFound("User not found".into()))
    }
}
