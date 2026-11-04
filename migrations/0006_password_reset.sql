-- ══════════════════════════════════════════════════════════════
-- Forgot / reset password.
--
-- Idempotent (IF NOT EXISTS), so it is safe on a fresh database, on one
-- already at 0005, and when run twice.
--
--   password_reset_tokens   one row per emailed reset link. Only the SHA-256
--                           of the token is stored, so a database leak does
--                           not hand out working links. A token is single-use
--                           (`used_at`) and short-lived (`expires_at`).
--   users.password_changed_at
--                           set by a reset. Access and refresh tokens issued
--                           before it are rejected, which signs the account
--                           out everywhere. NULL (never reset) means no cutoff,
--                           so existing sessions are untouched by this migration.
-- ══════════════════════════════════════════════════════════════

ALTER TABLE users ADD COLUMN IF NOT EXISTS password_changed_at TIMESTAMPTZ;

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
