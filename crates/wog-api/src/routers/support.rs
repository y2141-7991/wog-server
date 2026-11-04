//! Support chat: the WebSocket endpoint and its REST companions.
//!
//! The WebSocket is authenticated by the same `token` cookie as every HTTP
//! route (browsers send it on the same-origin handshake and cannot set an
//! `Authorization` header), so no token ever appears in a URL. Because a
//! cookie is sent automatically, the handshake also checks `Origin` against an
//! allowlist (cross-site WebSocket hijacking). Non-browser clients may send an
//! `Authorization: Bearer` header instead and need no Origin.

use std::{
    collections::HashMap,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use axum::{
    Json,
    extract::{
        Path, Query, State,
        ws::{CloseFrame, Message, Utf8Bytes, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderMap, StatusCode, header::ORIGIN},
    response::{IntoResponse, Response},
};
use serde_json::json;
use tokio::time::{interval_at, sleep, timeout};
use uuid::Uuid;
use wog_config::support::dto::{
    ConversationResponse, InboxQuery, InboxResponse, MessagesQuery, MessagesResponse,
    UserConversationResponse,
};
use wog_infras::{
    DefaultConfig,
    models::User,
    services::support::{ChatError, INBOX_PER_PAGE, SupportServices},
    support::{Audience, ChatRole, ClientEvent, ConnId, ServerEvent, WindowLimiter},
};
use wog_middleware::{AdminClaims, AdminError, AppState, VerifiedUser, session_is_current};

use crate::errors::{RestApiError, RestApiResponseError};

/// Largest frame the server accepts. A 2000-character message is at most
/// ~8 KB of UTF-8, so this leaves room for the JSON around it. Larger frames
/// close the connection (the protocol layer refuses them before we see them).
pub const MAX_FRAME_BYTES: usize = 16 * 1024;
/// `send_message` budget per connection.
const SEND_LIMIT: (u32, Duration) = (10, Duration::from_secs(10));
/// Budget for every frame a connection sends (typing included).
const FRAME_LIMIT: (u32, Duration) = (60, Duration::from_secs(10));
/// Rate-limit hits in a row before the connection is closed.
const MAX_STRIKES: u32 = 20;
const PING_EVERY: Duration = Duration::from_secs(30);
const IDLE_TIMEOUT: Duration = Duration::from_secs(90);
const WRITE_TIMEOUT: Duration = Duration::from_secs(10);
const OWNER_CACHE_MAX: usize = 512;

impl From<ChatError> for RestApiError {
    fn from(e: ChatError) -> Self {
        match e {
            ChatError::Empty
            | ChatError::TooLong
            | ChatError::BadClientMsgId
            | ChatError::BadRequest(_) => RestApiError::BadRequest(e.to_string()),
            ChatError::NotFound => RestApiError::NotFound(e.to_string()),
            ChatError::Forbidden => RestApiError::Forbidden(e.to_string()),
            ChatError::Closed => RestApiError::Conflict(e.to_string()),
            ChatError::Db(db) => RestApiError::from(db),
        }
    }
}

// ═══════════════════════════ WebSocket ═══════════════════════════

/// `scheme://host[:port]`, lowercased, without path or trailing slash.
fn normalize_origin(value: &str) -> String {
    let value = value.trim().to_ascii_lowercase();
    match value.find("://") {
        Some(i) => {
            let rest = &value[i + 3..];
            let host = rest.split('/').next().unwrap_or(rest);
            format!("{}://{}", &value[..i], host)
        }
        None => value,
    }
}

/// Origins that may open the socket: the site itself plus `ALLOWED_ORIGINS`.
pub fn allowed_origins(cfg: &DefaultConfig) -> Vec<String> {
    [cfg.client_url.as_str(), cfg.rest_api_url.as_str()]
        .into_iter()
        .chain(cfg.allowed_origins.split(','))
        .map(str::trim)
        .filter(|o| !o.is_empty())
        .map(normalize_origin)
        .collect()
}

/// A browser always sends `Origin` on a WebSocket handshake, so a missing one
/// means a non-browser client (CLI, tests), which is not a CSWSH vector.
pub fn origin_allowed(cfg: &DefaultConfig, origin: Option<&str>) -> bool {
    match origin {
        None => true,
        Some(origin) => {
            let origin = normalize_origin(origin);
            allowed_origins(cfg).contains(&origin)
        }
    }
}

fn reject(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "error": true, "message": message }))).into_response()
}

/// `GET /api/v1/support/ws`: upgrade to the chat socket.
pub async fn support_ws(
    State(state): State<AppState>,
    headers: HeaderMap,
    auth: Result<VerifiedUser, AdminError>,
    ws: WebSocketUpgrade,
) -> Response {
    let origin = headers.get(ORIGIN).and_then(|v| v.to_str().ok());
    if !origin_allowed(&state.app_config.default_config, origin) {
        tracing::warn!(origin = ?origin, "Chat WebSocket refused: origin not allowed");
        return reject(StatusCode::FORBIDDEN, "Origin not allowed");
    }
    let VerifiedUser {
        user,
        token_exp,
        token_iat,
    } = match auth {
        Ok(v) => v,
        Err(e) => return e.into_response(),
    };
    ws.max_message_size(MAX_FRAME_BYTES)
        .max_frame_size(MAX_FRAME_BYTES)
        .on_upgrade(move |socket| run_connection(state, user, token_exp, token_iat, socket))
}

struct Connection {
    conn_id: ConnId,
    user: User,
    role: ChatRole,
    svc: SupportServices,
    state: AppState,
    send_limit: WindowLimiter,
    frame_limit: WindowLimiter,
    strikes: u32,
    /// When this connection's access token was issued (seconds).
    token_iat: usize,
    /// A user's own conversation id, once known.
    own_conversation: Option<Uuid>,
    /// conversation id → owner, so typing/read events skip the database.
    owners: HashMap<Uuid, Uuid>,
}

enum Flow {
    Continue,
    Close(u16, &'static str),
}

async fn write(socket: &mut WebSocket, message: Message) -> bool {
    matches!(
        timeout(WRITE_TIMEOUT, socket.send(message)).await,
        Ok(Ok(()))
    )
}

async fn close(socket: &mut WebSocket, code: u16, reason: &'static str) {
    let frame = CloseFrame {
        code,
        reason: Utf8Bytes::from_static(reason),
    };
    let _ = timeout(WRITE_TIMEOUT, socket.send(Message::Close(Some(frame)))).await;
}

fn seconds_until(exp: usize) -> Duration {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    Duration::from_secs((exp as u64).saturating_sub(now))
}

async fn run_connection(
    state: AppState,
    user: User,
    token_exp: usize,
    token_iat: usize,
    mut socket: WebSocket,
) {
    let svc = state.support_services.clone();
    let role = if user.is_admin() {
        ChatRole::Admin
    } else {
        ChatRole::User
    };

    let reg = match svc.hub().register(user.id, role) {
        Ok(reg) => reg,
        Err(e) => {
            let event = ServerEvent::error("too_many_connections", e.to_string());
            write(&mut socket, Message::Text(event.frame().to_string().into())).await;
            close(&mut socket, 1008, "too many connections").await;
            return;
        }
    };
    let conn_id = reg.conn_id;
    let mut rx = reg.rx;
    let kick = reg.kick;
    tracing::debug!(conn_id, user_id = %user.id, ?role, "Chat connection opened");

    if role == ChatRole::User && reg.became_online {
        svc.publish(
            Audience::Admins,
            &ServerEvent::Presence {
                user_id: user.id,
                online: true,
            },
        );
    }

    let own_conversation = match role {
        ChatRole::User => svc.own_conversation_id(user.id).await.ok().flatten(),
        ChatRole::Admin => None,
    };
    svc.publish(
        Audience::Connection(conn_id),
        &ServerEvent::Ready {
            user_id: user.id,
            role,
            conversation_id: own_conversation,
        },
    );

    let mut conn = Connection {
        conn_id,
        user,
        role,
        svc: svc.clone(),
        state,
        send_limit: WindowLimiter::new(SEND_LIMIT.0, SEND_LIMIT.1),
        frame_limit: WindowLimiter::new(FRAME_LIMIT.0, FRAME_LIMIT.1),
        strikes: 0,
        token_iat,
        own_conversation,
        owners: HashMap::new(),
    };

    let mut ping = interval_at(tokio::time::Instant::now() + PING_EVERY, PING_EVERY);
    let mut last_seen = Instant::now();
    let session_end = sleep(seconds_until(token_exp));
    tokio::pin!(session_end);

    loop {
        tokio::select! {
            biased;
            _ = kick.notified() => {
                close(&mut socket, 1013, "too slow").await;
                break;
            }
            _ = &mut session_end => {
                close(&mut socket, 1008, "session expired").await;
                break;
            }
            _ = ping.tick() => {
                if last_seen.elapsed() > IDLE_TIMEOUT {
                    close(&mut socket, 1001, "idle timeout").await;
                    break;
                }
                if !conn.session_still_valid().await {
                    close(&mut socket, 1008, "session no longer valid").await;
                    break;
                }
                if !write(&mut socket, Message::Ping(Vec::new().into())).await {
                    break;
                }
            }
            outgoing = rx.recv() => match outgoing {
                Some(frame) => {
                    if !write(&mut socket, Message::Text(frame.to_string().into())).await {
                        break;
                    }
                }
                None => break,
            },
            incoming = socket.recv() => match incoming {
                None | Some(Err(_)) => break,
                Some(Ok(message)) => {
                    last_seen = Instant::now();
                    match message {
                        Message::Text(text) => {
                            if let Flow::Close(code, reason) = conn.handle_text(text.as_str()).await {
                                // Let the client read why before the close frame.
                                while let Ok(frame) = rx.try_recv() {
                                    if !write(&mut socket, Message::Text(frame.to_string().into())).await {
                                        break;
                                    }
                                }
                                close(&mut socket, code, reason).await;
                                break;
                            }
                        }
                        Message::Binary(_) => conn.reply(ServerEvent::error(
                            "bad_request",
                            "Binary frames are not supported",
                        )),
                        Message::Ping(_) | Message::Pong(_) => {}
                        Message::Close(_) => break,
                    }
                }
            },
        }
    }

    if let Some(gone) = svc.hub().unregister(conn_id)
        && gone.role == ChatRole::User
        && gone.went_offline
    {
        svc.publish(
            Audience::Admins,
            &ServerEvent::Presence {
                user_id: gone.user_id,
                online: false,
            },
        );
    }
    tracing::debug!(conn_id, "Chat connection closed");
}

impl Connection {
    fn reply(&self, event: ServerEvent) {
        self.svc.publish(Audience::Connection(self.conn_id), &event);
    }

    fn reply_error(&self, err: &ChatError, client_msg_id: Option<String>) {
        self.reply(ServerEvent::Error {
            code: err.code().to_string(),
            message: err.public_message(),
            client_msg_id,
        });
    }

    /// The account is read from the database, never trusted from the token or
    /// from when the socket opened: a demoted admin, a deleted account or a
    /// password reset (which voids earlier sessions) cuts the connection off.
    async fn session_still_valid(&self) -> bool {
        self.state
            .user_services
            .get_user(self.user.id)
            .await
            .is_ok_and(|u| {
                session_is_current(&u, self.token_iat)
                    && (self.role != ChatRole::Admin || (u.is_admin() && !u.must_change_password))
            })
    }

    async fn handle_text(&mut self, text: &str) -> Flow {
        if !self.frame_limit.allow() {
            self.strikes += 1;
            self.reply(ServerEvent::error(
                "rate_limited",
                "You are sending too fast; slow down",
            ));
            return if self.strikes >= MAX_STRIKES {
                Flow::Close(1008, "rate limited")
            } else {
                Flow::Continue
            };
        }
        self.strikes = 0;

        let event: ClientEvent = match serde_json::from_str(text) {
            Ok(event) => event,
            Err(_) => {
                self.reply(ServerEvent::error("bad_request", "Malformed message"));
                return Flow::Continue;
            }
        };

        if event.is_admin_only() && self.role != ChatRole::Admin {
            self.reply(ServerEvent::error("forbidden", "Admins only"));
            return Flow::Continue;
        }
        // Everything an admin does is re-authorised against the database.
        if self.role == ChatRole::Admin
            && !matches!(event, ClientEvent::Ping)
            && !self.session_still_valid().await
        {
            self.reply(ServerEvent::error("forbidden", "Admin access required"));
            return Flow::Close(1008, "no longer an admin");
        }

        match event {
            ClientEvent::Ping => self.reply(ServerEvent::Pong),
            ClientEvent::SendMessage {
                conversation_id,
                body,
                client_msg_id,
            } => {
                self.send_message(conversation_id, body, client_msg_id)
                    .await
            }
            ClientEvent::Typing {
                conversation_id,
                is_typing,
            } => {
                if let Some((conversation, owner)) = self.resolve(conversation_id).await {
                    self.svc
                        .typing(self.role, self.user.id, conversation, owner, is_typing);
                }
            }
            ClientEvent::MarkRead { conversation_id } => {
                if let Some((conversation, _)) = self.resolve(conversation_id).await
                    && let Err(e) = self
                        .svc
                        .mark_read(self.role, self.user.id, conversation)
                        .await
                {
                    self.reply_error(&e, None);
                }
            }
            ClientEvent::JoinConversation { conversation_id } => {
                match self
                    .svc
                    .authorize(self.role, self.user.id, conversation_id)
                    .await
                {
                    Ok(_) => self.svc.hub().join(self.conn_id, conversation_id),
                    Err(e) => self.reply_error(&e, None),
                }
            }
            ClientEvent::LeaveConversation { conversation_id } => {
                self.svc.hub().leave(self.conn_id, conversation_id);
            }
            ClientEvent::CloseConversation { conversation_id } => {
                if let Err(e) = self.svc.set_open(conversation_id, false).await {
                    self.reply_error(&e, None);
                }
            }
            ClientEvent::ReopenConversation { conversation_id } => {
                if let Err(e) = self.svc.set_open(conversation_id, true).await {
                    self.reply_error(&e, None);
                }
            }
            ClientEvent::AssignToMe { conversation_id } => {
                if let Err(e) = self.svc.assign(conversation_id, self.user.id).await {
                    self.reply_error(&e, None);
                }
            }
        }
        Flow::Continue
    }

    /// The conversation an event is about and who owns it, or `None` after
    /// answering with an error (or, for a user with no conversation yet,
    /// silently: there is nothing to type into or read).
    async fn resolve(&mut self, requested: Option<Uuid>) -> Option<(Uuid, Uuid)> {
        match self.role {
            ChatRole::User => {
                let conversation = match (requested, self.own_conversation) {
                    (Some(id), _) => id,
                    (None, Some(id)) => id,
                    (None, None) => {
                        let found = self.svc.own_conversation_id(self.user.id).await;
                        match found {
                            Ok(Some(id)) => {
                                self.own_conversation = Some(id);
                                id
                            }
                            Ok(None) => return None,
                            Err(e) => {
                                self.reply_error(&e, None);
                                return None;
                            }
                        }
                    }
                };
                if Some(conversation) == self.own_conversation {
                    return Some((conversation, self.user.id));
                }
                // Someone else's id, or one we have not seen: ask the database.
                match self
                    .svc
                    .authorize(self.role, self.user.id, conversation)
                    .await
                {
                    Ok(owner) => {
                        self.own_conversation = Some(conversation);
                        Some((conversation, owner))
                    }
                    Err(e) => {
                        self.reply_error(&e, None);
                        None
                    }
                }
            }
            ChatRole::Admin => {
                let Some(conversation) = requested else {
                    self.reply(ServerEvent::error(
                        "bad_request",
                        "conversation_id is required",
                    ));
                    return None;
                };
                if let Some(owner) = self.owners.get(&conversation) {
                    return Some((conversation, *owner));
                }
                match self
                    .svc
                    .authorize(self.role, self.user.id, conversation)
                    .await
                {
                    Ok(owner) => {
                        if self.owners.len() >= OWNER_CACHE_MAX {
                            self.owners.clear();
                        }
                        self.owners.insert(conversation, owner);
                        Some((conversation, owner))
                    }
                    Err(e) => {
                        self.reply_error(&e, None);
                        None
                    }
                }
            }
        }
    }

    async fn send_message(
        &mut self,
        conversation_id: Option<Uuid>,
        body: String,
        client_msg_id: String,
    ) {
        if !self.send_limit.allow() {
            self.reply(ServerEvent::Error {
                code: "rate_limited".into(),
                message: "You are sending messages too fast".into(),
                client_msg_id: Some(client_msg_id),
            });
            return;
        }
        let result = match self.role {
            ChatRole::User => {
                // A user can only ever write to their own conversation.
                if let Some(id) = conversation_id
                    && let Err(e) = self.svc.authorize(self.role, self.user.id, id).await
                {
                    self.reply_error(&e, Some(client_msg_id));
                    return;
                }
                self.svc
                    .post_user_message(self.user.id, &body, &client_msg_id)
                    .await
            }
            ChatRole::Admin => {
                let Some(id) = conversation_id else {
                    self.reply(ServerEvent::Error {
                        code: "bad_request".into(),
                        message: "conversation_id is required".into(),
                        client_msg_id: Some(client_msg_id),
                    });
                    return;
                };
                self.svc
                    .post_admin_message(self.user.id, id, &body, &client_msg_id)
                    .await
            }
        };
        match result {
            Ok(posted) => {
                let conversation = posted.conversation.id;
                match self.role {
                    ChatRole::User => self.own_conversation = Some(conversation),
                    ChatRole::Admin => {
                        // Replying means following the conversation.
                        self.svc.hub().join(self.conn_id, conversation);
                        self.owners
                            .insert(conversation, posted.conversation.user_id);
                    }
                }
                self.reply(ServerEvent::Ack {
                    client_msg_id,
                    message_id: posted.message.id,
                    conversation_id: conversation,
                    created_at: posted.message.created_at,
                    duplicate: posted.duplicate,
                });
            }
            Err(e) => self.reply_error(&e, Some(client_msg_id)),
        }
    }
}

// ═══════════════════════════ REST ═══════════════════════════

#[utoipa::path(
    get,
    path = "/api/v1/support/conversation",
    tag = "Support",
    params(MessagesQuery),
    responses(
        (status = 200, description = "Your support conversation (open, else the latest closed) and a page of its messages; `conversation` is null before your first message", body = UserConversationResponse),
        (status = 401, description = "Unauthorized", body = RestApiResponseError),
        (status = 403, description = "Verify your email / change your password first", body = RestApiResponseError),
    ),
    security(("bearer_auth" = []))
)]
#[axum::debug_handler]
pub async fn get_my_conversation(
    State(state): State<AppState>,
    VerifiedUser { user, .. }: VerifiedUser,
    Query(query): Query<MessagesQuery>,
) -> Result<Json<UserConversationResponse>, RestApiError> {
    let (conversation, page) = state
        .support_services
        .user_view(user.id, query.before, query.after, query.limit)
        .await?;
    Ok(Json(UserConversationResponse {
        conversation: conversation.map(Into::into),
        messages: page.messages.into_iter().map(Into::into).collect(),
        has_more: page.has_more,
    }))
}

#[utoipa::path(
    get,
    path = "/api/v1/support/conversations",
    tag = "Support",
    params(InboxQuery),
    responses(
        (status = 200, description = "Admin inbox, newest activity first", body = InboxResponse),
        (status = 401, description = "Unauthorized", body = RestApiResponseError),
        (status = 403, description = "Not an admin", body = RestApiResponseError),
    ),
    security(("bearer_auth" = []))
)]
#[axum::debug_handler]
pub async fn list_conversations(
    State(state): State<AppState>,
    AdminClaims(_claims): AdminClaims,
    Query(query): Query<InboxQuery>,
) -> Result<Json<InboxResponse>, RestApiError> {
    let page = query.page.unwrap_or(0).max(0);
    let inbox = state
        .support_services
        .inbox(query.status.as_deref(), query.q.as_deref(), page)
        .await?;
    Ok(Json(InboxResponse {
        conversations: inbox.rows.into_iter().map(Into::into).collect(),
        page,
        per_page: INBOX_PER_PAGE,
        total: inbox.total,
        total_unread: inbox.total_unread,
    }))
}

#[utoipa::path(
    get,
    path = "/api/v1/support/conversations/{id}",
    tag = "Support",
    params(("id" = Uuid, Path, description = "Conversation ID")),
    responses(
        (status = 200, description = "One conversation", body = ConversationResponse),
        (status = 401, description = "Unauthorized", body = RestApiResponseError),
        (status = 403, description = "Not an admin", body = RestApiResponseError),
        (status = 404, description = "Not found", body = RestApiResponseError),
    ),
    security(("bearer_auth" = []))
)]
#[axum::debug_handler]
pub async fn get_conversation(
    State(state): State<AppState>,
    AdminClaims(_claims): AdminClaims,
    Path(id): Path<Uuid>,
) -> Result<Json<ConversationResponse>, RestApiError> {
    Ok(Json(
        state
            .support_services
            .conversation_for_admin(id)
            .await?
            .into(),
    ))
}

#[utoipa::path(
    get,
    path = "/api/v1/support/conversations/{id}/messages",
    tag = "Support",
    params(("id" = Uuid, Path, description = "Conversation ID"), MessagesQuery),
    responses(
        (status = 200, description = "A page of messages, oldest first", body = MessagesResponse),
        (status = 400, description = "Bad cursor", body = RestApiResponseError),
        (status = 401, description = "Unauthorized", body = RestApiResponseError),
        (status = 404, description = "Not found, or not yours", body = RestApiResponseError),
    ),
    security(("bearer_auth" = []))
)]
#[axum::debug_handler]
pub async fn list_messages(
    State(state): State<AppState>,
    VerifiedUser { user, .. }: VerifiedUser,
    Path(id): Path<Uuid>,
    Query(query): Query<MessagesQuery>,
) -> Result<Json<MessagesResponse>, RestApiError> {
    let role = if user.is_admin() {
        ChatRole::Admin
    } else {
        ChatRole::User
    };
    let page = state
        .support_services
        .messages(role, user.id, id, query.before, query.after, query.limit)
        .await?;
    Ok(Json(MessagesResponse {
        messages: page.messages.into_iter().map(Into::into).collect(),
        has_more: page.has_more,
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/support/conversations/{id}/close",
    tag = "Support",
    params(("id" = Uuid, Path, description = "Conversation ID")),
    responses(
        (status = 200, description = "The conversation, now closed", body = ConversationResponse),
        (status = 401, description = "Unauthorized", body = RestApiResponseError),
        (status = 403, description = "Not an admin", body = RestApiResponseError),
        (status = 404, description = "Not found", body = RestApiResponseError),
    ),
    security(("bearer_auth" = []))
)]
#[axum::debug_handler]
pub async fn close_conversation(
    State(state): State<AppState>,
    AdminClaims(_claims): AdminClaims,
    Path(id): Path<Uuid>,
) -> Result<Json<ConversationResponse>, RestApiError> {
    Ok(Json(
        state.support_services.set_open(id, false).await?.into(),
    ))
}

#[utoipa::path(
    post,
    path = "/api/v1/support/conversations/{id}/reopen",
    tag = "Support",
    params(("id" = Uuid, Path, description = "Conversation ID")),
    responses(
        (status = 200, description = "The conversation, now open", body = ConversationResponse),
        (status = 401, description = "Unauthorized", body = RestApiResponseError),
        (status = 403, description = "Not an admin", body = RestApiResponseError),
        (status = 404, description = "Not found", body = RestApiResponseError),
    ),
    security(("bearer_auth" = []))
)]
#[axum::debug_handler]
pub async fn reopen_conversation(
    State(state): State<AppState>,
    AdminClaims(_claims): AdminClaims,
    Path(id): Path<Uuid>,
) -> Result<Json<ConversationResponse>, RestApiError> {
    Ok(Json(
        state.support_services.set_open(id, true).await?.into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origins_are_normalised() {
        assert_eq!(
            normalize_origin("HTTPS://WoGHub.site/"),
            "https://woghub.site"
        );
        assert_eq!(
            normalize_origin("http://localhost:5173/some/path"),
            "http://localhost:5173"
        );
    }
}
