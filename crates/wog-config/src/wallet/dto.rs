use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use wog_infras::models::Wallet;

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct WalletResponse {
    pub balance: Decimal,
    pub updated_at: DateTime<Utc>,
}

impl From<Wallet> for WalletResponse {
    fn from(value: Wallet) -> Self {
        Self {
            balance: value.balance,
            updated_at: value.updated_at,
        }
    }
}

/// Send `amount` credits directly to another user's wallet, identified
/// by their username or email. Purely an in-app ledger move -- this is
/// the only way credits move between accounts.
#[derive(Debug, Deserialize, ToSchema)]
pub struct TransferCreditsRequest {
    pub recipient: String,
    pub amount: Decimal,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TransferCreditsResponse {
    pub wallet: WalletResponse,
    pub recipient_username: String,
}
