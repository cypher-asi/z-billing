//! PostgreSQL storage backend for z-billing.
//!
//! Implements the `Store` trait using sqlx with PostgreSQL.

use sqlx::PgPool;

use z_billing_core::{Account, CreditTransaction, TransactionId, UsageEvent, UserId};

use crate::error::{Result, StoreError};
use crate::Store;

/// PostgreSQL-backed store for z-billing.
#[derive(Clone)]
pub struct PgStore {
    pool: PgPool,
}

impl PgStore {
    /// Create a new PostgreSQL store with the given connection pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Get a reference to the connection pool.
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}

impl Store for PgStore {
    fn put_account(&self, account: &Account) -> Result<()> {
        let pool = self.pool.clone();
        let account = account.clone();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                sqlx::query(
                    r#"
                    INSERT INTO accounts (user_id, balance_cents, lifetime_purchased_cents,
                        lifetime_granted_cents, lifetime_used_cents, subscription, auto_refill,
                        lago_customer_id, stripe_customer_id, is_zero_pro, signup_grant_at,
                        last_daily_grant_at, last_monthly_grant_at, created_at, updated_at)
                    VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
                    ON CONFLICT (user_id) DO UPDATE SET
                        balance_cents = $2,
                        lifetime_purchased_cents = $3,
                        lifetime_granted_cents = $4,
                        lifetime_used_cents = $5,
                        subscription = $6,
                        auto_refill = $7,
                        lago_customer_id = $8,
                        stripe_customer_id = $9,
                        is_zero_pro = $10,
                        signup_grant_at = $11,
                        last_daily_grant_at = $12,
                        last_monthly_grant_at = $13,
                        updated_at = $15
                    "#,
                )
                .bind(account.user_id.as_uuid())
                .bind(account.balance_cents)
                .bind(account.lifetime_purchased_cents)
                .bind(account.lifetime_granted_cents)
                .bind(account.lifetime_used_cents)
                .bind(serde_json::to_value(&account.subscription).unwrap_or_default())
                .bind(serde_json::to_value(&account.auto_refill).unwrap_or_default())
                .bind(&account.lago_customer_id)
                .bind(&account.stripe_customer_id)
                .bind(account.is_zero_pro)
                .bind(account.signup_grant_at)
                .bind(account.last_daily_grant_at)
                .bind(account.last_monthly_grant_at)
                .bind(account.created_at)
                .bind(account.updated_at)
                .execute(&pool)
                .await
                .map_err(|e| StoreError::Database(e.to_string()))?;

                Ok(())
            })
        })
    }

    fn get_account(&self, user_id: &UserId) -> Result<Option<Account>> {
        let pool = self.pool.clone();
        let user_id = *user_id;
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let row =
                    sqlx::query_as::<_, AccountRow>("SELECT * FROM accounts WHERE user_id = $1")
                        .bind(user_id.as_uuid())
                        .fetch_optional(&pool)
                        .await
                        .map_err(|e| StoreError::Database(e.to_string()))?;

                Ok(row.map(|r| r.into_account()))
            })
        })
    }

    fn delete_account(&self, user_id: &UserId) -> Result<()> {
        let pool = self.pool.clone();
        let user_id = *user_id;
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let result = sqlx::query("DELETE FROM accounts WHERE user_id = $1")
                    .bind(user_id.as_uuid())
                    .execute(&pool)
                    .await
                    .map_err(|e| StoreError::Database(e.to_string()))?;

                if result.rows_affected() == 0 {
                    return Err(StoreError::NotFound {
                        entity: "account",
                        id: user_id.to_string(),
                    });
                }
                Ok(())
            })
        })
    }

    #[allow(deprecated)]
    fn update_balance(&self, user_id: &UserId, delta_cents: i64) -> Result<i64> {
        let pool = self.pool.clone();
        let user_id = *user_id;
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let row = sqlx::query_scalar::<_, i64>(
                    r#"
                    UPDATE accounts
                    SET balance_cents = balance_cents + $2, updated_at = NOW()
                    WHERE user_id = $1
                    RETURNING balance_cents
                    "#,
                )
                .bind(user_id.as_uuid())
                .bind(delta_cents)
                .fetch_optional(&pool)
                .await
                .map_err(|e| StoreError::Database(e.to_string()))?;

                row.ok_or(StoreError::NotFound {
                    entity: "account",
                    id: user_id.to_string(),
                })
            })
        })
    }

    fn put_transaction(&self, transaction: &CreditTransaction) -> Result<()> {
        let pool = self.pool.clone();
        let tx = transaction.clone();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                sqlx::query(
                    r#"
                    INSERT INTO credit_transactions (id, user_id, amount_cents, transaction_type,
                        balance_after_cents, description, metadata, created_at)
                    VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                    "#,
                )
                .bind(tx.id.to_string())
                .bind(tx.user_id.as_uuid())
                .bind(tx.amount_cents)
                .bind(
                    serde_json::to_string(&tx.transaction_type)
                        .unwrap_or_default()
                        .trim_matches('"'),
                )
                .bind(tx.balance_after_cents)
                .bind(&tx.description)
                .bind(&tx.metadata)
                .bind(tx.created_at)
                .execute(&pool)
                .await
                .map_err(|e| StoreError::Database(e.to_string()))?;

                Ok(())
            })
        })
    }

    fn get_transaction(&self, transaction_id: &TransactionId) -> Result<Option<CreditTransaction>> {
        let pool = self.pool.clone();
        let tx_id = transaction_id.to_string();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let row = sqlx::query_as::<_, TransactionRow>(
                    "SELECT * FROM credit_transactions WHERE id = $1",
                )
                .bind(&tx_id)
                .fetch_optional(&pool)
                .await
                .map_err(|e| StoreError::Database(e.to_string()))?;

                Ok(row.map(|r| r.into_transaction()))
            })
        })
    }

    fn list_transactions_by_user(
        &self,
        user_id: &UserId,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<CreditTransaction>> {
        let pool = self.pool.clone();
        let user_id = *user_id;
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let rows = sqlx::query_as::<_, TransactionRow>(
                    r#"
                    SELECT * FROM credit_transactions
                    WHERE user_id = $1
                    ORDER BY created_at DESC
                    LIMIT $2 OFFSET $3
                    "#,
                )
                .bind(user_id.as_uuid())
                .bind(limit as i64)
                .bind(offset as i64)
                .fetch_all(&pool)
                .await
                .map_err(|e| StoreError::Database(e.to_string()))?;

                Ok(rows.into_iter().map(|r| r.into_transaction()).collect())
            })
        })
    }

    fn has_usage_event(&self, event_id: &str) -> Result<bool> {
        let pool = self.pool.clone();
        let event_id = event_id.to_string();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let exists = sqlx::query_scalar::<_, bool>(
                    "SELECT EXISTS(SELECT 1 FROM usage_events WHERE event_id = $1)",
                )
                .bind(&event_id)
                .fetch_one(&pool)
                .await
                .map_err(|e| StoreError::Database(e.to_string()))?;

                Ok(exists)
            })
        })
    }

    fn put_usage_event(&self, event: &UsageEvent) -> Result<()> {
        let pool = self.pool.clone();
        let event = event.clone();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                sqlx::query(
                    r#"
                    INSERT INTO usage_events (event_id, user_id, agent_id, source, metric,
                        quantity, cost_cents, event_timestamp, metadata)
                    VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                    ON CONFLICT (event_id) DO NOTHING
                    "#,
                )
                .bind(&event.event_id)
                .bind(event.user_id.as_uuid())
                .bind(event.agent_id.map(|a| *a.as_uuid()))
                .bind(serde_json::to_value(&event.source).unwrap_or_default())
                .bind(serde_json::to_value(&event.metric).unwrap_or_default())
                .bind(event.quantity)
                .bind(event.cost_cents)
                .bind(event.timestamp)
                .bind(&event.metadata)
                .execute(&pool)
                .await
                .map_err(|e| StoreError::Database(e.to_string()))?;

                Ok(())
            })
        })
    }

    fn get_usage_event(&self, event_id: &str) -> Result<Option<UsageEvent>> {
        let pool = self.pool.clone();
        let event_id = event_id.to_string();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let row = sqlx::query_as::<_, UsageEventRow>(
                    "SELECT * FROM usage_events WHERE event_id = $1",
                )
                .bind(&event_id)
                .fetch_optional(&pool)
                .await
                .map_err(|e| StoreError::Database(e.to_string()))?;

                Ok(row.map(|r| r.into_usage_event()))
            })
        })
    }

    fn has_webhook_event(&self, event_id: &str) -> Result<bool> {
        let pool = self.pool.clone();
        let event_id = event_id.to_string();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let exists = sqlx::query_scalar::<_, bool>(
                    "SELECT EXISTS(SELECT 1 FROM processed_webhooks WHERE event_id = $1)",
                )
                .bind(&event_id)
                .fetch_one(&pool)
                .await
                .map_err(|e| StoreError::Database(e.to_string()))?;

                Ok(exists)
            })
        })
    }

    fn record_webhook_event(&self, event_id: &str, source: &str) -> Result<()> {
        let pool = self.pool.clone();
        let event_id = event_id.to_string();
        let source = source.to_string();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                sqlx::query(
                    "INSERT INTO processed_webhooks (event_id, source) VALUES ($1, $2) ON CONFLICT (event_id) DO NOTHING",
                )
                .bind(&event_id)
                .bind(&source)
                .execute(&pool)
                .await
                .map_err(|e| StoreError::Database(e.to_string()))?;

                Ok(())
            })
        })
    }

    fn process_usage(&self, event: &UsageEvent, transaction: &CreditTransaction) -> Result<i64> {
        let pool = self.pool.clone();
        let event = event.clone();
        let tx = transaction.clone();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                // Use a database transaction for atomicity
                let mut db_tx = pool
                    .begin()
                    .await
                    .map_err(|e| StoreError::Database(e.to_string()))?;

                // Check idempotency
                let exists = sqlx::query_scalar::<_, bool>(
                    "SELECT EXISTS(SELECT 1 FROM usage_events WHERE event_id = $1)",
                )
                .bind(&event.event_id)
                .fetch_one(&mut *db_tx)
                .await
                .map_err(|e| StoreError::Database(e.to_string()))?;

                if exists {
                    return Err(StoreError::DuplicateEvent {
                        event_id: event.event_id.clone(),
                    });
                }

                // Lock and check balance
                let balance = sqlx::query_scalar::<_, i64>(
                    "SELECT balance_cents FROM accounts WHERE user_id = $1 FOR UPDATE",
                )
                .bind(event.user_id.as_uuid())
                .fetch_optional(&mut *db_tx)
                .await
                .map_err(|e| StoreError::Database(e.to_string()))?
                .ok_or(StoreError::NotFound {
                    entity: "account",
                    id: event.user_id.to_string(),
                })?;

                if balance < event.cost_cents {
                    return Err(StoreError::InsufficientCredits {
                        balance,
                        required: event.cost_cents,
                    });
                }

                // Deduct credits
                let new_balance = sqlx::query_scalar::<_, i64>(
                    r#"
                    UPDATE accounts
                    SET balance_cents = balance_cents - $2,
                        lifetime_used_cents = lifetime_used_cents + $2,
                        updated_at = NOW()
                    WHERE user_id = $1
                    RETURNING balance_cents
                    "#,
                )
                .bind(event.user_id.as_uuid())
                .bind(event.cost_cents)
                .fetch_one(&mut *db_tx)
                .await
                .map_err(|e| StoreError::Database(e.to_string()))?;

                // Record transaction
                sqlx::query(
                    r#"
                    INSERT INTO credit_transactions (id, user_id, amount_cents, transaction_type,
                        balance_after_cents, description, metadata, created_at)
                    VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                    "#,
                )
                .bind(tx.id.to_string())
                .bind(tx.user_id.as_uuid())
                .bind(tx.amount_cents)
                .bind(
                    serde_json::to_string(&tx.transaction_type)
                        .unwrap_or_default()
                        .trim_matches('"'),
                )
                .bind(new_balance)
                .bind(&tx.description)
                .bind(&tx.metadata)
                .bind(tx.created_at)
                .execute(&mut *db_tx)
                .await
                .map_err(|e| StoreError::Database(e.to_string()))?;

                // Record usage event
                sqlx::query(
                    r#"
                    INSERT INTO usage_events (event_id, user_id, agent_id, source, metric,
                        quantity, cost_cents, event_timestamp, metadata)
                    VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                    "#,
                )
                .bind(&event.event_id)
                .bind(event.user_id.as_uuid())
                .bind(event.agent_id.map(|a| *a.as_uuid()))
                .bind(serde_json::to_value(&event.source).unwrap_or_default())
                .bind(serde_json::to_value(&event.metric).unwrap_or_default())
                .bind(event.quantity)
                .bind(event.cost_cents)
                .bind(event.timestamp)
                .bind(&event.metadata)
                .execute(&mut *db_tx)
                .await
                .map_err(|e| StoreError::Database(e.to_string()))?;

                db_tx
                    .commit()
                    .await
                    .map_err(|e| StoreError::Database(e.to_string()))?;

                Ok(new_balance)
            })
        })
    }

    fn add_credits(
        &self,
        user_id: &UserId,
        amount_cents: i64,
        transaction: &CreditTransaction,
    ) -> Result<i64> {
        let pool = self.pool.clone();
        let user_id = *user_id;
        let tx = transaction.clone();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let mut db_tx = pool
                    .begin()
                    .await
                    .map_err(|e| StoreError::Database(e.to_string()))?;

                // Add credits
                let new_balance = sqlx::query_scalar::<_, i64>(
                    r#"
                    UPDATE accounts
                    SET balance_cents = balance_cents + $2,
                        lifetime_purchased_cents = lifetime_purchased_cents + $2,
                        updated_at = NOW()
                    WHERE user_id = $1
                    RETURNING balance_cents
                    "#,
                )
                .bind(user_id.as_uuid())
                .bind(amount_cents)
                .fetch_optional(&mut *db_tx)
                .await
                .map_err(|e| StoreError::Database(e.to_string()))?
                .ok_or(StoreError::NotFound {
                    entity: "account",
                    id: user_id.to_string(),
                })?;

                // Record transaction
                sqlx::query(
                    r#"
                    INSERT INTO credit_transactions (id, user_id, amount_cents, transaction_type,
                        balance_after_cents, description, metadata, created_at)
                    VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                    "#,
                )
                .bind(tx.id.to_string())
                .bind(tx.user_id.as_uuid())
                .bind(tx.amount_cents)
                .bind(
                    serde_json::to_string(&tx.transaction_type)
                        .unwrap_or_default()
                        .trim_matches('"'),
                )
                .bind(new_balance)
                .bind(&tx.description)
                .bind(&tx.metadata)
                .bind(tx.created_at)
                .execute(&mut *db_tx)
                .await
                .map_err(|e| StoreError::Database(e.to_string()))?;

                db_tx
                    .commit()
                    .await
                    .map_err(|e| StoreError::Database(e.to_string()))?;

                Ok(new_balance)
            })
        })
    }
}

// ---------------------------------------------------------------------------
// Row types for sqlx mapping
// ---------------------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct AccountRow {
    user_id: uuid::Uuid,
    balance_cents: i64,
    lifetime_purchased_cents: i64,
    lifetime_granted_cents: i64,
    lifetime_used_cents: i64,
    subscription: Option<serde_json::Value>,
    auto_refill: Option<serde_json::Value>,
    lago_customer_id: Option<String>,
    stripe_customer_id: Option<String>,
    is_zero_pro: bool,
    signup_grant_at: Option<chrono::DateTime<chrono::Utc>>,
    last_daily_grant_at: Option<chrono::DateTime<chrono::Utc>>,
    last_monthly_grant_at: Option<chrono::DateTime<chrono::Utc>>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl AccountRow {
    fn into_account(self) -> Account {
        Account {
            user_id: UserId::from_uuid(self.user_id),
            balance_cents: self.balance_cents,
            lifetime_purchased_cents: self.lifetime_purchased_cents,
            lifetime_granted_cents: self.lifetime_granted_cents,
            lifetime_used_cents: self.lifetime_used_cents,
            subscription: self
                .subscription
                .and_then(|v| serde_json::from_value(v).ok()),
            auto_refill: self
                .auto_refill
                .and_then(|v| serde_json::from_value(v).ok()),
            lago_customer_id: self.lago_customer_id,
            stripe_customer_id: self.stripe_customer_id,
            is_zero_pro: self.is_zero_pro,
            signup_grant_at: self.signup_grant_at,
            last_daily_grant_at: self.last_daily_grant_at,
            last_monthly_grant_at: self.last_monthly_grant_at,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct TransactionRow {
    id: String,
    user_id: uuid::Uuid,
    amount_cents: i64,
    transaction_type: String,
    balance_after_cents: i64,
    description: String,
    metadata: serde_json::Value,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl TransactionRow {
    fn into_transaction(self) -> CreditTransaction {
        CreditTransaction {
            id: self
                .id
                .parse::<TransactionId>()
                .unwrap_or_else(|_| TransactionId::generate()),
            user_id: UserId::from_uuid(self.user_id),
            amount_cents: self.amount_cents,
            transaction_type: serde_json::from_str(&format!("\"{}\"", self.transaction_type))
                .unwrap_or(z_billing_core::TransactionType::Purchase),
            balance_after_cents: self.balance_after_cents,
            description: self.description,
            metadata: self.metadata,
            created_at: self.created_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct UsageEventRow {
    event_id: String,
    user_id: uuid::Uuid,
    agent_id: Option<uuid::Uuid>,
    source: serde_json::Value,
    metric: serde_json::Value,
    quantity: f64,
    cost_cents: i64,
    event_timestamp: chrono::DateTime<chrono::Utc>,
    metadata: serde_json::Value,
}

impl UsageEventRow {
    fn into_usage_event(self) -> UsageEvent {
        UsageEvent {
            event_id: self.event_id,
            user_id: UserId::from_uuid(self.user_id),
            agent_id: self.agent_id.map(z_billing_core::AgentId::from_uuid),
            source: serde_json::from_value(self.source)
                .unwrap_or(z_billing_core::UsageSource::Custom("unknown".to_string())),
            metric: serde_json::from_value(self.metric).unwrap_or(
                z_billing_core::UsageMetric::ApiCalls {
                    endpoint: "unknown".to_string(),
                },
            ),
            quantity: self.quantity,
            cost_cents: self.cost_cents,
            timestamp: self.event_timestamp,
            metadata: self.metadata,
        }
    }
}
