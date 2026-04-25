-- Add columns for credit grant tracking and ZERO Pro status.

ALTER TABLE accounts ADD COLUMN is_zero_pro BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE accounts ADD COLUMN signup_grant_at TIMESTAMPTZ;
ALTER TABLE accounts ADD COLUMN last_daily_grant_at TIMESTAMPTZ;
ALTER TABLE accounts ADD COLUMN last_monthly_grant_at TIMESTAMPTZ;
