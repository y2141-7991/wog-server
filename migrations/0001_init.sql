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
    oauth_provider  VARCHAR(20),
    oauth_sub       VARCHAR(255),
    created_at      TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ  NOT NULL DEFAULT NOW()
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
-- ══════════════════════════════════════════════════════════════
-- Chat Rooms
-- ══════════════════════════════════════════════════════════════
CREATE TABLE IF NOT EXISTS rooms (
    id          UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name        VARCHAR(100) NOT NULL,
    description TEXT,
    is_private  BOOLEAN      NOT NULL DEFAULT FALSE,
    owner_id    UUID         NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at  TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

-- ══════════════════════════════════════════════════════════════
-- Room Members (many-to-many)
-- ══════════════════════════════════════════════════════════════
CREATE TABLE IF NOT EXISTS room_members (
    room_id     UUID NOT NULL REFERENCES rooms(id) ON DELETE CASCADE,
    user_id     UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role        VARCHAR(20)  NOT NULL DEFAULT 'member',
    joined_at   TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    PRIMARY KEY (room_id, user_id)
);

-- ══════════════════════════════════════════════════════════════
-- Messages
-- ══════════════════════════════════════════════════════════════
CREATE TABLE IF NOT EXISTS messages (
    id          UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    room_id     UUID         NOT NULL REFERENCES rooms(id) ON DELETE CASCADE,
    sender_id   UUID         NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    content     TEXT         NOT NULL,
    msg_type    VARCHAR(20)  NOT NULL DEFAULT 'text',
    is_edited   BOOLEAN      NOT NULL DEFAULT FALSE,
    created_at  TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

-- ══════════════════════════════════════════════════════════════
-- Wallets — one per user
-- ══════════════════════════════════════════════════════════════
--
-- `version` column = optimistic lock
-- Every UPDATE must include: WHERE version = <expected_version>
-- If two requests try to spend simultaneously:
--   Request A: UPDATE ... SET version = 3 WHERE version = 2  ← succeeds
--   Request B: UPDATE ... SET version = 3 WHERE version = 2  ← 0 rows affected → retry or fail
--
CREATE TABLE IF NOT EXISTS wallets (
    id          UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id     UUID         NOT NULL UNIQUE REFERENCES users(id),
    balance     DECIMAL(18,2) NOT NULL DEFAULT 0.00,
    version     BIGINT       NOT NULL DEFAULT 0,
    created_at  TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ  NOT NULL DEFAULT NOW(),

    CONSTRAINT positive_balance CHECK (balance >= 0)
);

-- ══════════════════════════════════════════════════════════════
-- Wallet Transactions — append-only ledger
-- ══════════════════════════════════════════════════════════════
--
-- Every balance change MUST have a corresponding transaction.
-- This is the audit trail / source of truth.
--
-- Types:
--   TOP_UP       +amount  (user adds money)
--   EVENT_JOIN   -amount  (user pays to join event)
--   REFUND       +amount  (user gets money back)
--   WITHDRAWAL   -amount  (user withdraws money)
--
CREATE TABLE IF NOT EXISTS wallet_transactions (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    wallet_id       UUID         NOT NULL REFERENCES wallets(id),
    tx_type         VARCHAR(20)  NOT NULL,
    amount          DECIMAL(18,2) NOT NULL,
    balance_before  DECIMAL(18,2) NOT NULL,
    balance_after   DECIMAL(18,2) NOT NULL,
    reference_type  VARCHAR(20),          -- 'event_registration', null for top-up
    reference_id    UUID,                 -- event_registration.id, null for top-up
    description     TEXT,
    created_at      TIMESTAMPTZ  NOT NULL DEFAULT NOW(),

    CONSTRAINT positive_amount CHECK (amount > 0),
    CONSTRAINT valid_tx_type CHECK (
        tx_type IN ('TOP_UP', 'EVENT_JOIN', 'REFUND', 'WITHDRAWAL')
    )
);

-- ══════════════════════════════════════════════════════════════
-- Events
-- ══════════════════════════════════════════════════════════════
CREATE TABLE IF NOT EXISTS events (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    title           VARCHAR(200) NOT NULL,
    description     TEXT,
    organizer_id    UUID         NOT NULL REFERENCES users(id),
    price           DECIMAL(18,2) NOT NULL DEFAULT 0.00,
    capacity        INT          NOT NULL DEFAULT 100,
    registered_count INT         NOT NULL DEFAULT 0,
    status          VARCHAR(20)  NOT NULL DEFAULT 'OPEN',
    start_time      TIMESTAMPTZ  NOT NULL,
    end_time        TIMESTAMPTZ  NOT NULL,
    location        TEXT,
    created_at      TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ  NOT NULL DEFAULT NOW(),

    CONSTRAINT positive_price CHECK (price >= 0),
    CONSTRAINT positive_capacity CHECK (capacity > 0),
    CONSTRAINT valid_time CHECK (end_time > start_time),
    CONSTRAINT valid_status CHECK (
        status IN ('DRAFT', 'OPEN', 'CLOSED', 'CANCELLED')
    )
);

-- ══════════════════════════════════════════════════════════════
-- Event Registrations — join table
-- ══════════════════════════════════════════════════════════════
CREATE TABLE IF NOT EXISTS event_registrations (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    event_id        UUID         NOT NULL REFERENCES events(id),
    user_id         UUID         NOT NULL REFERENCES users(id),
    wallet_tx_id    UUID         REFERENCES wallet_transactions(id),
    amount_paid     DECIMAL(18,2) NOT NULL DEFAULT 0.00,
    status          VARCHAR(20)  NOT NULL DEFAULT 'CONFIRMED',
    registered_at   TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    cancelled_at    TIMESTAMPTZ,

    CONSTRAINT unique_registration UNIQUE (event_id, user_id),
    CONSTRAINT valid_reg_status CHECK (
        status IN ('CONFIRMED', 'CANCELLED', 'REFUNDED')
    )
);

-- ══════════════════════════════════════════════════════════════
-- Indexes
-- ══════════════════════════════════════════════════════════════
CREATE INDEX IF NOT EXISTS idx_wallet_tx_wallet ON wallet_transactions(wallet_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_wallet_tx_ref    ON wallet_transactions(reference_type, reference_id);
CREATE INDEX IF NOT EXISTS idx_events_status    ON events(status, start_time);
CREATE INDEX IF NOT EXISTS idx_events_organizer ON events(organizer_id);
CREATE INDEX IF NOT EXISTS idx_reg_event        ON event_registrations(event_id);
CREATE INDEX IF NOT EXISTS idx_reg_user         ON event_registrations(user_id);

-- ══════════════════════════════════════════════════════════════
-- Indexes
-- ══════════════════════════════════════════════════════════════
CREATE INDEX IF NOT EXISTS idx_users_email       ON users(email);
CREATE INDEX IF NOT EXISTS idx_users_username    ON users(username);
CREATE UNIQUE INDEX IF NOT EXISTS idx_users_oauth
    ON users(oauth_provider, oauth_sub)
    WHERE oauth_provider IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_messages_room     ON messages(room_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_messages_sender   ON messages(sender_id);
CREATE INDEX IF NOT EXISTS idx_room_members_user ON room_members(user_id);
