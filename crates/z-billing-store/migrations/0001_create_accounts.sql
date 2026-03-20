-- Billing accounts
CREATE TABLE accounts (
    user_id UUID PRIMARY KEY,
    balance_cents BIGINT NOT NULL DEFAULT 0,
    lifetime_purchased_cents BIGINT NOT NULL DEFAULT 0,
    lifetime_granted_cents BIGINT NOT NULL DEFAULT 0,
    lifetime_used_cents BIGINT NOT NULL DEFAULT 0,
    subscription JSONB,
    auto_refill JSONB,
    lago_customer_id TEXT,
    stripe_customer_id TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Credit transactions (ULID-ordered)
CREATE TABLE credit_transactions (
    id TEXT PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES accounts(user_id),
    amount_cents BIGINT NOT NULL,
    transaction_type TEXT NOT NULL,
    balance_after_cents BIGINT NOT NULL,
    description TEXT NOT NULL,
    metadata JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_credit_transactions_user_id ON credit_transactions(user_id, created_at DESC);

-- Usage events (for idempotency)
CREATE TABLE usage_events (
    event_id TEXT PRIMARY KEY,
    user_id UUID NOT NULL,
    agent_id UUID,
    source JSONB NOT NULL,
    metric JSONB NOT NULL,
    quantity DOUBLE PRECISION NOT NULL,
    cost_cents BIGINT NOT NULL,
    event_timestamp TIMESTAMPTZ NOT NULL,
    metadata JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_usage_events_user_id ON usage_events(user_id, event_timestamp DESC);
