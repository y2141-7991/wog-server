-- ══════════════════════════════════════════════════════════════
-- Subscription plans and SePay bank-transfer payments.
--
-- One migration for the whole feature; idempotent (IF NOT EXISTS /
-- ON CONFLICT DO NOTHING), so it is safe on a fresh database, on one already
-- at 0002, and when run twice.
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
