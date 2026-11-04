use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;
use wog_infras::models::{SupportConversationRow, SupportMessage};

/// One chat message. Same shape as the `message` WebSocket event's payload.
#[derive(Debug, Serialize, ToSchema)]
pub struct MessageResponse {
    pub id: Uuid,
    pub conversation_id: Uuid,
    pub sender_id: Uuid,
    /// `user` or `admin`
    pub sender_role: String,
    pub body: String,
    pub client_msg_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub read_at: Option<DateTime<Utc>>,
}

impl From<SupportMessage> for MessageResponse {
    fn from(m: SupportMessage) -> Self {
        Self {
            id: m.id,
            conversation_id: m.conversation_id,
            sender_id: m.sender_id,
            sender_role: m.sender_role,
            body: m.body,
            client_msg_id: m.client_msg_id,
            created_at: m.created_at,
            read_at: m.read_at,
        }
    }
}

/// A conversation with its user: an inbox row. Same shape as the
/// `conversation_updated` WebSocket event's payload.
#[derive(Debug, Serialize, ToSchema)]
pub struct ConversationResponse {
    pub id: Uuid,
    pub user_id: Uuid,
    /// `open` or `closed`
    pub status: String,
    pub subject: Option<String>,
    pub last_message_at: DateTime<Utc>,
    pub last_message_preview: String,
    /// Admin replies the user has not read yet
    pub user_unread_count: i32,
    /// User messages no admin has read yet
    pub admin_unread_count: i32,
    pub assigned_admin_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
    pub username: String,
    pub email: String,
    pub avatar_url: Option<String>,
    /// Name of the user's running paid plan
    pub plan_name: Option<String>,
    /// The user has a live chat connection
    pub online: bool,
}

impl From<SupportConversationRow> for ConversationResponse {
    fn from(c: SupportConversationRow) -> Self {
        Self {
            id: c.id,
            user_id: c.user_id,
            status: c.status,
            subject: c.subject,
            last_message_at: c.last_message_at,
            last_message_preview: c.last_message_preview,
            user_unread_count: c.user_unread_count,
            admin_unread_count: c.admin_unread_count,
            assigned_admin_id: c.assigned_admin_id,
            created_at: c.created_at,
            closed_at: c.closed_at,
            username: c.username,
            email: c.email,
            avatar_url: c.avatar_url,
            plan_name: c.plan_name,
            online: c.online,
        }
    }
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct MessagesQuery {
    /// Messages older than this message id (scroll back)
    pub before: Option<Uuid>,
    /// Messages newer than this message id (catch up after a reconnect)
    pub after: Option<Uuid>,
    /// Page size (1-100, default 50)
    pub limit: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MessagesResponse {
    /// Oldest first
    pub messages: Vec<MessageResponse>,
    /// More messages exist beyond this page in the direction asked for
    pub has_more: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UserConversationResponse {
    /// `null` until the user has written their first message
    pub conversation: Option<ConversationResponse>,
    pub messages: Vec<MessageResponse>,
    pub has_more: bool,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct InboxQuery {
    /// `open`, `closed` or `all` (default `all`)
    pub status: Option<String>,
    /// Matches username or email
    pub q: Option<String>,
    /// Zero-based page
    pub page: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct InboxResponse {
    pub conversations: Vec<ConversationResponse>,
    pub page: i64,
    pub per_page: i64,
    pub total: i64,
    /// Unread user messages across all open conversations
    pub total_unread: i64,
}
