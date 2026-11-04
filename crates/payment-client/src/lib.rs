//! Bank-transfer payment providers.
//!
//! A provider knows how to show a customer where to send money (bank details
//! and a QR code) and how to look up the transfers that arrived, which is what
//! the webhook-free reconciliation path uses. SePay is the only implementation.

pub mod sepay;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;
use thiserror::Error;

pub use sepay::{SepayClient, SepayConfig};

/// The receiving bank account shown to the customer.
#[derive(Debug, Clone)]
pub struct BankDetails {
    pub bank_name: String,
    pub account_number: String,
    pub account_holder: String,
}

/// A money movement on the receiving account, in the provider-neutral shape
/// both the webhook and the transactions API are mapped to.
#[derive(Debug, Clone)]
pub struct Transfer {
    /// The provider's transaction id; stable across retries and replays.
    pub id: i64,
    pub incoming: bool,
    /// Integer VND, always positive.
    pub amount_vnd: i64,
    /// The transfer content the sender typed.
    pub content: String,
    /// The payment code the provider extracted from the content, if any.
    pub code: Option<String>,
    /// The payload as received, kept for the audit trail.
    pub raw: Value,
}

#[async_trait]
pub trait PaymentProvider: Send + Sync {
    fn bank_details(&self) -> &BankDetails;

    /// URL of a QR image that pre-fills the amount and the transfer content.
    fn qr_url(&self, amount_vnd: i64, transfer_content: &str) -> String;

    /// Whether `recent_transfers` can be used (an API token is configured).
    fn can_list_transfers(&self) -> bool;

    /// Transfers that arrived on the receiving account since `since`.
    async fn recent_transfers(&self, since: DateTime<Utc>) -> Result<Vec<Transfer>>;
}

#[derive(Error, Debug)]
pub enum PaymentError {
    #[error("HTTP request failed: {0}")]
    Request(#[from] reqwest::Error),

    #[error("Unexpected provider response: {0}")]
    Parse(String),

    #[error("The provider answered {status}: {body}")]
    Status { status: u16, body: String },

    #[error("The provider API token is not configured")]
    NotConfigured,
}

pub type Result<T> = std::result::Result<T, PaymentError>;
