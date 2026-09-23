use std::sync::Arc;

use async_trait::async_trait;
use sqlx::QueryBuilder;
use uuid::Uuid;

use chrono::{DateTime, Utc};

use crate::{
    errors::DatabaseError,
    models::{
        EmailVerificationToken, Event, EventFilter, EventNew, EventRegistration, EventUpdate,
        PaginateVec, PaginationRequest, User, Wallet,
    },
    repos::oauth::OAuthConnection,
};

pub mod event_registration;
pub mod events;
pub mod oauth;
pub mod users;
pub mod wallet;

pub type DynUserRepository = Arc<dyn UserRepository + Send + Sync>;
pub type DynOAuthRepository = Arc<dyn OAuthRepository + Send + Sync>;
pub type DynEventRepository = Arc<dyn EventRepository + Send + Sync>;
pub type DynWalletRepository = Arc<dyn WalletRepository + Send + Sync>;
pub type DynEventRegistrationRepository = Arc<dyn EventRegistrationRepository + Send + Sync>;

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

    async fn find_by_email(&self, email: &str) -> Result<Option<User>, DatabaseError>;

    async fn find_by_username(&self, username: &str) -> Result<Option<User>, DatabaseError>;

    async fn create_verification_token(
        &self,
        user_id: Uuid,
        token: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<(), DatabaseError>;

    async fn find_verification_token(
        &self,
        token: &str,
    ) -> Result<Option<EmailVerificationToken>, DatabaseError>;

    async fn delete_verification_token(&self, token: &str) -> Result<(), DatabaseError>;

    async fn mark_email_verified(&self, user_id: Uuid) -> Result<User, DatabaseError>;
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

#[async_trait]
pub trait WalletRepository {
    async fn get_or_create_wallet(&self, user_id: Uuid) -> Result<Wallet, DatabaseError>;

    /// Move `amount` credits directly from `sender_id`'s wallet to
    /// `recipient_id`'s wallet -- a purely in-app ledger change, with no
    /// on-chain activity involved. Fails with `DatabaseError::InsufficientBalance`
    /// if the sender doesn't have enough. `sender_description` and
    /// `recipient_description` are recorded on each side's own
    /// `wallet_transactions` row.
    async fn transfer_credits(
        &self,
        sender_id: Uuid,
        recipient_id: Uuid,
        amount: rust_decimal::Decimal,
        sender_description: &str,
        recipient_description: &str,
    ) -> Result<(Wallet, Wallet), DatabaseError>;
}

#[async_trait]
pub trait EventRepository {
    async fn create_event(&self, event: EventNew) -> Result<Event, DatabaseError>;

    async fn update_event(
        &self,
        event: EventUpdate,
        event_id: Uuid,
    ) -> Result<Event, DatabaseError>;

    async fn delete_event(&self, id: Uuid);

    async fn find_event_by_id(&self, id: Uuid) -> Result<Event, DatabaseError>;

    async fn find_list_events_by_current_id(&self, id: Uuid) -> Result<Vec<Event>, DatabaseError>;

    async fn find_list_events(
        &self,
        pagination: PaginationRequest,
        filter: EventFilter,
    ) -> Result<PaginateVec<Event>, DatabaseError>;
}

#[async_trait]
pub trait EventRegistrationRepository {
    async fn join_event(
        &self,
        event_id: Uuid,
        user_id: Uuid,
    ) -> Result<(EventRegistration, Event, Option<Wallet>), DatabaseError>;

    async fn leave_event(
        &self,
        event_id: Uuid,
        user_id: Uuid,
    ) -> Result<(EventRegistration, Event, Option<Wallet>), DatabaseError>;

    async fn find_registration(
        &self,
        event_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<EventRegistration>, DatabaseError>;
}

pub trait Filterable {
    fn apply_filter(&self, qb: &mut QueryBuilder<'_, sqlx::Postgres>);
}

fn build_paginated_query<'a, F: Filterable>(
    table: &'a str,
    filter: &'a F,
    pagination: &'a PaginationRequest,
    order_by: &'a str,
) -> QueryBuilder<'a, sqlx::Postgres> {
    let mut qb = QueryBuilder::new(format!("SELECT * FROM {table} WHERE 1=1"));
    filter.apply_filter(&mut qb);
    qb.push(format!(" ORDER BY {order_by}"));
    qb.push(" LIMIT ")
        .push_bind(pagination.per_page.unwrap_or(3));
    qb.push(" OFFSET ")
        .push_bind(pagination.page * pagination.per_page.unwrap_or(0));
    qb
}
