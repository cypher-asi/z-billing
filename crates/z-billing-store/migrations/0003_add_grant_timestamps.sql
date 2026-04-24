-- Add timestamp columns for tracking one-time signup grant and daily credit grants.
-- These enable idempotent grant operations without scanning transaction history.

ALTER TABLE accounts ADD COLUMN signup_grant_at TIMESTAMPTZ;
ALTER TABLE accounts ADD COLUMN last_daily_grant_at TIMESTAMPTZ;
