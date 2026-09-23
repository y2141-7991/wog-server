use uuid::Uuid;

use crate::{
    errors::DatabaseError,
    models::{Event, EventRegistration, Wallet},
    repos::DynEventRegistrationRepository,
};

#[derive(Clone)]
pub struct EventRegistrationServices {
    event_registration_repo: DynEventRegistrationRepository,
}

impl EventRegistrationServices {
    pub fn new(event_registration_repo: DynEventRegistrationRepository) -> Self {
        Self {
            event_registration_repo,
        }
    }

    pub async fn join_event(
        &self,
        event_id: Uuid,
        user_id: Uuid,
    ) -> Result<(EventRegistration, Event, Option<Wallet>), DatabaseError> {
        self.event_registration_repo
            .join_event(event_id, user_id)
            .await
    }

    pub async fn leave_event(
        &self,
        event_id: Uuid,
        user_id: Uuid,
    ) -> Result<(EventRegistration, Event, Option<Wallet>), DatabaseError> {
        self.event_registration_repo
            .leave_event(event_id, user_id)
            .await
    }

    pub async fn get_my_registration(
        &self,
        event_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<EventRegistration>, DatabaseError> {
        self.event_registration_repo
            .find_registration(event_id, user_id)
            .await
    }
}
