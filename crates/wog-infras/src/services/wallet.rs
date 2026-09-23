use rust_decimal::Decimal;
use uuid::Uuid;

use crate::{
    errors::DatabaseError,
    models::Wallet,
    repos::{DynUserRepository, DynWalletRepository},
};

#[derive(Clone)]
pub struct WalletServices {
    wallet_repo: DynWalletRepository,
    user_repo: DynUserRepository,
}

impl WalletServices {
    pub fn new(wallet_repo: DynWalletRepository, user_repo: DynUserRepository) -> Self {
        Self {
            wallet_repo,
            user_repo,
        }
    }

    pub async fn get_wallet(&self, user_id: Uuid) -> Result<Wallet, DatabaseError> {
        self.wallet_repo.get_or_create_wallet(user_id).await
    }

    /// Send `amount` credits straight from `sender_id`'s wallet to another
    /// user's, identified by `recipient` (their username, or their email
    /// if it contains an `@`). Purely an internal ledger move -- this is
    /// the only way credits move between accounts. Returns the sender's
    /// updated wallet and the recipient's user record (so the caller can
    /// confirm who received it).
    pub async fn transfer_credits(
        &self,
        sender_id: Uuid,
        recipient: &str,
        amount: Decimal,
    ) -> Result<(Wallet, crate::models::User), DatabaseError> {
        if amount <= Decimal::ZERO {
            return Err(DatabaseError::ValidationError(
                "Amount must be greater than zero".into(),
            ));
        }

        let recipient_identifier = recipient.trim();
        if recipient_identifier.is_empty() {
            return Err(DatabaseError::ValidationError(
                "Recipient username or email is required".into(),
            ));
        }

        let recipient_user = if recipient_identifier.contains('@') {
            self.user_repo.find_by_email(recipient_identifier).await?
        } else {
            self.user_repo.find_by_username(recipient_identifier).await?
        }
        .ok_or_else(|| {
            DatabaseError::ValueNotFound("No user found with that username or email".into())
        })?;

        if recipient_user.id == sender_id {
            return Err(DatabaseError::ValidationError(
                "You can't send credits to yourself".into(),
            ));
        }

        let sender_user = self
            .user_repo
            .find_by_id(sender_id)
            .await?
            .ok_or_else(|| DatabaseError::ValueNotFound("User not found".into()))?;

        let (sender_wallet, _recipient_wallet) = self
            .wallet_repo
            .transfer_credits(
                sender_id,
                recipient_user.id,
                amount,
                &format!("Sent to {}", recipient_user.username),
                &format!("Received from {}", sender_user.username),
            )
            .await?;

        Ok((sender_wallet, recipient_user))
    }
}
