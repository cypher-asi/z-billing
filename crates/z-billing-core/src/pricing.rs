//! Pricing configuration for z-billing.
//!
//! This module defines pricing for compute resources and LLM models.

use crate::account::Plan;
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

        // Anthropic models — current (Claude 4.x) at vendor/base rates
        let sonnet_pricing = LlmPricing {
            input_credits_per_million: 300,   // $3.00 per 1M input tokens
            output_credits_per_million: 1500, // $15.00 per 1M output tokens
        };
        let opus_pricing = LlmPricing {
            input_credits_per_million: 500,   // $5.00 per 1M
            output_credits_per_million: 2500, // $25.00 per 1M
        };
        let haiku_pricing = LlmPricing {
            input_credits_per_million: 100,  // $1.00 per 1M
            output_credits_per_million: 500, // $5.00 per 1M
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
        llm_pricing.insert(
            ModelKey::new("anthropic", "aura-claude-opus-4-7"),
            opus_pricing.clone(),
        );
        llm_pricing.insert(
            ModelKey::new("anthropic", "aura-claude-opus-4-6"),
            opus_pricing.clone(),
        );
        llm_pricing.insert(
            ModelKey::new("anthropic", "aura-claude-sonnet-4-6"),
            sonnet_pricing.clone(),
        );
        llm_pricing.insert(
            ModelKey::new("anthropic", "aura-claude-haiku-4-5"),
            haiku_pricing.clone(),
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
                input_credits_per_million: 25,
                output_credits_per_million: 125,
            },
        );
        llm_pricing.insert(
            ModelKey::new("anthropic", "claude-3-opus"),
            LlmPricing {
                input_credits_per_million: 1500,
                output_credits_per_million: 7500,
            },
        );

        // OpenAI models at vendor/base rates
        llm_pricing.insert(
            ModelKey::new("openai", "gpt-4o"),
            LlmPricing {
                input_credits_per_million: 250,
                output_credits_per_million: 1000,
            },
        );
        llm_pricing.insert(
            ModelKey::new("openai", "gpt-4o-mini"),
            LlmPricing {
                input_credits_per_million: 15,
                output_credits_per_million: 60,
            },
        );
        let gpt_5_4_pricing = LlmPricing {
            input_credits_per_million: 250,
            output_credits_per_million: 1500,
        };
        let gpt_5_4_mini_pricing = LlmPricing {
            input_credits_per_million: 75,
            output_credits_per_million: 450,
        };
        let gpt_5_4_nano_pricing = LlmPricing {
            input_credits_per_million: 20,
            output_credits_per_million: 125,
        };
        llm_pricing.insert(
            ModelKey::new("openai", "gpt-5.4"),
            gpt_5_4_pricing.clone(),
        );
        llm_pricing.insert(
            ModelKey::new("openai", "gpt-5.4-mini"),
            gpt_5_4_mini_pricing.clone(),
        );
        llm_pricing.insert(
            ModelKey::new("openai", "gpt-5.4-nano"),
            gpt_5_4_nano_pricing.clone(),
        );
        llm_pricing.insert(
            ModelKey::new("openai", "aura-gpt-5-4"),
            gpt_5_4_pricing,
        );
        llm_pricing.insert(
            ModelKey::new("openai", "aura-gpt-5-4-mini"),
            gpt_5_4_mini_pricing,
        );
        llm_pricing.insert(
            ModelKey::new("openai", "aura-gpt-5-4-nano"),
            gpt_5_4_nano_pricing,
        );

        // Google models at vendor/base rates
        llm_pricing.insert(
            ModelKey::new("google", "gemini-2.5-pro"),
            LlmPricing {
                input_credits_per_million: 125,
                output_credits_per_million: 1000,
            },
        );
        llm_pricing.insert(
            ModelKey::new("google", "gemini-2.5-flash"),
            LlmPricing {
                input_credits_per_million: 30,
                output_credits_per_million: 250,
            },
        );

        // Fireworks-hosted open-weight models at vendor/base rates.
        let deepseek_v3_2_pricing = LlmPricing {
            input_credits_per_million: 56,
            output_credits_per_million: 168,
        };
        let kimi_k2_pricing = LlmPricing {
            input_credits_per_million: 60,
            output_credits_per_million: 300,
        };
        let gpt_oss_120b_pricing = LlmPricing {
            input_credits_per_million: 15,
            output_credits_per_million: 60,
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
            kimi_k2_pricing.clone(),
        );
        llm_pricing.insert(
            ModelKey::new("fireworks", "accounts/fireworks/models/kimi-k2p5"),
            kimi_k2_pricing.clone(),
        );
        llm_pricing.insert(
            ModelKey::new("fireworks", "aura-kimi-k2-6"),
            kimi_k2_pricing.clone(),
        );
        llm_pricing.insert(
            ModelKey::new("fireworks", "accounts/fireworks/models/kimi-k2p6"),
            kimi_k2_pricing,
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
    fn llm_markup_percent(plan: &Plan) -> i64 {
        match plan {
            Plan::Pro => 10,
            Plan::Free | Plan::Standard | Plan::Enterprise => 20,
        }
    }

    fn marked_up_llm_pricing(&self, pricing: &LlmPricing, plan: &Plan) -> LlmPricing {
        let markup_percent = Self::llm_markup_percent(plan);
        LlmPricing {
            input_credits_per_million:
                (pricing.input_credits_per_million * (100 + markup_percent) + 50) / 100,
            output_credits_per_million:
                (pricing.output_credits_per_million * (100 + markup_percent) + 50) / 100,
        }
    }

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

    /// Calculate the cost in cents for LLM token usage after applying the plan markup.
    #[must_use]
    pub fn calculate_llm_cost_for_plan(
        &self,
        provider: &str,
        model: &str,
        input_tokens: u64,
        output_tokens: u64,
        plan: &Plan,
    ) -> i64 {
        let key = ModelKey::new(provider, model);
        let pricing = self
            .llm_pricing
            .get(&key)
            .unwrap_or(&self.default_llm_pricing);
        let marked_up_pricing = self.marked_up_llm_pricing(pricing, plan);

        let input_cost = (i64::try_from(input_tokens).unwrap_or(i64::MAX)
            * marked_up_pricing.input_credits_per_million)
            / 1_000_000;
        let output_cost = (i64::try_from(output_tokens).unwrap_or(i64::MAX)
            * marked_up_pricing.output_credits_per_million)
            / 1_000_000;
        let total = input_cost + output_cost;

        if total == 0 && (input_tokens > 0 || output_tokens > 0) {
            1
        } else {
            total
        }
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
            .contains_key(&ModelKey::new("openai", "aura-gpt-5-4")));
        assert!(config
            .llm_pricing
            .contains_key(&ModelKey::new("fireworks", "aura-kimi-k2-6")));
        assert!(config
            .llm_pricing
            .contains_key(&ModelKey::new("anthropic", "claude-3-5-sonnet")));
    }

    #[test]
    fn calculate_llm_cost_claude() {
        let config = PricingConfig::default();

        // Claude Sonnet 4.6: 300 credits/1M input, 1500 credits/1M output
        // 10,000 input tokens = 3 credits
        // 5,000 output tokens = 7.5 -> 7 credits
        let cost = config.calculate_llm_cost("anthropic", "claude-sonnet-4-6", 10_000, 5_000);
        assert_eq!(cost, 10); // 3 + 7 = 10
    }

    #[test]
    fn calculate_llm_cost_applies_plan_markup() {
        let config = PricingConfig::default();

        let free_cost = config.calculate_llm_cost_for_plan(
            "anthropic",
            "claude-sonnet-4-6",
            10_000,
            5_000,
            &Plan::Free,
        );
        let pro_cost = config.calculate_llm_cost_for_plan(
            "anthropic",
            "claude-sonnet-4-6",
            10_000,
            5_000,
            &Plan::Pro,
        );

        assert_eq!(free_cost, 12);
        assert_eq!(pro_cost, 11);
    }

    #[test]
    fn calculate_llm_cost_for_plan_matches_small_usage_rounding_behavior() {
        let config = PricingConfig::default();

        let free_cost = config.calculate_llm_cost_for_plan(
            "anthropic",
            "claude-sonnet-4-6",
            2_000,
            2_000,
            &Plan::Free,
        );

        assert_eq!(free_cost, 3);
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
