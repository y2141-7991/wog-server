use uuid::Uuid;

use crate::{
    errors::DatabaseError,
    models::{PostedMessage, SupportConversationRow, SupportMessage},
    repos::DynSupportRepository,
    support::{Audience, ChatRole, DynSupportHub, ServerEvent},
};

pub const MAX_BODY_CHARS: usize = 2000;
pub const MAX_CLIENT_MSG_ID_LEN: usize = 64;
pub const DEFAULT_PAGE_SIZE: i64 = 50;
pub const MAX_PAGE_SIZE: i64 = 100;
pub const INBOX_PER_PAGE: i64 = 30;
const PREVIEW_CHARS: usize = 120;

/// Why a chat operation was refused. `code()` is what WebSocket clients see
/// in `error` events.
#[derive(Debug, Clone, thiserror::Error)]
pub enum ChatError {
    #[error("Message is empty")]
    Empty,
    #[error("Message is too long (max {MAX_BODY_CHARS} characters)")]
    TooLong,
    #[error("client_msg_id must be 1-{MAX_CLIENT_MSG_ID_LEN} printable characters")]
    BadClientMsgId,
    #[error("{0}")]
    BadRequest(String),
    #[error("Conversation not found")]
    NotFound,
    #[error("You are not allowed to do that")]
    Forbidden,
    #[error("This conversation is closed")]
    Closed,
    #[error("{0}")]
    Db(DatabaseError),
}

impl ChatError {
    pub fn code(&self) -> &'static str {
        match self {
            ChatError::Empty => "empty_message",
            ChatError::TooLong => "message_too_long",
            ChatError::BadClientMsgId => "invalid_client_msg_id",
            ChatError::BadRequest(_) => "bad_request",
            ChatError::NotFound => "not_found",
            ChatError::Forbidden => "forbidden",
            ChatError::Closed => "conversation_closed",
            ChatError::Db(_) => "internal",
        }
    }

    /// Safe to show a client: database details never leave the server.
    pub fn public_message(&self) -> String {
        match self {
            ChatError::Db(e) => {
                tracing::error!("Support chat database error: {e}");
                "Something went wrong, please try again".to_string()
            }
            other => other.to_string(),
        }
    }
}

impl From<DatabaseError> for ChatError {
    fn from(e: DatabaseError) -> Self {
        match e {
            DatabaseError::ValueNotFound(_) => ChatError::NotFound,
            DatabaseError::ValidationError(msg) => ChatError::BadRequest(msg),
            other => ChatError::Db(other),
        }
    }
}

/// One page of a conversation's history.
pub struct MessagePage {
    pub messages: Vec<SupportMessage>,
    pub has_more: bool,
}

pub struct Inbox {
    pub rows: Vec<SupportConversationRow>,
    pub total: i64,
    pub total_unread: i64,
}

#[derive(Clone)]
pub struct SupportServices {
    repo: DynSupportRepository,
    hub: DynSupportHub,
}

/// Trim, and refuse empty or over-long text. Length is counted in characters,
/// which is what the database CHECK counts too.
fn clean_body(body: &str) -> Result<String, ChatError> {
    let body = body.trim();
    if body.is_empty() {
        return Err(ChatError::Empty);
    }
    if body.chars().count() > MAX_BODY_CHARS {
        return Err(ChatError::TooLong);
    }
    Ok(body.to_string())
}

fn clean_client_msg_id(id: &str) -> Result<&str, ChatError> {
    let ok = !id.is_empty()
        && id.chars().count() <= MAX_CLIENT_MSG_ID_LEN
        && id.chars().all(|c| !c.is_control());
    if ok {
        Ok(id)
    } else {
        Err(ChatError::BadClientMsgId)
    }
}

/// One line, collapsed whitespace, cut to a short length.
fn make_preview(body: &str) -> String {
    let flat = body.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= PREVIEW_CHARS {
        return flat;
    }
    let cut: String = flat.chars().take(PREVIEW_CHARS - 1).collect();
    format!("{cut}…")
}

fn clamp_limit(limit: Option<i64>) -> i64 {
    limit.unwrap_or(DEFAULT_PAGE_SIZE).clamp(1, MAX_PAGE_SIZE)
}

impl SupportServices {
    pub fn new(repo: DynSupportRepository, hub: DynSupportHub) -> Self {
        Self { repo, hub }
    }

    pub fn hub(&self) -> &DynSupportHub {
        &self.hub
    }

    pub fn publish(&self, audience: Audience, event: &ServerEvent) {
        self.hub.publish(audience, event.frame());
    }

    fn with_online(&self, mut row: SupportConversationRow) -> SupportConversationRow {
        row.online = !self.hub.online_users(&[row.user_id]).is_empty();
        row
    }

    // ── sending ──

    /// A user's message: stored, then delivered. Persisting comes first, so
    /// nothing is announced that was not saved. The caller acks the sender.
    pub async fn post_user_message(
        &self,
        user_id: Uuid,
        body: &str,
        client_msg_id: &str,
    ) -> Result<PostedMessage, ChatError> {
        let body = clean_body(body)?;
        let client_msg_id = clean_client_msg_id(client_msg_id)?;
        let posted = self
            .repo
            .post_user_message(user_id, &body, &make_preview(&body), client_msg_id)
            .await?;
        self.after_post(&posted).await;
        Ok(posted)
    }

    pub async fn post_admin_message(
        &self,
        admin_id: Uuid,
        conversation_id: Uuid,
        body: &str,
        client_msg_id: &str,
    ) -> Result<PostedMessage, ChatError> {
        let body = clean_body(body)?;
        let client_msg_id = clean_client_msg_id(client_msg_id)?;
        let posted = self
            .repo
            .post_admin_message(
                admin_id,
                conversation_id,
                &body,
                &make_preview(&body),
                client_msg_id,
            )
            .await
            .map_err(|e| match e {
                DatabaseError::ValidationError(_) => ChatError::Closed,
                other => other.into(),
            })?;
        self.after_post(&posted).await;
        Ok(posted)
    }

    async fn after_post(&self, posted: &PostedMessage) {
        if posted.duplicate {
            return;
        }
        let conv = &posted.conversation;
        let message = ServerEvent::Message {
            message: posted.message.clone(),
        };
        self.publish(Audience::User(conv.user_id), &message);
        self.publish(Audience::AdminsInConversation(conv.id), &message);
        self.broadcast_conversation(conv.clone()).await;
    }

    /// Tell admins (all of them) and the owner that this row changed.
    pub async fn broadcast_conversation(&self, row: SupportConversationRow) {
        let total_unread = self.repo.total_admin_unread().await.ok();
        let row = self.with_online(row);
        let user_id = row.user_id;
        self.publish(
            Audience::Admins,
            &ServerEvent::ConversationUpdated {
                conversation: Box::new(row.clone()),
                total_unread,
            },
        );
        self.publish(
            Audience::User(user_id),
            &ServerEvent::ConversationUpdated {
                conversation: Box::new(row),
                total_unread: None,
            },
        );
    }

    // ── read state, typing, status ──

    /// Who owns this conversation. Users may only touch their own; to everyone
    /// else it does not exist.
    pub async fn authorize(
        &self,
        role: ChatRole,
        actor_id: Uuid,
        conversation_id: Uuid,
    ) -> Result<Uuid, ChatError> {
        let conv = self
            .repo
            .find_conversation(conversation_id)
            .await?
            .ok_or(ChatError::NotFound)?;
        if role == ChatRole::User && conv.user_id != actor_id {
            return Err(ChatError::NotFound);
        }
        Ok(conv.user_id)
    }

    pub async fn own_conversation_id(&self, user_id: Uuid) -> Result<Option<Uuid>, ChatError> {
        Ok(self
            .repo
            .find_user_conversation(user_id)
            .await?
            .map(|c| c.id))
    }

    /// Typing indicators go to the other side only: from a user to admins in
    /// that conversation, from an admin to the user.
    pub fn typing(
        &self,
        role: ChatRole,
        actor_id: Uuid,
        conversation_id: Uuid,
        owner_id: Uuid,
        is_typing: bool,
    ) {
        let event = ServerEvent::Typing {
            conversation_id,
            user_id: actor_id,
            role,
            is_typing,
        };
        match role {
            ChatRole::User => self.publish(Audience::AdminsInConversation(conversation_id), &event),
            ChatRole::Admin => self.publish(Audience::User(owner_id), &event),
        }
    }

    pub async fn mark_read(
        &self,
        role: ChatRole,
        actor_id: Uuid,
        conversation_id: Uuid,
    ) -> Result<(), ChatError> {
        let owner_id = self.authorize(role, actor_id, conversation_id).await?;
        let Some((row, at)) = self
            .repo
            .mark_read(conversation_id, role == ChatRole::Admin)
            .await?
        else {
            return Ok(());
        };
        let event = ServerEvent::Read {
            conversation_id,
            by: role,
            at,
        };
        self.publish(Audience::User(owner_id), &event);
        self.publish(Audience::AdminsInConversation(conversation_id), &event);
        self.broadcast_conversation(row).await;
        Ok(())
    }

    pub async fn set_open(
        &self,
        conversation_id: Uuid,
        open: bool,
    ) -> Result<SupportConversationRow, ChatError> {
        let (row, changed) = self.repo.set_status(conversation_id, open).await?;
        if changed {
            self.broadcast_conversation(row.clone()).await;
        }
        Ok(self.with_online(row))
    }

    pub async fn assign(
        &self,
        conversation_id: Uuid,
        admin_id: Uuid,
    ) -> Result<SupportConversationRow, ChatError> {
        let row = self.repo.assign(conversation_id, admin_id).await?;
        self.broadcast_conversation(row.clone()).await;
        Ok(self.with_online(row))
    }

    // ── reading ──

    /// The user's conversation (open, else the latest closed) and a page of it.
    pub async fn user_view(
        &self,
        user_id: Uuid,
        before: Option<Uuid>,
        after: Option<Uuid>,
        limit: Option<i64>,
    ) -> Result<(Option<SupportConversationRow>, MessagePage), ChatError> {
        let Some(conv) = self.repo.find_user_conversation(user_id).await? else {
            return Ok((
                None,
                MessagePage {
                    messages: vec![],
                    has_more: false,
                },
            ));
        };
        let (messages, has_more) = self
            .repo
            .list_messages(conv.id, before, after, clamp_limit(limit))
            .await?;
        Ok((
            Some(self.with_online(conv)),
            MessagePage { messages, has_more },
        ))
    }

    /// History of one conversation for its owner or an admin; anyone else
    /// gets "not found".
    pub async fn messages(
        &self,
        role: ChatRole,
        actor_id: Uuid,
        conversation_id: Uuid,
        before: Option<Uuid>,
        after: Option<Uuid>,
        limit: Option<i64>,
    ) -> Result<MessagePage, ChatError> {
        self.authorize(role, actor_id, conversation_id).await?;
        let (messages, has_more) = self
            .repo
            .list_messages(conversation_id, before, after, clamp_limit(limit))
            .await?;
        Ok(MessagePage { messages, has_more })
    }

    pub async fn conversation_for_admin(
        &self,
        conversation_id: Uuid,
    ) -> Result<SupportConversationRow, ChatError> {
        let row = self
            .repo
            .find_conversation(conversation_id)
            .await?
            .ok_or(ChatError::NotFound)?;
        Ok(self.with_online(row))
    }

    pub async fn inbox(
        &self,
        status: Option<&str>,
        search: Option<&str>,
        page: i64,
    ) -> Result<Inbox, ChatError> {
        let status = match status
            .map(str::trim)
            .filter(|s| !s.is_empty() && *s != "all")
        {
            Some(s @ ("open" | "closed")) => Some(s),
            Some(_) => {
                return Err(ChatError::BadRequest(
                    "status must be open or closed".into(),
                ));
            }
            None => None,
        };
        let search = search.map(str::trim).filter(|q| !q.is_empty());
        let rows = self
            .repo
            .list_conversations(status, search, INBOX_PER_PAGE, page.max(0) * INBOX_PER_PAGE)
            .await?;
        let total = rows.first().map_or(0, |r| r.total);
        let ids: Vec<Uuid> = rows.iter().map(|r| r.user_id).collect();
        let online = self.hub.online_users(&ids);
        let rows = rows
            .into_iter()
            .map(|mut r| {
                r.online = online.contains(&r.user_id);
                r
            })
            .collect();
        Ok(Inbox {
            rows,
            total,
            total_unread: self.repo.total_admin_unread().await?,
        })
    }

    pub async fn total_unread(&self) -> Result<i64, ChatError> {
        Ok(self.repo.total_admin_unread().await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_rules() {
        assert!(matches!(clean_body("   \n "), Err(ChatError::Empty)));
        assert!(matches!(
            clean_body(&"x".repeat(2001)),
            Err(ChatError::TooLong)
        ));
        assert_eq!(clean_body("  hi\nthere ").unwrap(), "hi\nthere");
        // Characters, not bytes.
        assert!(clean_body(&"é".repeat(2000)).is_ok());
    }

    #[test]
    fn previews_are_one_short_line() {
        assert_eq!(make_preview("a\n  b\tc"), "a b c");
        let p = make_preview(&"word ".repeat(100));
        assert!(p.chars().count() <= PREVIEW_CHARS);
        assert!(p.ends_with('…'));
    }

    #[test]
    fn client_msg_id_rules() {
        assert!(clean_client_msg_id("").is_err());
        assert!(clean_client_msg_id(&"a".repeat(65)).is_err());
        assert!(clean_client_msg_id("a\nb").is_err());
        assert!(clean_client_msg_id("3f2b-uuid").is_ok());
    }
}
