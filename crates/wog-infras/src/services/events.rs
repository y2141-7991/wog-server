use uuid::Uuid;

use crate::{
    errors::DatabaseError,
    models::{Event, EventNew, EventUpdate, PaginateVec, PaginationRequest},
    repos::{DynEventRepository, EventRepository},
};

#[derive(Clone)]
pub struct EventServices {
    event_repo: DynEventRepository,
}

impl EventServices {
    pub fn new(event_repo: std::sync::Arc<dyn EventRepository + Send + Sync>) -> Self {
        Self { event_repo }
    }
}

impl EventServices {
    pub async fn create_new_event(&self, event: EventNew) -> Result<Event, DatabaseError> {
        Ok(self.event_repo.create_event(event).await?)
    }
    pub async fn find_events_by_current_id(&self, id: Uuid) -> Result<Vec<Event>, DatabaseError> {
        Ok(self.event_repo.find_list_events_by_current_id(id).await?)
    }
    pub async fn update_event(
        &self,
        event: EventUpdate,
        event_id: Uuid,
    ) -> Result<Event, DatabaseError> {
        Ok(self.event_repo.update_event(event, event_id).await?)
    }
    pub async fn find_events(&self, pagination: PaginationRequest) -> Result<PaginateVec<Event>, DatabaseError> {
        Ok(self.event_repo.find_list_events(pagination).await?)
    }
}
