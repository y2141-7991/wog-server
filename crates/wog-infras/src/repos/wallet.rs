use async_trait::async_trait;
use rust_decimal::Decimal;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{errors::DatabaseError, models::Wallet, repos::WalletRepository};

#[derive(Clone)]
pub struct PgWalletRepo {
    pub pg_pool: PgPool,
}

impl PgWalletRepo {
    pub fn new(pg_pool: PgPool) -> Self {
        Self { pg_pool }
    }
}

#[async_trait]
impl WalletRepository for PgWalletRepo {
    async fn get_or_create_wallet(&self, user_id: Uuid) -> Result<Wallet, DatabaseError> {
        Ok(sqlx::query_as::<_, Wallet>(
            r#"
                INSERT INTO wallets (user_id)
                VALUES ($1)
                ON CONFLICT (user_id) DO UPDATE SET user_id = wallets.user_id
                RETURNING *
            "#,
        )
        .bind(user_id)
        .fetch_one(&self.pg_pool)
        .await?)
    }

    /// Move `amount` credits from `sender_id`'s wallet to `recipient_id`'s
    /// wallet in one DB transaction, recording a `TRANSFER_OUT` /
    /// `TRANSFER_IN` pair in `wallet_transactions`. Both wallet rows are
    /// locked in a fixed order (by user id) before either is touched, so a
    /// concurrent transfer running the other way between the same two
    /// users can't deadlock against this one.
    async fn transfer_credits(
        &self,
        sender_id: Uuid,
        recipient_id: Uuid,
        amount: Decimal,
        sender_description: &str,
        recipient_description: &str,
    ) -> Result<(Wallet, Wallet), DatabaseError> {
        let mut tx = self.pg_pool.begin().await?;

        sqlx::query("INSERT INTO wallets (user_id) VALUES ($1) ON CONFLICT (user_id) DO NOTHING")
            .bind(sender_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO wallets (user_id) VALUES ($1) ON CONFLICT (user_id) DO NOTHING")
            .bind(recipient_id)
            .execute(&mut *tx)
            .await?;

        let (first, second) = if sender_id < recipient_id {
            (sender_id, recipient_id)
        } else {
            (recipient_id, sender_id)
        };
        sqlx::query("SELECT id FROM wallets WHERE user_id = $1 FOR UPDATE")
            .bind(first)
            .fetch_one(&mut *tx)
            .await?;
        sqlx::query("SELECT id FROM wallets WHERE user_id = $1 FOR UPDATE")
            .bind(second)
            .fetch_one(&mut *tx)
            .await?;

        let sender_before = sqlx::query_as::<_, Wallet>("SELECT * FROM wallets WHERE user_id = $1")
            .bind(sender_id)
            .fetch_one(&mut *tx)
            .await?;
        if sender_before.balance < amount {
            return Err(DatabaseError::InsufficientBalance);
        }

        // The `positive_balance` check is the actual race-safe guard: this
        // only succeeds if the balance is still enough, even though we
        // already checked above under the row lock.
        let sender_wallet = sqlx::query_as::<_, Wallet>(
            r#"
                UPDATE wallets
                SET balance = balance - $2, version = version + 1, updated_at = NOW()
                WHERE user_id = $1 AND balance >= $2
                RETURNING *
            "#,
        )
        .bind(sender_id)
        .bind(amount)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(DatabaseError::InsufficientBalance)?;

        let recipient_before = sqlx::query_as::<_, Wallet>("SELECT * FROM wallets WHERE user_id = $1")
            .bind(recipient_id)
            .fetch_one(&mut *tx)
            .await?;

        let recipient_wallet = sqlx::query_as::<_, Wallet>(
            r#"
                UPDATE wallets
                SET balance = balance + $2, version = version + 1, updated_at = NOW()
                WHERE user_id = $1
                RETURNING *
            "#,
        )
        .bind(recipient_id)
        .bind(amount)
        .fetch_one(&mut *tx)
        .await?;

        let transfer_ref = Uuid::new_v4();

        sqlx::query(
            r#"
                INSERT INTO wallet_transactions
                    (wallet_id, tx_type, amount, balance_before, balance_after, reference_type, reference_id, description)
                VALUES ($1, 'TRANSFER_OUT', $2, $3, $4, 'transfer', $5, $6)
            "#,
        )
        .bind(sender_wallet.id)
        .bind(amount)
        .bind(sender_before.balance)
        .bind(sender_wallet.balance)
        .bind(transfer_ref)
        .bind(sender_description)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            r#"
                INSERT INTO wallet_transactions
                    (wallet_id, tx_type, amount, balance_before, balance_after, reference_type, reference_id, description)
                VALUES ($1, 'TRANSFER_IN', $2, $3, $4, 'transfer', $5, $6)
            "#,
        )
        .bind(recipient_wallet.id)
        .bind(amount)
        .bind(recipient_before.balance)
        .bind(recipient_wallet.balance)
        .bind(transfer_ref)
        .bind(recipient_description)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok((sender_wallet, recipient_wallet))
    }
}
