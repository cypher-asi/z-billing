# LLM cost observability

This telemetry separates three financial concepts that must not be treated as
the same metric:

1. **Billed cost** — credits actually debited from the AURA user.
2. **Estimated provider cost** — per-request list-price estimate derived from
   provider usage, including cache and supported server-tool details.
3. **Actual provider cost** — organization-level ledger cost returned by the
   provider's administrative cost API.

## Mixpanel events

### `tokens_consumed`

The existing event continues to represent one successfully debited usage
event. For requests reported by `aura-router`, it now includes:

| Property | Unit | Meaning |
|---|---:|---|
| `billed_cost_cents` | cents | AURA credits debited; same value as legacy `cost_cents` |
| `estimated_provider_cost_microusd` | $0.000001 | Provider list-price estimate before markup |
| `estimated_provider_cost_cents` | cents | Decimal-cent form of the same estimate |
| `estimated_gross_margin_microusd` | $0.000001 | Billed cost minus estimated provider cost |
| `estimated_gross_margin_percent` | percent | Estimated margin divided by billed cost |
| `markup_bps` | basis points | Configured markup used by the estimate |
| `uncached_input_tokens` | tokens | New input billed at the base input rate |
| `cache_creation_5m_input_tokens` | tokens | Five-minute cache writes |
| `cache_creation_1h_input_tokens` | tokens | One-hour cache writes |
| `cache_read_input_tokens` | tokens | Prompt-cache hits |
| `web_search_requests` | requests | Anthropic server-side searches billed separately |
| `web_fetch_requests` | requests | Anthropic server-side fetches (no separate request fee) |
| `code_execution_requests` | requests | Server-side code execution calls; runtime cost is reconciled at provider-ledger level |
| `service_tier` | string | Provider-reported standard, priority, or batch tier |
| `inference_geo` | string | Provider-reported inference geography |
| `speed` | string | Provider-reported standard or fast inference mode |
| `estimate_source` | string | `static_model_rates` or `provider_reported` |
| `pricing_source` | string | Versioned AURA pricing-table identifier |
| `org_id`, `project_id`, `agent_id` | string | Available AURA workload dimensions |

All cost metadata is allowlisted by z-billing and is accepted only from the
trusted `aura-router` service identity.

### `anthropic_provider_cost`

This event represents an authoritative line item from Anthropic's daily Cost
API, grouped by workspace and description. Important properties are:

| Property | Unit | Meaning |
|---|---:|---|
| `actual_provider_cost_cents` | cents | Fractional-cent amount from Anthropic |
| `actual_provider_cost_microusd` | $0.000001 | Same amount normalized for exact summation |
| `model`, `token_type`, `cost_type` | string | Anthropic ledger dimensions |
| `workspace_id`, `service_tier`, `context_window`, `inference_geo`, `speed` | string | Provider dimensions when available |
| `provider_cost_source` | string | Always `anthropic_cost_api` |
| `is_authoritative_provider_cost` | boolean | Always true |

The sync reads the last three completed UTC days once per day. Stable
`$insert_id` values prevent service restarts from double-counting line items.

Enable it by setting both:

```text
MIXPANEL_PROJECT_TOKEN=...
ANTHROPIC_ADMIN_API_KEY=sk-ant-admin01-...
```

The regular Anthropic inference key cannot access the administrative cost
endpoint.

## Recommended Mixpanel board

Use UTC day buckets and add these reports:

1. **Actual vs estimated vs billed**
   - Sum `anthropic_provider_cost.actual_provider_cost_cents`.
   - Sum `tokens_consumed.estimated_provider_cost_cents`, filtered to
     `provider = anthropic`.
   - Sum `tokens_consumed.billed_cost_cents`, filtered to
     `provider = anthropic`.
2. **Estimated gross margin**
   - Sum `estimated_gross_margin_cents` and weighted margin
     `sum(margin) / sum(billed cost)`.
3. **Reconciliation gap**
   - Actual provider cost minus estimated provider cost, with an alert when the
     absolute daily gap exceeds 10%.
4. **Cost drivers**
   - Estimated cost by model, user (`distinct_id`), org, project, service tier,
     and inference geography.
5. **Cache economics**
   - Cache reads, five-minute writes, and one-hour writes by model; monitor the
     cache-read share of total input.
6. **Coverage guardrails**
   - Percentage of Anthropic `tokens_consumed` events where
     `has_provider_cost_estimate = true`.
   - Priority-tier request count. Anthropic's standard Cost API excludes
     Priority Tier costs, so any non-zero value requires a separate review.
   - Fast-mode request count. Opus fast mode uses premium token rates; the
     request estimate applies the provider-reported speed multiplier.
   - `cost_type = code_execution` in the provider ledger. Runtime-based code
     execution cannot currently be allocated precisely to an AURA user.

## Interpretation

Use actual provider cost for cash reconciliation and organization-wide margin.
Use estimated provider cost for user, model, org, and project attribution.
Never label a per-user estimate as exact provider spend: Anthropic's standard
Cost API is an organization/workspace ledger and does not expose AURA's user
identifier at that grain.

Claude Sonnet 5 uses Anthropic's introductory $2/$10 per-million input/output
pricing through August 31, 2026. Both request estimation and z-billing switch
automatically to the published $3/$15 standard rate on September 1, 2026.

Provider references:

- [Usage and Cost API](https://platform.claude.com/docs/en/manage-claude/usage-cost-api)
- [Cost Report API](https://platform.claude.com/docs/en/api/admin/cost_report)
- [Claude pricing](https://platform.claude.com/docs/en/about-claude/pricing)
- [Fast mode](https://platform.claude.com/docs/en/build-with-claude/fast-mode)
