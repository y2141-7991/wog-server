CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- ══════════════════════════════════════════════════════════════
-- Users
-- ══════════════════════════════════════════════════════════════
CREATE TABLE IF NOT EXISTS users (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    username        VARCHAR(50)  NOT NULL UNIQUE,
    email           VARCHAR(255) NOT NULL UNIQUE,
    password        VARCHAR(255),
    avatar_url      TEXT,
    is_online       BOOLEAN      NOT NULL DEFAULT FALSE,
    email_verified  BOOLEAN      NOT NULL DEFAULT FALSE,
    oauth_provider  VARCHAR(20),
    oauth_sub       VARCHAR(255),
    role            VARCHAR(20)  NOT NULL DEFAULT 'user',
    must_change_password BOOLEAN NOT NULL DEFAULT FALSE,
    -- Set by a password reset. Access and refresh tokens issued before it are
    -- rejected, which signs the account out everywhere. NULL means no cutoff.
    password_changed_at  TIMESTAMPTZ,
    created_at      TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ  NOT NULL DEFAULT NOW(),

    CONSTRAINT users_role_check CHECK (role IN ('user', 'admin'))
);

-- ══════════════════════════════════════════════════════════════
-- Email verification tokens
-- ══════════════════════════════════════════════════════════════
CREATE TABLE IF NOT EXISTS email_verification_tokens (
    id          UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id     UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token       TEXT NOT NULL UNIQUE,
    expires_at  TIMESTAMPTZ NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ══════════════════════════════════════════════════════════════
-- OAuth PKCE/CSRF Verifiers (short-lived, cleaned up after use)
-- ══════════════════════════════════════════════════════════════
CREATE TABLE IF NOT EXISTS oauth_verifiers (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    csrf_token      TEXT NOT NULL UNIQUE,
    pkce_verifier   TEXT NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);


CREATE INDEX IF NOT EXISTS idx_users_email       ON users(email);
CREATE INDEX IF NOT EXISTS idx_users_username    ON users(username);
CREATE UNIQUE INDEX IF NOT EXISTS idx_users_oauth
    ON users(oauth_provider, oauth_sub)
    WHERE oauth_provider IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_email_verification_tokens_user  ON email_verification_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_email_verification_tokens_token ON email_verification_tokens(token);

-- ══════════════════════════════════════════════════════════════
-- API marketplace: catalog, API keys, usage metering
-- ══════════════════════════════════════════════════════════════

-- Catalog of APIs sold through the gateway. `params` documents (and
-- whitelists) the query parameters the gateway forwards upstream.
CREATE TABLE IF NOT EXISTS apis (
    id                     UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    slug                   VARCHAR(60)  NOT NULL UNIQUE,
    name                   VARCHAR(120) NOT NULL,
    description            TEXT         NOT NULL,
    provider               VARCHAR(60)  NOT NULL,
    category               VARCHAR(40)  NOT NULL DEFAULT 'Data',
    gateway_path           VARCHAR(200) NOT NULL UNIQUE,
    -- gRPC method on the crawler the gateway forwards to.
    upstream_method        VARCHAR(200) NOT NULL,
    params                 JSONB        NOT NULL DEFAULT '[]',
    example_response       JSONB,
    rate_limit_per_minute  INT          NOT NULL DEFAULT 30,
    monthly_quota          INT          NOT NULL DEFAULT 1000,
    enabled                BOOLEAN      NOT NULL DEFAULT TRUE,
    created_at             TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at             TIMESTAMPTZ  NOT NULL DEFAULT NOW(),

    CONSTRAINT positive_rate_limit CHECK (rate_limit_per_minute > 0),
    CONSTRAINT positive_monthly_quota CHECK (monthly_quota > 0)
);

-- Only the SHA-256 of a key is stored; the plaintext is shown once at creation.
CREATE TABLE IF NOT EXISTS api_keys (
    id            UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id       UUID         NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name          VARCHAR(100) NOT NULL,
    key_prefix    VARCHAR(16)  NOT NULL,
    key_hash      CHAR(64)     NOT NULL UNIQUE,
    created_at    TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    last_used_at  TIMESTAMPTZ,
    revoked_at    TIMESTAMPTZ
);

-- One row per request forwarded upstream; drives quotas and the dashboard.
CREATE TABLE IF NOT EXISTS api_usage (
    id          UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    api_key_id  UUID        NOT NULL REFERENCES api_keys(id) ON DELETE CASCADE,
    user_id     UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    api_id      UUID        NOT NULL REFERENCES apis(id) ON DELETE CASCADE,
    status_code SMALLINT    NOT NULL,
    latency_ms  INT         NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Every admin action that changes something, kept after the people involved
-- are deleted (SET NULL), so the trail survives account removal.
CREATE TABLE IF NOT EXISTS admin_audit_log (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    actor_id        UUID REFERENCES users(id) ON DELETE SET NULL,
    target_user_id  UUID REFERENCES users(id) ON DELETE SET NULL,
    action          VARCHAR(50) NOT NULL,
    old_value       TEXT,
    new_value       TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_users_role          ON users(role);
CREATE INDEX IF NOT EXISTS idx_api_usage_created   ON api_usage(created_at);
CREATE INDEX IF NOT EXISTS idx_api_usage_api_time  ON api_usage(api_id, created_at);
CREATE INDEX IF NOT EXISTS idx_audit_created       ON admin_audit_log(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_audit_target        ON admin_audit_log(target_user_id);
CREATE INDEX IF NOT EXISTS idx_api_keys_user       ON api_keys(user_id);
CREATE INDEX IF NOT EXISTS idx_api_usage_user_time ON api_usage(user_id, created_at);
CREATE INDEX IF NOT EXISTS idx_api_usage_quota     ON api_usage(user_id, api_id, created_at);

-- ══════════════════════════════════════════════════════════════
-- Catalog seed: the four TikTok crawls the pipeline exposes
-- ══════════════════════════════════════════════════════════════
INSERT INTO apis (slug, name, description, provider, category, gateway_path, upstream_method, params, example_response, rate_limit_per_minute, monthly_quota) VALUES
(
  'tiktok-post',
  'TikTok Post Details',
  'Details and engagement stats (plays, likes, comments, shares) for a single TikTok video, plus its author.',
  'TikTok',
  'Social',
  '/gateway/v1/tiktok/post',
  'post',
  '[{"name":"post_id","type":"string","required":true,"description":"Numeric TikTok video ID","example":"7656787492309552405"}]',
  '{"id":"7656787492309552405","url":"https://www.tiktok.com/@user/video/7656787492309552405","description":"Caption #tag","created_at":"2026-06-30T02:00:00+00:00","stats":{"plays":25877,"likes":916,"comments":26,"shares":130},"cover_url":"https://...","author":{"id":"7111953329186292741","username":"user","nickname":"Display Name","followers":40742}}',
  30, 1000
),
(
  'tiktok-comments',
  'TikTok Video Comments',
  'Comments on a TikTok video with author, likes and reply counts, 20 per page. Pass next_cursor as cursor to get the next page.',
  'TikTok',
  'Social',
  '/gateway/v1/tiktok/comments',
  'comments',
  '[{"name":"post_id","type":"string","required":true,"description":"Numeric TikTok video ID","example":"7656787492309552405"},{"name":"cursor","type":"string","required":false,"description":"Page position: the next_cursor of the previous response (omit for the first page)","example":"20"}]',
  '{"post_id":"7656787492309552405","comments":[{"id":"7656790000000000001","text":"Nice clip","created_at":"2026-06-30T03:10:00+00:00","likes":12,"reply_count":1,"author":{"id":"6800000000000000000","username":"someone","nickname":"Someone"}}],"next_cursor":"20","has_more":true}',
  10, 300
),
(
  'tiktok-search',
  'TikTok Search Posts',
  'Search TikTok videos by keyword or hashtag, 10 posts per page. Pass next_cursor as cursor and search_id from the response to get the next page.',
  'TikTok',
  'Social',
  '/gateway/v1/tiktok/search',
  'search',
  '[{"name":"keyword","type":"string","required":true,"description":"Keyword or #hashtag to search","example":"#CS2"},{"name":"cursor","type":"string","required":false,"description":"Page position: the next_cursor of the previous response (omit for the first page)","example":"10"},{"name":"search_id","type":"string","required":false,"description":"Search session id: the search_id of the previous response (omit for the first page)","example":"20261030143012ABCDEF0123456789AB"}]',
  '{"keyword":"#CS2","posts":[{"id":"7656787492309552405","url":"https://www.tiktok.com/@user/video/7656787492309552405","description":"Caption #CS2","created_at":"2026-06-30T02:00:00+00:00","stats":{"plays":25877,"likes":916,"comments":26,"shares":130},"cover_url":"https://...","author":{"id":"7111953329186292741","username":"user","nickname":"Display Name","followers":40742}}],"next_cursor":"10","has_more":true,"search_id":"20261030143012ABCDEF0123456789AB"}',
  10, 300
),
(
  'tiktok-profile-posts',
  'TikTok Profile Posts',
  'Recent videos published by a TikTok account, looked up by its numeric user ID.',
  'TikTok',
  'Social',
  '/gateway/v1/tiktok/profile-posts',
  'profile-posts',
  '[{"name":"user_id","type":"string","required":true,"description":"Numeric TikTok user ID (the author.id of any post)","example":"107955"}]',
  '{"user_id":"107955","posts":[{"id":"7647151424366824725","url":"https://www.tiktok.com/@tiktok/video/7647151424366824725","description":"Caption","created_at":"2026-06-04T10:30:00+00:00","stats":{"plays":772962,"likes":25348,"comments":250,"shares":651},"cover_url":"https://...","author":{"id":"107955","username":"tiktok","nickname":"TikTok","followers":90000000}}]}',
  10, 300
)
ON CONFLICT (slug) DO NOTHING;

-- ══════════════════════════════════════════════════════════════
-- Subscription plans and SePay bank-transfer payments.
--
--   plans            what can be bought (edit prices/limits here, no deploy)
--   subscriptions    a user's paid period; at most one `active` per user
--   payment_orders   a user's intent to pay for a plan, matched to a bank
--                    transfer by its short `payment_code`
--   payment_events   every payment notification we received (webhook or
--                    reconciliation), kept for audit and manual review
--
-- All money is integer VND. A user with no active subscription is on the
-- implicit free tier, which keeps using each API's own `monthly_quota` and
-- `rate_limit_per_minute`; a paid plan's pool replaces those per-API numbers.
-- ══════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS plans (
    id                    UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    slug                  VARCHAR(20)  NOT NULL UNIQUE,
    name                  VARCHAR(50)  NOT NULL,
    price_vnd             INT          NOT NULL,
    period_days           INT          NOT NULL DEFAULT 30,
    -- Total billable requests per billing period across ALL APIs.
    monthly_quota         INT          NOT NULL,
    rate_limit_per_minute INT          NOT NULL,
    features              JSONB        NOT NULL DEFAULT '[]'::jsonb,
    sort_order            INT          NOT NULL DEFAULT 0,
    enabled               BOOLEAN      NOT NULL DEFAULT TRUE,
    created_at            TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at            TIMESTAMPTZ  NOT NULL DEFAULT NOW(),

    CONSTRAINT plans_price_positive  CHECK (price_vnd > 0),
    CONSTRAINT plans_period_positive CHECK (period_days > 0),
    CONSTRAINT plans_quota_positive  CHECK (monthly_quota > 0),
    CONSTRAINT plans_rate_positive   CHECK (rate_limit_per_minute > 0)
);

CREATE TABLE IF NOT EXISTS subscriptions (
    id                   UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id              UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    plan_id              UUID        NOT NULL REFERENCES plans(id),
    status               VARCHAR(20) NOT NULL DEFAULT 'active',
    current_period_start TIMESTAMPTZ NOT NULL,
    current_period_end   TIMESTAMPTZ NOT NULL,
    created_at           TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at           TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT subscriptions_status_check CHECK (status IN ('active', 'expired', 'canceled')),
    CONSTRAINT subscriptions_period_check CHECK (current_period_end > current_period_start)
);

CREATE TABLE IF NOT EXISTS payment_orders (
    id                  UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id             UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    plan_id             UUID        NOT NULL REFERENCES plans(id),
    amount_vnd          INT         NOT NULL,
    status              VARCHAR(20) NOT NULL DEFAULT 'pending',
    -- What the customer types as the transfer content, e.g. WOG7K2M9QXA.
    payment_code        VARCHAR(20) NOT NULL UNIQUE,
    -- SePay's id of the transfer that paid this order. UNIQUE: one transfer
    -- can only ever settle one order.
    sepay_transaction_id BIGINT UNIQUE,
    paid_amount_vnd     BIGINT,
    paid_at             TIMESTAMPTZ,
    expires_at          TIMESTAMPTZ NOT NULL,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT payment_orders_status_check
        CHECK (status IN ('pending', 'paid', 'expired', 'canceled', 'failed')),
    CONSTRAINT payment_orders_amount_positive CHECK (amount_vnd > 0)
);

CREATE TABLE IF NOT EXISTS payment_events (
    id                   UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    -- 'webhook' or 'reconcile'
    source               VARCHAR(20) NOT NULL,
    sepay_transaction_id BIGINT,
    transfer_type        VARCHAR(10),
    amount_vnd           BIGINT,
    payment_code         VARCHAR(40),
    content              TEXT,
    payload              JSONB       NOT NULL,
    matched_order_id     UUID REFERENCES payment_orders(id) ON DELETE SET NULL,
    -- activated | duplicate | underpaid | unmatched | ignored | already_paid
    -- | canceled_order | not_applied | rejected
    outcome              VARCHAR(30) NOT NULL,
    note                 TEXT,
    -- Money arrived that did not cleanly activate a plan: a person must look.
    needs_review         BOOLEAN     NOT NULL DEFAULT FALSE,
    created_at           TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS uq_subscriptions_one_active
    ON subscriptions(user_id) WHERE status = 'active';
CREATE INDEX IF NOT EXISTS idx_subscriptions_active_end
    ON subscriptions(current_period_end) WHERE status = 'active';
CREATE UNIQUE INDEX IF NOT EXISTS uq_payment_orders_one_pending
    ON payment_orders(user_id, plan_id) WHERE status = 'pending';
CREATE INDEX IF NOT EXISTS idx_payment_orders_user
    ON payment_orders(user_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_payment_orders_pending_expiry
    ON payment_orders(expires_at) WHERE status = 'pending';
CREATE INDEX IF NOT EXISTS idx_payment_events_created
    ON payment_events(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_payment_events_review
    ON payment_events(created_at DESC) WHERE needs_review;
CREATE INDEX IF NOT EXISTS idx_payment_events_txn
    ON payment_events(sepay_transaction_id);

-- ══════════════════════════════════════════════════════════════
-- Plan seed. PLACEHOLDER prices and limits: edit them freely
-- (UPDATE plans ...); the app reads them on every request.
-- ══════════════════════════════════════════════════════════════
INSERT INTO plans (slug, name, price_vnd, period_days, monthly_quota, rate_limit_per_minute, features, sort_order) VALUES
(
  'basic', 'Basic', 99000, 30, 10000, 30,
  '["10,000 requests per month","30 requests per minute","All TikTok APIs","Email support"]',
  1
),
(
  'pro', 'Pro', 299000, 30, 100000, 120,
  '["100,000 requests per month","120 requests per minute","All TikTok APIs","Priority support"]',
  2
),
(
  'ultra', 'Ultra', 799000, 30, 1000000, 600,
  '["1,000,000 requests per month","600 requests per minute","All TikTok APIs","Priority support","Early access to new APIs"]',
  3
)
ON CONFLICT (slug) DO NOTHING;

-- ══════════════════════════════════════════════════════════════
-- Support chat: one conversation per user, answered by admins.
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

-- ══════════════════════════════════════════════════════════════
-- Forgot / reset password.
--
--   password_reset_tokens   one row per emailed reset link. Only the SHA-256
--                           of the token is stored, so a database leak does
--                           not hand out working links. A token is single-use
--                           (`used_at`) and short-lived (`expires_at`).
-- ══════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS password_reset_tokens (
    id         UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id    UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash CHAR(64)    NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    used_at    TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS password_reset_tokens_hash_idx
    ON password_reset_tokens (token_hash);

-- "How many links did this account ask for recently?" (throttling) and cleanup.
CREATE INDEX IF NOT EXISTS password_reset_tokens_user_idx
    ON password_reset_tokens (user_id, created_at DESC);

-- ══════════════════════════════════════════════════════════════
-- Auth sessions
-- ══════════════════════════════════════════════════════════════
-- Refresh-token sessions: one row per sign-in, so a refresh token can be rotated,
-- revoked (logout, password reset) and caught when it is replayed.
CREATE TABLE IF NOT EXISTS refresh_sessions (
    id            UUID PRIMARY KEY,
    user_id       UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    -- The `jti` of the refresh token that may be used next.
    current_jti   UUID NOT NULL,
    -- The one before it. A second tab that raced the rotation presents this one;
    -- inside a short grace window that is harmless, after it it means a copied token.
    previous_jti  UUID,
    rotated_at    TIMESTAMPTZ,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_used_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at    TIMESTAMPTZ NOT NULL,
    revoked_at    TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_refresh_sessions_user ON refresh_sessions(user_id);

-- Short-lived single-use codes handed to the browser instead of a token:
--   'oauth' : the Google sign-in redirect carries one; the page swaps it for tokens.
--   'ws'    : opens the chat socket (browsers cannot send an Authorization header on one).
-- Only the SHA-256 of the code is stored.
CREATE TABLE IF NOT EXISTS auth_codes (
    code_hash   CHAR(64) PRIMARY KEY,
    kind        TEXT NOT NULL,
    user_id     UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    session_id  UUID REFERENCES refresh_sessions(id) ON DELETE CASCADE,
    expires_at  TIMESTAMPTZ NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_auth_codes_expires ON auth_codes(expires_at);
