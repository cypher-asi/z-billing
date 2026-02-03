# Z-Billing v0.1.0 - Pricing Configuration

This document specifies the pricing configuration for all billable resources.

## Overview

Pricing is centralized in `PricingConfig`, which defines:

- Z Credit exchange rate (1 Z Credit = $0.01)
- LLM token pricing by provider and model
- Compute resource pricing (CPU, memory)
- Default pricing for unknown models

## Z Credit Unit

**1 Z Credit = $0.01 (1 cent)**

All prices and balances are stored as `i64` integers representing cents to avoid floating-point precision issues.

| USD Amount | Z Credits |
|------------|-----------|
| $1.00      | 100       |
| $5.00      | 500       |
| $50.00     | 5,000     |
| $100.00    | 10,000    |

## PricingConfig

### Structure

```rust
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
```

### Default Configuration

```rust
PricingConfig {
    z_credit_rate_usd: 0.01,        // 1 credit = $0.01
    cpu_hour_credits: 6,            // $0.06 per CPU hour
    memory_gb_hour_credits: 2,      // $0.02 per GB-hour
    llm_pricing: /* see below */,
    default_llm_pricing: LlmPricing {
        input_credits_per_million: 100,   // $1.00 per 1M input tokens
        output_credits_per_million: 300,  // $3.00 per 1M output tokens
    },
}
```

## LLM Pricing

### ModelKey

```rust
pub struct ModelKey {
    pub provider: String,  // e.g., "anthropic", "openai", "google"
    pub model: String,     // e.g., "claude-3-5-sonnet", "gpt-4o"
}
```

### LlmPricing

```rust
pub struct LlmPricing {
    /// Credits per 1 million input tokens.
    pub input_credits_per_million: i64,
    /// Credits per 1 million output tokens.
    pub output_credits_per_million: i64,
}
```

### Model Price Table

| Provider   | Model                        | Input (per 1M) | Output (per 1M) | USD Input | USD Output |
|------------|------------------------------|----------------|-----------------|-----------|------------|
| Anthropic  | claude-3-5-sonnet            | 300            | 1,500           | $3.00     | $15.00     |
| Anthropic  | claude-3-5-sonnet-20241022   | 300            | 1,500           | $3.00     | $15.00     |
| Anthropic  | claude-3-haiku               | 25             | 125             | $0.25     | $1.25      |
| Anthropic  | claude-3-opus                | 1,500          | 7,500           | $15.00    | $75.00     |
| OpenAI     | gpt-4-turbo                  | 1,000          | 3,000           | $10.00    | $30.00     |
| OpenAI     | gpt-4o                       | 250            | 1,000           | $2.50     | $10.00     |
| OpenAI     | gpt-4o-mini                  | 15             | 60              | $0.15     | $0.60      |
| Google     | gemini-1.5-pro               | 125            | 500             | $1.25     | $5.00      |
| Google     | gemini-1.5-flash             | 8              | 30              | $0.08     | $0.30      |
| (default)  | unknown models               | 100            | 300             | $1.00     | $3.00      |

## Cost Calculation

### LLM Token Cost

**Formula:**

```
input_cost = (input_tokens * input_credits_per_million) / 1,000,000
output_cost = (output_tokens * output_credits_per_million) / 1,000,000
total_cost = max(input_cost + output_cost, 1)  // minimum 1 credit
```

**Minimum Cost Rule:** Any non-zero usage costs at least 1 credit.

```rust
pub fn calculate_llm_cost(
    &self,
    provider: &str,
    model: &str,
    input_tokens: u64,
    output_tokens: u64,
) -> i64 {
    let key = ModelKey::new(provider, model);
    let pricing = self.llm_pricing.get(&key)
        .unwrap_or(&self.default_llm_pricing);

    let input_cost = (input_tokens * pricing.input_credits_per_million) / 1_000_000;
    let output_cost = (output_tokens * pricing.output_credits_per_million) / 1_000_000;
    let total = input_cost + output_cost;

    // Minimum 1 credit for any non-zero usage
    if total == 0 && (input_tokens > 0 || output_tokens > 0) {
        1
    } else {
        total
    }
}
```

### Examples

| Provider  | Model              | Input Tokens | Output Tokens | Cost (credits) | Cost (USD) |
|-----------|--------------------|--------------|---------------|----------------|------------|
| Anthropic | claude-3-5-sonnet  | 10,000       | 5,000         | 10             | $0.10      |
| Anthropic | claude-3-5-sonnet  | 100          | 50            | 1 (minimum)    | $0.01      |
| OpenAI    | gpt-4o             | 1,000,000    | 0             | 250            | $2.50      |
| Google    | gemini-1.5-flash   | 500,000      | 100,000       | 7              | $0.07      |
| Unknown   | mystery-model      | 1,000,000    | 0             | 100            | $1.00      |

### Compute Cost

**Formula:**

```
cpu_cost = round(cpu_hours * cpu_hour_credits)
memory_cost = round(memory_gb_hours * memory_gb_hour_credits)
total_cost = max(cpu_cost + memory_cost, 1)  // minimum 1 credit
```

```rust
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
```

### Example Compute Costs

| CPU Hours | Memory GB-Hours | CPU Cost | Memory Cost | Total | USD   |
|-----------|-----------------|----------|-------------|-------|-------|
| 2.0       | 4.0             | 12       | 8           | 20    | $0.20 |
| 0.5       | 1.0             | 3        | 2           | 5     | $0.05 |
| 0.01      | 0.01            | 0        | 0           | 1     | $0.01 |

## Currency Conversion

```rust
/// Convert USD to Z Credits.
pub fn usd_to_credits(&self, usd: f64) -> i64 {
    (usd / self.z_credit_rate_usd).round() as i64
}

/// Convert Z Credits to USD.
pub fn credits_to_usd(&self, credits: i64) -> f64 {
    credits as f64 * self.z_credit_rate_usd
}
```

### Examples

| USD     | Z Credits |
|---------|-----------|
| $50.00  | 5,000     |
| $1.00   | 100       |
| $0.01   | 1         |

## Plan Discounts

Subscription plans provide discounts on credit purchases:

| Plan       | Discount |
|------------|----------|
| Free       | 0%       |
| Standard   | 10%      |
| Pro        | 20%      |
| Enterprise | Custom   |

**Example:** User on Pro plan buys $50 of credits:
- Original: $50.00 → 5,000 credits
- With 20% discount: Pays $40.00 → Gets 5,000 credits

## Pricing Update Flow

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         Pricing Configuration Flow                          │
└─────────────────────────────────────────────────────────────────────────────┘

  ┌──────────────────┐
  │  Service Startup │
  └────────┬─────────┘
           │
           ▼
  ┌──────────────────┐
  │ Load default     │
  │ PricingConfig    │
  └────────┬─────────┘
           │
           ▼
  ┌──────────────────┐      ┌──────────────────┐
  │ Usage Request    │─────▶│ Calculate Cost   │
  │ (LLM/Compute)    │      │ using config     │
  └──────────────────┘      └────────┬─────────┘
                                     │
                                     ▼
                            ┌──────────────────┐
                            │ Look up model    │
                            │ pricing or use   │
                            │ default          │
                            └────────┬─────────┘
                                     │
                                     ▼
                            ┌──────────────────┐
                            │ Apply formula    │
                            │ with minimum     │
                            │ 1 credit rule    │
                            └────────┬─────────┘
                                     │
                                     ▼
                            ┌──────────────────┐
                            │ Return cost in   │
                            │ Z Credits (cents)│
                            └──────────────────┘
```

## Future Considerations

- Dynamic pricing updates via configuration reload
- Volume discounts for high-usage customers
- Time-of-day pricing for compute resources
- Custom enterprise pricing tiers
