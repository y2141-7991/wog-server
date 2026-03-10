use crate::{
    errors::DatabaseError,
    models::{Event, EventNew},
    repos::{DynEventRepository, EventRepository},
};

#[derive(Clone)]
pub struct EventServices {
    user_repo: DynEventRepository,
}

impl EventServices {
    pub fn new(user_repo: std::sync::Arc<dyn EventRepository + Send + Sync>) -> Self {
        Self { user_repo }
    }
}

impl EventServices {
    pub async fn create_new_event(&self, event: EventNew) -> Result<Event, DatabaseError> {
        Ok(self.user_repo.create_event(event).await?)
    }
}
