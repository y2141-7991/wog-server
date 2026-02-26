use uuid::Uuid;

use crate::{errors::DatabaseError, models::User};

mod users;

pub trait UserRepository: Send + Sync + Clone + 'static {
    fn create(
        &self,
        id: Uuid,
        username: &str,
        email: &str,
        hash_pwd: &str,
    ) -> impl Future<Output = Result<User, DatabaseError>> + Send;

    fn find_by_id(&self, id: Uuid) -> impl Future<Output = Result<Option<User>, DatabaseError>> + Send;
}
