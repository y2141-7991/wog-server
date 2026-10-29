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
    upstream_path          VARCHAR(200) NOT NULL,
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
INSERT INTO apis (slug, name, description, provider, category, gateway_path, upstream_path, params, example_response, rate_limit_per_minute, monthly_quota) VALUES
(
  'tiktok-post',
  'TikTok Post Details',
  'Details and engagement stats (plays, likes, comments, shares) for a single TikTok video, plus its author.',
  'TikTok',
  'Social',
  '/gateway/v1/tiktok/post',
  '/v1/post',
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
  '/v1/comments',
  '[{"name":"post_id","type":"string","required":true,"description":"Numeric TikTok video ID","example":"7656787492309552405"},{"name":"cursor","type":"string","required":false,"description":"Page position: the next_cursor of the previous response (omit for the first page)","example":"20"}]',
  '{"post_id":"7656787492309552405","comments":[{"id":"7656790000000000001","text":"Nice clip","created_at":"2026-06-30T03:10:00+00:00","likes":12,"reply_count":1,"author":{"id":"6800000000000000000","username":"someone","nickname":"Someone"}}],"next_cursor":"20","has_more":true}',
  10, 300
),
(
  'tiktok-search',
  'TikTok Search Posts',
  'Search TikTok videos by keyword or hashtag, 10 posts per page. Pass next_cursor as cursor to get the next page.',
  'TikTok',
  'Social',
  '/gateway/v1/tiktok/search',
  '/v1/search',
  '[{"name":"keyword","type":"string","required":true,"description":"Keyword or #hashtag to search","example":"#CS2"},{"name":"cursor","type":"string","required":false,"description":"Page position: the next_cursor of the previous response (omit for the first page)","example":"10"}]',
  '{"keyword":"#CS2","posts":[{"id":"7656787492309552405","url":"https://www.tiktok.com/@user/video/7656787492309552405","description":"Caption #CS2","created_at":"2026-06-30T02:00:00+00:00","stats":{"plays":25877,"likes":916,"comments":26,"shares":130},"cover_url":"https://...","author":{"id":"7111953329186292741","username":"user","nickname":"Display Name","followers":40742}}],"next_cursor":"10","has_more":true}',
  10, 300
),
(
  'tiktok-profile-posts',
  'TikTok Profile Posts',
  'Recent videos published by a TikTok account, looked up by its numeric user ID.',
  'TikTok',
  'Social',
  '/gateway/v1/tiktok/profile-posts',
  '/v1/profile-posts',
  '[{"name":"user_id","type":"string","required":true,"description":"Numeric TikTok user ID (the author.id of any post)","example":"107955"}]',
  '{"user_id":"107955","posts":[{"id":"7647151424366824725","url":"https://www.tiktok.com/@tiktok/video/7647151424366824725","description":"Caption","created_at":"2026-06-04T10:30:00+00:00","stats":{"plays":772962,"likes":25348,"comments":250,"shares":651},"cover_url":"https://...","author":{"id":"107955","username":"tiktok","nickname":"TikTok","followers":90000000}}]}',
  10, 300
)
ON CONFLICT (slug) DO NOTHING;
