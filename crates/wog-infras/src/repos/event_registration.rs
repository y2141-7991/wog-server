use async_trait::async_trait;
use rust_decimal::Decimal;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    errors::DatabaseError,
    models::{Event, EventRegistration, Wallet, WalletTransaction},
    repos::EventRegistrationRepository,
};

#[derive(Clone)]
pub struct PgEventRegistrationRepo {
    pub pg_pool: PgPool,
}

impl PgEventRegistrationRepo {
    pub fn new(pg_pool: PgPool) -> Self {
        Self { pg_pool }
    }
}

/// Get-or-create the wallet for `user_id` and return it, within `tx`.
async fn get_or_create_wallet(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: Uuid,
) -> Result<Wallet, DatabaseError> {
    Ok(sqlx::query_as::<_, Wallet>(
        r#"
            INSERT INTO wallets (user_id)
            VALUES ($1)
            ON CONFLICT (user_id) DO UPDATE SET user_id = wallets.user_id
            RETURNING *
        "#,
    )
    .bind(user_id)
    .fetch_one(&mut **tx)
    .await?)
}

#[async_trait]
impl EventRegistrationRepository for PgEventRegistrationRepo {
    /// Pay (if the event isn't free) and register for an event, all in one
    /// DB transaction: claim a seat, debit the joiner's wallet, credit the
    /// organizer's wallet, log both `EVENT_JOIN` ledger rows, and insert the
    /// registration. The seat-claim's `WHERE` guard is the actual race-safe
    /// capacity check; the lookup right before it only exists to give a
    /// precise error on the common, uncontested path.
    async fn join_event(
        &self,
        event_id: Uuid,
        user_id: Uuid,
    ) -> Result<(EventRegistration, Event, Option<Wallet>), DatabaseError> {
        let mut tx = self.pg_pool.begin().await?;

        let existing = sqlx::query_as::<_, Event>("SELECT * FROM events WHERE id = $1")
            .bind(event_id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| DatabaseError::ValueNotFound("Event not found".into()))?;

        if existing.organizer_id == user_id {
            return Err(DatabaseError::CannotJoinOwnEvent);
        }
        if existing.status != "OPEN" {
            return Err(DatabaseError::EventNotOpen);
        }
        if existing.registered_count >= existing.capacity {
            return Err(DatabaseError::EventFull);
        }

        let event = sqlx::query_as::<_, Event>(
            r#"
                UPDATE events
                SET registered_count = registered_count + 1, updated_at = NOW()
                WHERE id = $1 AND status = 'OPEN' AND registered_count < capacity
                RETURNING *
            "#,
        )
        .bind(event_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(DatabaseError::EventFull)?;

        let mut wallet_after: Option<Wallet> = None;

        if event.price > Decimal::ZERO {
            let wallet_before = get_or_create_wallet(&mut tx, user_id).await?;

            if wallet_before.balance < event.price {
                return Err(DatabaseError::InsufficientBalance);
            }

            let wallet = sqlx::query_as::<_, Wallet>(
                r#"
                    UPDATE wallets
                    SET balance = balance - $2, version = version + 1, updated_at = NOW()
                    WHERE user_id = $1
                    RETURNING *
                "#,
            )
            .bind(user_id)
            .bind(event.price)
            .fetch_one(&mut *tx)
            .await?;

            wallet_after = Some(wallet);
        }

        // `unique_registration UNIQUE (event_id, user_id)` isn't scoped by
        // status, so a user who left and is rejoining already has a row
        // (CANCELLED) occupying that slot -- reactivate it instead of
        // inserting a new one. If the existing row is already CONFIRMED,
        // the WHERE guard blocks the update and we surface AlreadyRegistered.
        let mut registration = sqlx::query_as::<_, EventRegistration>(
            r#"
                INSERT INTO event_registrations
                    (event_id, user_id, amount_paid, status, registered_at, cancelled_at, wallet_tx_id)
                VALUES ($1, $2, $3, 'CONFIRMED', NOW(), NULL, NULL)
                ON CONFLICT (event_id, user_id) DO UPDATE
                SET amount_paid = EXCLUDED.amount_paid,
                    status = 'CONFIRMED',
                    registered_at = NOW(),
                    cancelled_at = NULL,
                    wallet_tx_id = NULL
                WHERE event_registrations.status != 'CONFIRMED'
                RETURNING *
            "#,
        )
        .bind(event_id)
        .bind(user_id)
        .bind(event.price)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(DatabaseError::AlreadyRegistered)?;

        if let Some(ref wallet) = wallet_after {
            let wallet_tx = sqlx::query_as::<_, WalletTransaction>(
                r#"
                    INSERT INTO wallet_transactions
                        (wallet_id, tx_type, amount, balance_before, balance_after, reference_type, reference_id, description)
                    VALUES ($1, 'EVENT_JOIN', $2, $3, $4, 'event_registration', $5, $6)
                    RETURNING *
                "#,
            )
            .bind(wallet.id)
            .bind(event.price)
            .bind(wallet.balance + event.price)
            .bind(wallet.balance)
            .bind(registration.id)
            .bind(format!("Joined event: {}", event.title))
            .fetch_one(&mut *tx)
            .await?;

            registration = sqlx::query_as::<_, EventRegistration>(
                "UPDATE event_registrations SET wallet_tx_id = $2 WHERE id = $1 RETURNING *",
            )
            .bind(registration.id)
            .bind(wallet_tx.id)
            .fetch_one(&mut *tx)
            .await?;

            // Pay the organizer: credit the same amount into their wallet
            // (auto-creating it if this is their first sale) and log a
            // matching ledger row referencing the same registration.
            let organizer_wallet_before = get_or_create_wallet(&mut tx, event.organizer_id).await?;

            let organizer_wallet = sqlx::query_as::<_, Wallet>(
                r#"
                    UPDATE wallets
                    SET balance = balance + $2, version = version + 1, updated_at = NOW()
                    WHERE user_id = $1
                    RETURNING *
                "#,
            )
            .bind(event.organizer_id)
            .bind(event.price)
            .fetch_one(&mut *tx)
            .await?;

            sqlx::query(
                r#"
                    INSERT INTO wallet_transactions
                        (wallet_id, tx_type, amount, balance_before, balance_after, reference_type, reference_id, description)
                    VALUES ($1, 'EVENT_JOIN', $2, $3, $4, 'event_registration', $5, $6)
                "#,
            )
            .bind(organizer_wallet.id)
            .bind(event.price)
            .bind(organizer_wallet_before.balance)
            .bind(organizer_wallet.balance)
            .bind(registration.id)
            .bind(format!("Payment received for event: {}", event.title))
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok((registration, event, wallet_after))
    }

    /// Cancel a confirmed registration, free up the seat, and refund
    /// whatever was paid (if anything) -- reversing both sides of the
    /// original payment -- all in one DB transaction.
    async fn leave_event(
        &self,
        event_id: Uuid,
        user_id: Uuid,
    ) -> Result<(EventRegistration, Event, Option<Wallet>), DatabaseError> {
        let mut tx = self.pg_pool.begin().await?;

        let existing = sqlx::query_as::<_, EventRegistration>(
            r#"
                SELECT * FROM event_registrations
                WHERE event_id = $1 AND user_id = $2 AND status = 'CONFIRMED'
            "#,
        )
        .bind(event_id)
        .bind(user_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(DatabaseError::RegistrationNotFound)?;

        let registration = sqlx::query_as::<_, EventRegistration>(
            r#"
                UPDATE event_registrations
                SET status = 'CANCELLED', cancelled_at = NOW()
                WHERE id = $1
                RETURNING *
            "#,
        )
        .bind(existing.id)
        .fetch_one(&mut *tx)
        .await?;

        let event = sqlx::query_as::<_, Event>(
            r#"
                UPDATE events
                SET registered_count = GREATEST(registered_count - 1, 0), updated_at = NOW()
                WHERE id = $1
                RETURNING *
            "#,
        )
        .bind(event_id)
        .fetch_one(&mut *tx)
        .await?;

        let mut wallet_after: Option<Wallet> = None;

        if registration.amount_paid > Decimal::ZERO {
            let wallet_before = get_or_create_wallet(&mut tx, user_id).await?;

            let wallet = sqlx::query_as::<_, Wallet>(
                r#"
                    UPDATE wallets
                    SET balance = balance + $2, version = version + 1, updated_at = NOW()
                    WHERE user_id = $1
                    RETURNING *
                "#,
            )
            .bind(user_id)
            .bind(registration.amount_paid)
            .fetch_one(&mut *tx)
            .await?;

            sqlx::query(
                r#"
                    INSERT INTO wallet_transactions
                        (wallet_id, tx_type, amount, balance_before, balance_after, reference_type, reference_id, description)
                    VALUES ($1, 'REFUND', $2, $3, $4, 'event_registration', $5, $6)
                "#,
            )
            .bind(wallet.id)
            .bind(registration.amount_paid)
            .bind(wallet_before.balance)
            .bind(wallet.balance)
            .bind(registration.id)
            .bind(format!("Refund for cancelling event: {}", event.title))
            .execute(&mut *tx)
            .await?;

            wallet_after = Some(wallet);

            // Reverse the organizer's side of the original payment: pull the
            // same amount back out of their wallet. Their wallet already
            // exists (it was credited when this registration was created),
            // so this just debits it; the `positive_balance` check guards
            // against them somehow having spent below what's owed back.
            let organizer_wallet_before = get_or_create_wallet(&mut tx, event.organizer_id).await?;

            let organizer_wallet = sqlx::query_as::<_, Wallet>(
                r#"
                    UPDATE wallets
                    SET balance = balance - $2, version = version + 1, updated_at = NOW()
                    WHERE user_id = $1
                    RETURNING *
                "#,
            )
            .bind(event.organizer_id)
            .bind(registration.amount_paid)
            .fetch_one(&mut *tx)
            .await?;

            sqlx::query(
                r#"
                    INSERT INTO wallet_transactions
                        (wallet_id, tx_type, amount, balance_before, balance_after, reference_type, reference_id, description)
                    VALUES ($1, 'REFUND', $2, $3, $4, 'event_registration', $5, $6)
                "#,
            )
            .bind(organizer_wallet.id)
            .bind(registration.amount_paid)
            .bind(organizer_wallet_before.balance)
            .bind(organizer_wallet.balance)
            .bind(registration.id)
            .bind(format!(
                "Payment reversed (cancelled registration) for event: {}",
                event.title
            ))
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok((registration, event, wallet_after))
    }

    async fn find_registration(
        &self,
        event_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<EventRegistration>, DatabaseError> {
        Ok(sqlx::query_as::<_, EventRegistration>(
            r#"
                SELECT * FROM event_registrations
                WHERE event_id = $1 AND user_id = $2 AND status = 'CONFIRMED'
            "#,
        )
        .bind(event_id)
        .bind(user_id)
        .fetch_optional(&self.pg_pool)
        .await?)
    }
}
