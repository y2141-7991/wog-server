use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, FromRow, Deserialize, Serialize)]
pub struct User {
    id: Uuid,
    username: String,
    email: String,
    #[serde(skip_serializing)]
    password: String,
    avatar_url: String,
    is_online: bool,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Deserialize, Serialize)]
pub struct UserProfile {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub avatar_url: Option<String>,
    pub is_online: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Deserialize, Serialize)]
pub struct Room {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub is_private: bool,
    pub owner_id: Uuid,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Deserialize, Serialize)]
pub struct RoomMember {
    pub room_id: Uuid,
    pub user_id: Uuid,
    pub role: String,
    pub joined_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Deserialize, Serialize)]
pub struct Message {
    pub id: Uuid,
    pub room_id: Uuid,
    pub sender_id: Uuid,
    pub content: String,
    pub msg_type: String,
    pub is_edited: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Deserialize, Serialize)]
pub struct MessageWithSender {
    pub id: Uuid,
    pub room_id: Uuid,
    pub sender_id: Uuid,
    pub sender_name: String,
    pub sender_avatar: Option<String>,
    pub content: String,
    pub msg_type: String,
    pub is_edited: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Wallet {
    pub id: Uuid,
    pub user_id: Uuid,
    pub balance: Decimal,
    pub version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "VARCHAR", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TxType {
    TopUp,
    EventJoin,
    Refund,
    Withdrawal,
}

impl TxType {
    pub fn as_str(&self) -> &'static str {
        match self {
            TxType::TopUp => "TOP_UP",
            TxType::EventJoin => "EVENT_JOIN",
            TxType::Refund => "REFUND",
            TxType::Withdrawal => "WITHDRAWAL",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct WalletTransaction {
    pub id: Uuid,
    pub wallet_id: Uuid,
    pub tx_type: String,
    pub amount: Decimal,
    pub balance_before: Decimal,
    pub balance_after: Decimal,
    pub reference_type: Option<String>,
    pub reference_id: Option<Uuid>,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
}

// ══════════════════════ Event ══════════════════════

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Event {
    pub id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub organizer_id: Uuid,
    pub price: Decimal,
    pub capacity: i32,
    pub registered_count: i32,
    pub status: String,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub location: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct EventRegistration {
    pub id: Uuid,
    pub event_id: Uuid,
    pub user_id: Uuid,
    pub wallet_tx_id: Option<Uuid>,
    pub amount_paid: Decimal,
    pub status: String,
    pub registered_at: DateTime<Utc>,
    pub cancelled_at: Option<DateTime<Utc>>,
}