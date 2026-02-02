//! Usage event types for z-billing.
//!
//! This module defines usage events that services report to z-billing.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{AgentId, UserId};

/// A usage event reported by a service.
///
/// Services like aura-runtime report usage events to z-billing,
/// which deducts credits and forwards to Lago for analytics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageEvent {
    /// Unique event ID for idempotency.
    pub event_id: String,

    /// The user being charged.
    pub user_id: UserId,

    /// The agent that generated this usage, if applicable.
    pub agent_id: Option<AgentId>,

    /// Which service reported this usage.
    pub source: UsageSource,

    /// What was used.
    pub metric: UsageMetric,

    /// Quantity used (tokens, hours, etc.).
    pub quantity: f64,

    /// Pre-calculated cost in cents based on current pricing.
    pub cost_cents: i64,

    /// When the usage occurred.
    pub timestamp: DateTime<Utc>,

    /// Additional context (`session_id`, `request_id`, etc.).
    pub metadata: serde_json::Value,
}

impl UsageEvent {
    /// Create a new LLM usage event.
    #[must_use]
    #[allow(clippy::too_many_arguments, clippy::cast_precision_loss)]
    pub fn llm(
        event_id: String,
        user_id: UserId,
        agent_id: Option<AgentId>,
        provider: LlmProvider,
        model: String,
        direction: TokenDirection,
        tokens: u64,
        cost_cents: i64,
    ) -> Self {
        Self {
            event_id,
            user_id,
            agent_id,
            source: UsageSource::AuraRuntime,
            metric: UsageMetric::LlmTokens {
                provider,
                model,
                direction,
            },
            quantity: tokens as f64,
            cost_cents,
            timestamp: Utc::now(),
            metadata: serde_json::Value::Null,
        }
    }

    /// Create a new compute usage event.
    #[must_use]
    pub fn compute(
        event_id: String,
        user_id: UserId,
        agent_id: Option<AgentId>,
        cpu_hours: f64,
        memory_gb_hours: f64,
        cost_cents: i64,
    ) -> Self {
        Self {
            event_id,
            user_id,
            agent_id,
            source: UsageSource::AuraSwarm,
            metric: UsageMetric::Compute {
                cpu_hours,
                memory_gb_hours,
            },
            quantity: cpu_hours,
            cost_cents,
            timestamp: Utc::now(),
            metadata: serde_json::Value::Null,
        }
    }

    /// Set metadata on the event.
    #[must_use]
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = metadata;
        self
    }

    /// Set the source service.
    #[must_use]
    pub fn with_source(mut self, source: UsageSource) -> Self {
        self.source = source;
        self
    }
}

/// Source service that generated the usage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageSource {
    /// Aura Swarm agent orchestration.
    AuraSwarm,

    /// Aura Runtime code execution.
    AuraRuntime,

    /// Custom service.
    Custom(String),
}

impl UsageSource {
    /// Get the source name as a string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::AuraSwarm => "aura_swarm",
            Self::AuraRuntime => "aura_runtime",
            Self::Custom(name) => name,
        }
    }
}

/// What was used (metric type).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum UsageMetric {
    /// Compute resources (CPU and memory).
    Compute {
        /// CPU hours used.
        cpu_hours: f64,
        /// Memory GB-hours used.
        memory_gb_hours: f64,
    },

    /// LLM token usage.
    LlmTokens {
        /// Which LLM provider.
        provider: LlmProvider,
        /// Model name (e.g., "claude-3-5-sonnet").
        model: String,
        /// Input or output tokens.
        direction: TokenDirection,
    },

    /// API calls.
    ApiCalls {
        /// Endpoint or operation name.
        endpoint: String,
    },

    /// Storage usage.
    Storage {
        /// GB-hours of storage used.
        gb_hours: f64,
    },
}

/// LLM provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LlmProvider {
    /// Anthropic (Claude models).
    Anthropic,

    /// `OpenAI` (GPT models).
    OpenAi,

    /// Google (Gemini models).
    Google,

    /// Custom provider.
    Custom(String),
}

impl LlmProvider {
    /// Get the provider name as a string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Anthropic => "anthropic",
            Self::OpenAi => "openai",
            Self::Google => "google",
            Self::Custom(name) => name,
        }
    }
}

/// Token direction (input or output).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenDirection {
    /// Input tokens (prompt).
    Input,
    /// Output tokens (completion).
    Output,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn llm_usage_event() {
        let user_id = UserId::generate();
        let event = UsageEvent::llm(
            "evt_123".to_string(),
            user_id,
            None,
            LlmProvider::Anthropic,
            "claude-3-5-sonnet".to_string(),
            TokenDirection::Output,
            1500,
            15,
        );

        assert_eq!(event.event_id, "evt_123");
        assert_eq!(event.quantity, 1500.0);
        assert_eq!(event.cost_cents, 15);
        assert!(matches!(event.metric, UsageMetric::LlmTokens { .. }));
    }

    #[test]
    fn compute_usage_event() {
        let user_id = UserId::generate();
        let agent_id = AgentId::generate();
        let event =
            UsageEvent::compute("evt_456".to_string(), user_id, Some(agent_id), 2.5, 4.0, 25);

        assert_eq!(event.event_id, "evt_456");
        assert_eq!(event.cost_cents, 25);
        assert!(event.agent_id.is_some());
    }

    #[test]
    fn usage_source_as_str() {
        assert_eq!(UsageSource::AuraSwarm.as_str(), "aura_swarm");
        assert_eq!(UsageSource::AuraRuntime.as_str(), "aura_runtime");
        assert_eq!(
            UsageSource::Custom("my-service".into()).as_str(),
            "my-service"
        );
    }
}
