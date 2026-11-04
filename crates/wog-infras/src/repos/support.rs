use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::{
    errors::DatabaseError,
    models::{PostedMessage, SupportConversationRow, SupportMessage},
    repos::SupportRepository,
};

/// A conversation joined with its user and the user's running plan.
const CONVERSATION_SELECT: &str = r#"
    SELECT c.id, c.user_id, c.status, c.subject, c.last_message_at, c.last_message_preview,
           c.user_unread_count, c.admin_unread_count, c.assigned_admin_id,
           c.created_at, c.closed_at,
           u.username, u.email, u.avatar_url,
           (SELECT p.name
              FROM subscriptions s JOIN plans p ON p.id = s.plan_id
             WHERE s.user_id = c.user_id AND s.status = 'active'
               AND s.current_period_end > NOW()
             ORDER BY p.sort_order DESC LIMIT 1) AS plan_name
    FROM support_conversations c
    JOIN users u ON u.id = c.user_id
"#;

const MESSAGE_COLUMNS: &str =
    "id, conversation_id, sender_id, sender_role, body, client_msg_id, created_at, read_at";

#[derive(Clone)]
pub struct PgSupportRepo {
    pub pg_pool: PgPool,
}

impl PgSupportRepo {
    pub fn new(pg_pool: PgPool) -> Self {
        Self { pg_pool }
    }

    async fn conversation(
        &self,
        id: Uuid,
    ) -> Result<Option<SupportConversationRow>, DatabaseError> {
        Ok(sqlx::query_as::<_, SupportConversationRow>(&format!(
            "{CONVERSATION_SELECT} WHERE c.id = $1"
        ))
        .bind(id)
        .fetch_optional(&self.pg_pool)
        .await?)
    }
}

/// Serialise every post to one user's conversation, so `created_at` order is
/// commit order and the "messages after X" catch-up can never skip a message
/// that commits late.
async fn lock_user(tx: &mut Transaction<'_, Postgres>, user_id: Uuid) -> Result<(), DatabaseError> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1::text, 7002))")
        .bind(user_id.to_string())
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn find_duplicate(
    tx: &mut Transaction<'_, Postgres>,
    sender_id: Uuid,
    client_msg_id: &str,
) -> Result<Option<SupportMessage>, DatabaseError> {
    Ok(sqlx::query_as::<_, SupportMessage>(&format!(
        "SELECT {MESSAGE_COLUMNS} FROM support_messages WHERE sender_id = $1 AND client_msg_id = $2"
    ))
    .bind(sender_id)
    .bind(client_msg_id)
    .fetch_optional(&mut **tx)
    .await?)
}

#[allow(clippy::too_many_arguments)]
async fn insert_message(
    tx: &mut Transaction<'_, Postgres>,
    conversation_id: Uuid,
    sender_id: Uuid,
    sender_role: &str,
    body: &str,
    preview: &str,
    client_msg_id: &str,
    unread_column_for_recipient: &str,
) -> Result<SupportMessage, DatabaseError> {
    let message = sqlx::query_as::<_, SupportMessage>(&format!(
        r#"
        INSERT INTO support_messages
            (conversation_id, sender_id, sender_role, body, client_msg_id, created_at)
        VALUES ($1, $2, $3, $4, $5, clock_timestamp())
        RETURNING {MESSAGE_COLUMNS}
        "#
    ))
    .bind(conversation_id)
    .bind(sender_id)
    .bind(sender_role)
    .bind(body)
    .bind(client_msg_id)
    .fetch_one(&mut **tx)
    .await?;

    // The column name comes from a fixed internal set, never from input.
    sqlx::query(&format!(
        r#"
        UPDATE support_conversations
        SET last_message_at = $2, last_message_preview = $3,
            {unread_column_for_recipient} = {unread_column_for_recipient} + 1,
            assigned_admin_id = CASE WHEN $5 THEN COALESCE(assigned_admin_id, $4)
                                     ELSE assigned_admin_id END
        WHERE id = $1
        "#
    ))
    .bind(conversation_id)
    .bind(message.created_at)
    .bind(preview)
    .bind(sender_id)
    .bind(sender_role == "admin")
    .execute(&mut **tx)
    .await?;
    Ok(message)
}

#[async_trait]
impl SupportRepository for PgSupportRepo {
    async fn find_conversation(
        &self,
        id: Uuid,
    ) -> Result<Option<SupportConversationRow>, DatabaseError> {
        self.conversation(id).await
    }

    async fn find_user_conversation(
        &self,
        user_id: Uuid,
    ) -> Result<Option<SupportConversationRow>, DatabaseError> {
        Ok(sqlx::query_as::<_, SupportConversationRow>(&format!(
            r#"{CONVERSATION_SELECT}
                   WHERE c.user_id = $1
                   ORDER BY (c.status = 'open') DESC, c.last_message_at DESC
                   LIMIT 1"#
        ))
        .bind(user_id)
        .fetch_optional(&self.pg_pool)
        .await?)
    }

    async fn post_user_message(
        &self,
        user_id: Uuid,
        body: &str,
        preview: &str,
        client_msg_id: &str,
    ) -> Result<PostedMessage, DatabaseError> {
        let mut tx = self.pg_pool.begin().await?;
        lock_user(&mut tx, user_id).await?;

        if let Some(message) = find_duplicate(&mut tx, user_id, client_msg_id).await? {
            tx.commit().await?;
            let conversation = self
                .conversation(message.conversation_id)
                .await?
                .ok_or_else(|| DatabaseError::ValueNotFound("Conversation not found".into()))?;
            return Ok(PostedMessage {
                message,
                conversation,
                duplicate: true,
            });
        }

        // Their open conversation; else reopen the latest closed one (one
        // continuous thread per user); else start the first.
        let open: Option<(Uuid,)> = sqlx::query_as(
            "SELECT id FROM support_conversations WHERE user_id = $1 AND status = 'open' FOR UPDATE",
        )
        .bind(user_id)
        .fetch_optional(&mut *tx)
        .await?;
        let conversation_id = match open {
            Some((id,)) => id,
            None => {
                let closed: Option<(Uuid,)> = sqlx::query_as(
                    r#"SELECT id FROM support_conversations
                       WHERE user_id = $1 AND status = 'closed'
                       ORDER BY last_message_at DESC LIMIT 1 FOR UPDATE"#,
                )
                .bind(user_id)
                .fetch_optional(&mut *tx)
                .await?;
                match closed {
                    Some((id,)) => {
                        sqlx::query(
                            "UPDATE support_conversations SET status = 'open', closed_at = NULL WHERE id = $1",
                        )
                        .bind(id)
                        .execute(&mut *tx)
                        .await?;
                        id
                    }
                    None => {
                        let (id,): (Uuid,) = sqlx::query_as(
                            "INSERT INTO support_conversations (user_id) VALUES ($1) RETURNING id",
                        )
                        .bind(user_id)
                        .fetch_one(&mut *tx)
                        .await?;
                        id
                    }
                }
            }
        };

        let message = insert_message(
            &mut tx,
            conversation_id,
            user_id,
            "user",
            body,
            preview,
            client_msg_id,
            "admin_unread_count",
        )
        .await?;
        tx.commit().await?;

        let conversation = self
            .conversation(conversation_id)
            .await?
            .ok_or_else(|| DatabaseError::ValueNotFound("Conversation not found".into()))?;
        Ok(PostedMessage {
            message,
            conversation,
            duplicate: false,
        })
    }

    async fn post_admin_message(
        &self,
        admin_id: Uuid,
        conversation_id: Uuid,
        body: &str,
        preview: &str,
        client_msg_id: &str,
    ) -> Result<PostedMessage, DatabaseError> {
        let owner: Option<(Uuid,)> =
            sqlx::query_as("SELECT user_id FROM support_conversations WHERE id = $1")
                .bind(conversation_id)
                .fetch_optional(&self.pg_pool)
                .await?;
        let (owner_id,) =
            owner.ok_or_else(|| DatabaseError::ValueNotFound("Conversation not found".into()))?;

        let mut tx = self.pg_pool.begin().await?;
        lock_user(&mut tx, owner_id).await?;

        if let Some(message) = find_duplicate(&mut tx, admin_id, client_msg_id).await? {
            tx.commit().await?;
            let conversation = self
                .conversation(message.conversation_id)
                .await?
                .ok_or_else(|| DatabaseError::ValueNotFound("Conversation not found".into()))?;
            return Ok(PostedMessage {
                message,
                conversation,
                duplicate: true,
            });
        }

        let status: Option<(String,)> =
            sqlx::query_as("SELECT status FROM support_conversations WHERE id = $1 FOR UPDATE")
                .bind(conversation_id)
                .fetch_optional(&mut *tx)
                .await?;
        match status {
            None => {
                return Err(DatabaseError::ValueNotFound(
                    "Conversation not found".into(),
                ));
            }
            Some((s,)) if s != "open" => {
                return Err(DatabaseError::ValidationError(
                    "This conversation is closed; reopen it to reply".into(),
                ));
            }
            Some(_) => {}
        }

        let message = insert_message(
            &mut tx,
            conversation_id,
            admin_id,
            "admin",
            body,
            preview,
            client_msg_id,
            "user_unread_count",
        )
        .await?;
        tx.commit().await?;

        let conversation = self
            .conversation(conversation_id)
            .await?
            .ok_or_else(|| DatabaseError::ValueNotFound("Conversation not found".into()))?;
        Ok(PostedMessage {
            message,
            conversation,
            duplicate: false,
        })
    }

    async fn mark_read(
        &self,
        conversation_id: Uuid,
        reader_is_admin: bool,
    ) -> Result<Option<(SupportConversationRow, DateTime<Utc>)>, DatabaseError> {
        // The reader clears what the *other* side sent.
        let (sender_role, unread_column) = if reader_is_admin {
            ("user", "admin_unread_count")
        } else {
            ("admin", "user_unread_count")
        };
        let mut tx = self.pg_pool.begin().await?;
        let marked = sqlx::query(
            r#"UPDATE support_messages SET read_at = NOW()
               WHERE conversation_id = $1 AND sender_role = $2 AND read_at IS NULL"#,
        )
        .bind(conversation_id)
        .bind(sender_role)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        let reset = sqlx::query(&format!(
            "UPDATE support_conversations SET {unread_column} = 0 WHERE id = $1 AND {unread_column} <> 0"
        ))
        .bind(conversation_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        tx.commit().await?;

        if marked == 0 && reset == 0 {
            return Ok(None);
        }
        Ok(self
            .conversation(conversation_id)
            .await?
            .map(|c| (c, Utc::now())))
    }

    async fn set_status(
        &self,
        conversation_id: Uuid,
        open: bool,
    ) -> Result<(SupportConversationRow, bool), DatabaseError> {
        let result = if open {
            sqlx::query(
                "UPDATE support_conversations SET status = 'open', closed_at = NULL WHERE id = $1 AND status = 'closed'",
            )
        } else {
            sqlx::query(
                "UPDATE support_conversations SET status = 'closed', closed_at = NOW() WHERE id = $1 AND status = 'open'",
            )
        }
        .bind(conversation_id)
        .execute(&self.pg_pool)
        .await?;
        let conversation = self
            .conversation(conversation_id)
            .await?
            .ok_or_else(|| DatabaseError::ValueNotFound("Conversation not found".into()))?;
        Ok((conversation, result.rows_affected() > 0))
    }

    async fn assign(
        &self,
        conversation_id: Uuid,
        admin_id: Uuid,
    ) -> Result<SupportConversationRow, DatabaseError> {
        sqlx::query("UPDATE support_conversations SET assigned_admin_id = $2 WHERE id = $1")
            .bind(conversation_id)
            .bind(admin_id)
            .execute(&self.pg_pool)
            .await?;
        self.conversation(conversation_id)
            .await?
            .ok_or_else(|| DatabaseError::ValueNotFound("Conversation not found".into()))
    }

    async fn list_messages(
        &self,
        conversation_id: Uuid,
        before: Option<Uuid>,
        after: Option<Uuid>,
        limit: i64,
    ) -> Result<(Vec<SupportMessage>, bool), DatabaseError> {
        // Keyset pagination on (created_at, id). The cursor must belong to
        // this conversation, otherwise it is rejected.
        let cursor = before.or(after);
        let cursor_row: Option<(DateTime<Utc>, Uuid)> = match cursor {
            Some(id) => {
                let row = sqlx::query_as(
                    "SELECT created_at, id FROM support_messages WHERE id = $1 AND conversation_id = $2",
                )
                .bind(id)
                .bind(conversation_id)
                .fetch_optional(&self.pg_pool)
                .await?;
                Some(row.ok_or_else(|| DatabaseError::ValidationError("Unknown cursor".into()))?)
            }
            None => None,
        };

        let mut rows = if after.is_some() {
            let (at, id) = cursor_row.expect("checked above");
            sqlx::query_as::<_, SupportMessage>(&format!(
                r#"SELECT {MESSAGE_COLUMNS} FROM support_messages
                   WHERE conversation_id = $1 AND (created_at, id) > ($2, $3)
                   ORDER BY created_at ASC, id ASC LIMIT $4"#
            ))
            .bind(conversation_id)
            .bind(at)
            .bind(id)
            .bind(limit + 1)
            .fetch_all(&self.pg_pool)
            .await?
        } else {
            let (at, id) = match cursor_row {
                Some((at, id)) => (Some(at), Some(id)),
                None => (None, None),
            };
            let mut newest_first = sqlx::query_as::<_, SupportMessage>(&format!(
                r#"SELECT {MESSAGE_COLUMNS} FROM support_messages
                   WHERE conversation_id = $1
                     AND ($2::timestamptz IS NULL OR (created_at, id) < ($2, $3))
                   ORDER BY created_at DESC, id DESC LIMIT $4"#
            ))
            .bind(conversation_id)
            .bind(at)
            .bind(id)
            .bind(limit + 1)
            .fetch_all(&self.pg_pool)
            .await?;
            newest_first.reverse();
            newest_first
        };

        let has_more = rows.len() as i64 > limit;
        if has_more {
            if after.is_some() {
                rows.truncate(limit as usize);
            } else {
                // Drop the oldest extra row of the (now ascending) page.
                rows.remove(0);
            }
        }
        Ok((rows, has_more))
    }

    async fn list_conversations(
        &self,
        status: Option<&str>,
        search: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<SupportConversationRow>, DatabaseError> {
        Ok(sqlx::query_as::<_, SupportConversationRow>(&format!(
            r#"SELECT page.*, COUNT(*) OVER () AS total FROM (
                   {CONVERSATION_SELECT}
                   WHERE ($1::text IS NULL OR c.status = $1)
                     AND ($2::text IS NULL
                          OR u.username ILIKE $2 ESCAPE '\'
                          OR u.email ILIKE $2 ESCAPE '\')
               ) page
               ORDER BY page.last_message_at DESC, page.id
               LIMIT $3 OFFSET $4"#
        ))
        .bind(status)
        .bind(search.map(|q| {
            let escaped = q
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_");
            format!("%{escaped}%")
        }))
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pg_pool)
        .await?)
    }

    async fn total_admin_unread(&self) -> Result<i64, DatabaseError> {
        let (total,): (i64,) = sqlx::query_as(
            "SELECT COALESCE(SUM(admin_unread_count), 0)::bigint FROM support_conversations WHERE status = 'open'",
        )
        .fetch_one(&self.pg_pool)
        .await?;
        Ok(total)
    }
}
