-- Processed webhook events for idempotency / replay protection
CREATE TABLE processed_webhooks (
    event_id TEXT PRIMARY KEY,
    source TEXT NOT NULL,
    processed_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
