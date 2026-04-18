//! Pricing configuration for z-billing.
//!
//! This module defines pricing for compute resources and LLM models.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Pricing configuration for all billable resources.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PricingConfig {
    /// Z Credit exchange rate in USD (0.01 = 1 Z Credit = $0.01).
    pub z_credit_rate_usd: f64,

    /// Cost per CPU hour in Z Credits.
    pub cpu_hour_credits: i64,

    /// Cost per GB-hour of memory in Z Credits.
    pub memory_gb_hour_credits: i64,

    /// LLM pricing by provider and model.
    pub llm_pricing: HashMap<ModelKey, LlmPricing>,

    /// Default LLM pricing for unknown models.
    pub default_llm_pricing: LlmPricing,
}

impl Default for PricingConfig {
    fn default() -> Self {
        let mut llm_pricing = HashMap::new();

        // Anthropic models — current (Claude 4.x), priced at provider cost +20%
        let sonnet_pricing = LlmPricing {
            input_credits_per_million: 360,   // $3.60 per 1M input tokens
            output_credits_per_million: 1800, // $18.00 per 1M output tokens
        };
        let opus_pricing = LlmPricing {
            input_credits_per_million: 600,   // $6.00 per 1M
            output_credits_per_million: 3000, // $30.00 per 1M
        };
        let haiku_pricing = LlmPricing {
            input_credits_per_million: 120,  // $1.20 per 1M
            output_credits_per_million: 600, // $6.00 per 1M
        };

        // Current model IDs
        llm_pricing.insert(
            ModelKey::new("anthropic", "claude-sonnet-4-6"),
            sonnet_pricing.clone(),
        );
        llm_pricing.insert(
            ModelKey::new("anthropic", "claude-opus-4-6"),
            opus_pricing.clone(),
        );
        llm_pricing.insert(
            ModelKey::new("anthropic", "claude-opus-4-7"),
            opus_pricing.clone(),
        );
        llm_pricing.insert(
            ModelKey::new("anthropic", "claude-haiku-4-5-20251001"),
            haiku_pricing.clone(),
        );
        llm_pricing.insert(
            ModelKey::new("anthropic", "claude-haiku-4-5"),
            haiku_pricing.clone(),
        );

        // Aura-managed Anthropic aliases (+20% premium over vendor base rates)
        let aura_sonnet_pricing = LlmPricing {
            input_credits_per_million: 360,
            output_credits_per_million: 1800,
        };
        let aura_opus_pricing = LlmPricing {
            input_credits_per_million: 600,
            output_credits_per_million: 3000,
        };
        let aura_haiku_pricing = LlmPricing {
            input_credits_per_million: 120,
            output_credits_per_million: 600,
        };
        llm_pricing.insert(
            ModelKey::new("anthropic", "aura-claude-opus-4-7"),
            aura_opus_pricing.clone(),
        );
        llm_pricing.insert(
            ModelKey::new("anthropic", "aura-claude-opus-4-6"),
            aura_opus_pricing.clone(),
        );
        llm_pricing.insert(
            ModelKey::new("anthropic", "aura-claude-sonnet-4-6"),
            aura_sonnet_pricing.clone(),
        );
        llm_pricing.insert(
            ModelKey::new("anthropic", "aura-claude-haiku-4-5"),
            aura_haiku_pricing.clone(),
        );

        // Legacy model IDs (backward compatibility)
        llm_pricing.insert(
            ModelKey::new("anthropic", "claude-3-5-sonnet"),
            sonnet_pricing.clone(),
        );
        llm_pricing.insert(
            ModelKey::new("anthropic", "claude-3-5-sonnet-20241022"),
            sonnet_pricing,
        );
        llm_pricing.insert(
            ModelKey::new("anthropic", "claude-3-haiku"),
            LlmPricing {
                input_credits_per_million: 30,
                output_credits_per_million: 150,
            },
        );
        llm_pricing.insert(
            ModelKey::new("anthropic", "claude-3-opus"),
            LlmPricing {
                input_credits_per_million: 1800,
                output_credits_per_million: 9000,
            },
        );

        // OpenAI models, priced at provider cost +20%
        llm_pricing.insert(
            ModelKey::new("openai", "gpt-4o"),
            LlmPricing {
                input_credits_per_million: 300,
                output_credits_per_million: 1200,
            },
        );
        llm_pricing.insert(
            ModelKey::new("openai", "gpt-4o-mini"),
            LlmPricing {
                input_credits_per_million: 18,
                output_credits_per_million: 72,
            },
        );
        let gpt_5_4_pricing = LlmPricing {
            input_credits_per_million: 300,
            output_credits_per_million: 1800,
        };
        let gpt_5_4_mini_pricing = LlmPricing {
            input_credits_per_million: 90,
            output_credits_per_million: 540,
        };
        let gpt_5_4_nano_pricing = LlmPricing {
            input_credits_per_million: 24,
            output_credits_per_million: 150,
        };
        llm_pricing.insert(ModelKey::new("openai", "gpt-5.4"), gpt_5_4_pricing.clone());
        llm_pricing.insert(
            ModelKey::new("openai", "gpt-5.4-mini"),
            gpt_5_4_mini_pricing.clone(),
        );
        llm_pricing.insert(
            ModelKey::new("openai", "gpt-5.4-nano"),
            gpt_5_4_nano_pricing.clone(),
        );

        // Aura-managed OpenAI aliases
        llm_pricing.insert(ModelKey::new("openai", "aura-gpt-5-4"), gpt_5_4_pricing);
        llm_pricing.insert(
            ModelKey::new("openai", "aura-gpt-5-4-mini"),
            gpt_5_4_mini_pricing,
        );
        llm_pricing.insert(
            ModelKey::new("openai", "aura-gpt-5-4-nano"),
            gpt_5_4_nano_pricing,
        );

        // Google models, corrected to current vendor pricing then marked up +20%
        llm_pricing.insert(
            ModelKey::new("google", "gemini-2.5-pro"),
            LlmPricing {
                input_credits_per_million: 150,
                output_credits_per_million: 1200,
            },
        );
        llm_pricing.insert(
            ModelKey::new("google", "gemini-2.5-flash"),
            LlmPricing {
                input_credits_per_million: 36,
                output_credits_per_million: 300,
            },
        );

        // Fireworks models, priced at provider cost +20%
        let deepseek_v3_2_pricing = LlmPricing {
            input_credits_per_million: 67,
            output_credits_per_million: 202,
        };
        let kimi_k2_5_pricing = LlmPricing {
            input_credits_per_million: 72,
            output_credits_per_million: 360,
        };
        let gpt_oss_120b_pricing = LlmPricing {
            input_credits_per_million: 18,
            output_credits_per_million: 72,
        };
        llm_pricing.insert(
            ModelKey::new("fireworks", "aura-deepseek-v3-2"),
            deepseek_v3_2_pricing.clone(),
        );
        llm_pricing.insert(
            ModelKey::new("fireworks", "accounts/fireworks/models/deepseek-v3p2"),
            deepseek_v3_2_pricing,
        );
        llm_pricing.insert(
            ModelKey::new("fireworks", "aura-kimi-k2-5"),
            kimi_k2_5_pricing.clone(),
        );
        llm_pricing.insert(
            ModelKey::new("fireworks", "accounts/fireworks/models/kimi-k2p5"),
            kimi_k2_5_pricing,
        );
        llm_pricing.insert(
            ModelKey::new("fireworks", "aura-oss-120b"),
            gpt_oss_120b_pricing.clone(),
        );
        llm_pricing.insert(
            ModelKey::new("fireworks", "accounts/fireworks/models/gpt-oss-120b"),
            gpt_oss_120b_pricing,
        );

        Self {
            z_credit_rate_usd: 0.01,
            cpu_hour_credits: 6,       // $0.06 per CPU hour
            memory_gb_hour_credits: 2, // $0.02 per GB-hour
            llm_pricing,
            default_llm_pricing: LlmPricing {
                input_credits_per_million: 100,  // Default $1.00 per 1M
                output_credits_per_million: 300, // Default $3.00 per 1M
            },
        }
    }
}

impl PricingConfig {
    /// Calculate the cost in cents for LLM token usage.
    ///
    /// Minimum cost is 1 credit for any non-zero usage.
    #[must_use]
    pub fn calculate_llm_cost(
        &self,
        provider: &str,
        model: &str,
        input_tokens: u64,
        output_tokens: u64,
    ) -> i64 {
        let key = ModelKey::new(provider, model);
        let pricing = self
            .llm_pricing
            .get(&key)
            .unwrap_or(&self.default_llm_pricing);

        let input_cost = (i64::try_from(input_tokens).unwrap_or(i64::MAX)
            * pricing.input_credits_per_million)
            / 1_000_000;
        let output_cost = (i64::try_from(output_tokens).unwrap_or(i64::MAX)
            * pricing.output_credits_per_million)
            / 1_000_000;

        let total = input_cost + output_cost;

        // Minimum 1 credit for any non-zero usage
        if total == 0 && (input_tokens > 0 || output_tokens > 0) {
            1
        } else {
            total
        }
    }

    /// Calculate the minimum balance reserve in cents for starting a short text turn.
    ///
    /// This uses the same pricing table as final billing so preflight checks stay aligned.
    /// The reserve assumes a short turn of roughly 2k input + 1k output tokens.
    #[must_use]
    pub fn minimum_llm_reserve_cents(&self, provider: &str, model: &str) -> i64 {
        let key = ModelKey::new(provider, model);
        let pricing = self
            .llm_pricing
            .get(&key)
            .unwrap_or(&self.default_llm_pricing);

        let reserve_numerator = 2_000_i64 * pricing.input_credits_per_million
            + 1_000_i64 * pricing.output_credits_per_million;
        ((reserve_numerator + 999_999) / 1_000_000).max(1)
    }

    /// Calculate the cost in cents for compute usage.
    #[must_use]
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    pub fn calculate_compute_cost(&self, cpu_hours: f64, memory_gb_hours: f64) -> i64 {
        let cpu_cost = (cpu_hours * self.cpu_hour_credits as f64).round() as i64;
        let memory_cost = (memory_gb_hours * self.memory_gb_hour_credits as f64).round() as i64;

        let total = cpu_cost + memory_cost;

        // Minimum 1 credit for any non-zero usage
        if total == 0 && (cpu_hours > 0.0 || memory_gb_hours > 0.0) {
            1
        } else {
            total
        }
    }

    /// Convert USD to Z Credits.
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub fn usd_to_credits(&self, usd: f64) -> i64 {
        (usd / self.z_credit_rate_usd).round() as i64
    }

    /// Convert Z Credits to USD.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn credits_to_usd(&self, credits: i64) -> f64 {
        credits as f64 * self.z_credit_rate_usd
    }
}

/// Key for looking up LLM model pricing.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ModelKey {
    /// Provider name (e.g., "anthropic", "openai").
    pub provider: String,
    /// Model name (e.g., "claude-3-5-sonnet", "gpt-4-turbo").
    pub model: String,
}

impl ModelKey {
    /// Create a new model key.
    #[must_use]
    pub fn new(provider: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            model: model.into(),
        }
    }
}

/// Pricing for an LLM model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmPricing {
    /// Credits per 1 million input tokens.
    pub input_credits_per_million: i64,
    /// Credits per 1 million output tokens.
    pub output_credits_per_million: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_pricing_config() {
        let config = PricingConfig::default();
        assert_eq!(config.z_credit_rate_usd, 0.01);
        assert!(config
            .llm_pricing
            .contains_key(&ModelKey::new("anthropic", "claude-sonnet-4-6")));
        assert!(config
            .llm_pricing
            .contains_key(&ModelKey::new("anthropic", "claude-opus-4-6")));
        assert!(config
            .llm_pricing
            .contains_key(&ModelKey::new("anthropic", "aura-claude-opus-4-7")));
        assert!(config
            .llm_pricing
            .contains_key(&ModelKey::new("openai", "aura-gpt-5-4-mini")));
        assert!(config
            .llm_pricing
            .contains_key(&ModelKey::new("fireworks", "aura-oss-120b")));
        // Legacy models still present
        assert!(config
            .llm_pricing
            .contains_key(&ModelKey::new("anthropic", "claude-3-5-sonnet")));
    }

    #[test]
    fn calculate_llm_cost_claude() {
        let config = PricingConfig::default();

        // Claude Sonnet 4.6: 360 credits/1M input, 1800 credits/1M output
        // 10,000 input tokens = 3.6 -> 3 credits
        // 5,000 output tokens = 9 credits
        let cost = config.calculate_llm_cost("anthropic", "claude-sonnet-4-6", 10_000, 5_000);
        assert_eq!(cost, 12); // 3 + 9 = 12
    }

    #[test]
    fn calculate_llm_cost_small_usage_minimum() {
        let config = PricingConfig::default();

        // Very small usage should still cost at least 1 credit
        let cost = config.calculate_llm_cost("anthropic", "claude-sonnet-4-6", 100, 50);
        assert_eq!(cost, 1);
    }

    #[test]
    fn calculate_llm_cost_unknown_model() {
        let config = PricingConfig::default();

        // Unknown model uses default pricing
        let cost = config.calculate_llm_cost("unknown", "mystery-model", 1_000_000, 0);
        assert_eq!(cost, 100); // Default input: 100 credits/1M
    }

    #[test]
    fn minimum_llm_reserve_uses_same_pricing_table() {
        let config = PricingConfig::default();

        assert_eq!(
            config.minimum_llm_reserve_cents("anthropic", "aura-claude-opus-4-7"),
            5
        );
        assert_eq!(
            config.minimum_llm_reserve_cents("openai", "aura-gpt-5-4"),
            3
        );
        assert_eq!(
            config.minimum_llm_reserve_cents("fireworks", "aura-oss-120b"),
            1
        );
    }

    #[test]
    fn calculate_compute_cost() {
        let config = PricingConfig::default();

        // 2 CPU hours at 6 credits/hour = 12 credits
        // 4 GB-hours at 2 credits/GB-hour = 8 credits
        let cost = config.calculate_compute_cost(2.0, 4.0);
        assert_eq!(cost, 20);
    }

    #[test]
    fn usd_to_credits_conversion() {
        let config = PricingConfig::default();

        // $50 = 5000 credits at 0.01 rate
        assert_eq!(config.usd_to_credits(50.0), 5000);
        assert_eq!(config.usd_to_credits(1.0), 100);
    }

    #[test]
    fn credits_to_usd_conversion() {
        let config = PricingConfig::default();

        // 5000 credits = $50 at 0.01 rate
        assert!((config.credits_to_usd(5000) - 50.0).abs() < 0.001);
    }
}
