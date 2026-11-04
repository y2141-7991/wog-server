-- ══════════════════════════════════════════════════════════════
-- Support chat: one conversation per user, answered by admins.
--
-- Idempotent (IF NOT EXISTS), so it is safe on a fresh database, on one
-- already at 0004, and when run twice.
--
--   support_conversations  one row per thread. A user has at most ONE open
--                          conversation (partial unique index); closing it and
--                          writing again reopens the same row.
--   support_messages       the messages. `client_msg_id` is the id the sender's
--                          client generated, so a resend after a reconnect is
--                          recognised and not stored twice.
--
-- The unread counters are denormalised on the conversation so the inbox is a
-- single indexed query; they are updated in the same transaction as the
-- message insert / mark-read.
-- ══════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS support_conversations (
    id                   UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id              UUID         NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    status               VARCHAR(10)  NOT NULL DEFAULT 'open',
    subject              VARCHAR(200),
    last_message_at      TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    last_message_preview VARCHAR(160) NOT NULL DEFAULT '',
    user_unread_count    INT          NOT NULL DEFAULT 0,
    admin_unread_count   INT          NOT NULL DEFAULT 0,
    assigned_admin_id    UUID         REFERENCES users(id) ON DELETE SET NULL,
    created_at           TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    closed_at            TIMESTAMPTZ,

    CONSTRAINT support_conversations_status_check CHECK (status IN ('open', 'closed')),
    CONSTRAINT support_conversations_unread_check
        CHECK (user_unread_count >= 0 AND admin_unread_count >= 0)
);

-- One OPEN conversation per user.
CREATE UNIQUE INDEX IF NOT EXISTS support_conversations_one_open_per_user
    ON support_conversations (user_id) WHERE status = 'open';

-- Admin inbox: filter by status, newest activity first.
CREATE INDEX IF NOT EXISTS support_conversations_inbox_idx
    ON support_conversations (status, last_message_at DESC);

-- "My conversation(s)" lookups.
CREATE INDEX IF NOT EXISTS support_conversations_user_idx
    ON support_conversations (user_id, created_at DESC);

CREATE TABLE IF NOT EXISTS support_messages (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    conversation_id UUID        NOT NULL REFERENCES support_conversations(id) ON DELETE CASCADE,
    sender_id       UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    sender_role     VARCHAR(10) NOT NULL,
    body            TEXT        NOT NULL,
    client_msg_id   VARCHAR(64),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    read_at         TIMESTAMPTZ,

    CONSTRAINT support_messages_role_check CHECK (sender_role IN ('user', 'admin')),
    CONSTRAINT support_messages_body_check CHECK (char_length(body) BETWEEN 1 AND 2000)
);

-- History pages and "messages after X" catch-up. `id` breaks created_at ties
-- so keyset pagination is stable.
CREATE INDEX IF NOT EXISTS support_messages_conversation_idx
    ON support_messages (conversation_id, created_at, id);

-- Idempotent sends: the same client_msg_id from the same sender is one message.
CREATE UNIQUE INDEX IF NOT EXISTS support_messages_client_msg_idx
    ON support_messages (sender_id, client_msg_id) WHERE client_msg_id IS NOT NULL;
