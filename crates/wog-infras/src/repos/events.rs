use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    errors::DatabaseError,
    models::{Event, EventNew, EventUpdate},
    repos::EventRepository,
};

pub struct PgEventRepo {
    pg_pool: PgPool,
}
impl PgEventRepo {
    pub fn new(pg_pool: PgPool) -> Self {
        Self { pg_pool }
    }
}

#[async_trait]
impl EventRepository for PgEventRepo {
    async fn create_event(&self, event: EventNew) -> Result<Event, DatabaseError> {
        let event = sqlx::query_as::<_, Event>(
            r#"
                INSERT INTO events (title, organizer_id, price, capacity, registered_count, status, start_time, end_time)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                RETURNING *
            "#,
        )
        .bind(event.title)
        .bind(event.organizer_id)
        .bind(event.price)
        .bind(event.capacity)
        .bind(event.registered_count)
        .bind(event.status)
        .bind(event.start_time)
        .bind(event.end_time)
        .fetch_one(&self.pg_pool)
        .await?;
        Ok(event)
    }
    async fn update_event(
        &self,
        event: EventUpdate,
        event_id: Uuid,
    ) -> Result<Event, DatabaseError> {
        Ok(sqlx::query_as::<_, Event>(
        r#"
            UPDATE events SET 
                title = $1,
                description = $2,
                price = $3,
                capacity = $4,
                status = $5,
                start_time = $6,
                end_time = $7
            WHERE id = $8
            RETURNING *
        "#,
        )
        .bind(event.title)
        .bind(event.description)
        .bind(event.price)
        .bind(event.capacity)
        .bind(event.status)
        .bind(event.start_time)
        .bind(event.end_time)
        .bind(event_id)
        .fetch_one(&self.pg_pool)
        .await?)
    }
    async fn delete_event(&self, id: Uuid) {}
    async fn find_event_by_id(&self, id: Uuid) -> Result<Event, DatabaseError> {
        Ok(sqlx::query_as::<_, Event>(
            r#"
                SELECT * FROM events WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_one(&self.pg_pool)
        .await?)
    }
    async fn find_list_events_by_current_id(&self, id: Uuid) -> Result<Vec<Event>, DatabaseError> {
        let events = sqlx::query_as::<_, Event>(
            r#"
                SELECT * FROM events ORDER BY created_at DESC
            "#,
        )
        .bind(id)
        .fetch_all(&self.pg_pool)
        .await?;
        Ok(events)
    }
}
