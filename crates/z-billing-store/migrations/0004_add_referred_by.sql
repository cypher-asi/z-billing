-- Track who referred this user for deferred referral credits.
-- Populated during signup grant, consumed when user subscribes to a paid plan.

ALTER TABLE accounts ADD COLUMN referred_by TEXT;
