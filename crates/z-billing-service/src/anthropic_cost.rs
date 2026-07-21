//! Authoritative Anthropic provider-cost reconciliation for Mixpanel.

use chrono::{Days, NaiveDate, SecondsFormat, Utc};
use serde::Deserialize;

use crate::{config::ServiceConfig, mixpanel};

const ANTHROPIC_API_BASE_URL: &str = "https://api.anthropic.com";
const ANTHROPIC_VERSION: &str = "2023-06-01";
const LOOKBACK_DAYS: u64 = 3;
const SYNC_INTERVAL: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);

#[derive(Debug, Deserialize)]
struct CostReport {
    data: Vec<CostBucket>,
    has_more: bool,
    next_page: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CostBucket {
    starting_at: String,
    ending_at: String,
    results: Vec<CostLine>,
}

#[derive(Debug, Clone, Deserialize)]
struct CostLine {
    amount: String,
    currency: String,
    workspace_id: Option<String>,
    description: Option<String>,
    cost_type: Option<String>,
    model: Option<String>,
    token_type: Option<String>,
    service_tier: Option<String>,
    context_window: Option<String>,
    inference_geo: Option<String>,
    speed: Option<String>,
}

#[derive(Debug)]
struct DatedCostLine {
    starting_at: String,
    ending_at: String,
    line: CostLine,
}

/// Start the optional daily Anthropic cost sync.
///
/// The task is enabled only when both `ANTHROPIC_ADMIN_API_KEY` and
/// `MIXPANEL_PROJECT_TOKEN` are configured. It imports the last three
/// completed UTC days using stable Mixpanel insert IDs, so restarts do not
/// double-count provider ledger rows.
pub fn spawn_daily_sync(config: &ServiceConfig) {
    let Some(admin_key) = config.anthropic_admin_api_key.clone() else {
        tracing::info!("Anthropic cost sync disabled - ANTHROPIC_ADMIN_API_KEY is not set");
        return;
    };
    let Some(mixpanel_token) = config.mixpanel_token.clone() else {
        tracing::warn!("Anthropic cost sync disabled - MIXPANEL_PROJECT_TOKEN is not set");
        return;
    };

    tokio::spawn(async move {
        let client = reqwest::Client::new();
        loop {
            sync_recent_days(&client, ANTHROPIC_API_BASE_URL, &admin_key, &mixpanel_token).await;
            tokio::time::sleep(SYNC_INTERVAL).await;
        }
    });
}

async fn sync_recent_days(
    client: &reqwest::Client,
    api_base_url: &str,
    admin_key: &str,
    mixpanel_token: &str,
) {
    let today = Utc::now().date_naive();
    for days_ago in 1..=LOOKBACK_DAYS {
        let Some(date) = today.checked_sub_days(Days::new(days_ago)) else {
            continue;
        };
        match fetch_cost_lines(client, api_base_url, admin_key, date).await {
            Ok(lines) => {
                let count = lines.len();
                for line in &lines {
                    track_cost_line(mixpanel_token, line);
                }
                tracing::info!(date = %date, rows = count, "Synced Anthropic provider cost");
            }
            Err(error) => {
                tracing::warn!(date = %date, error = %error, "Anthropic provider cost sync failed");
            }
        }
    }
}

async fn fetch_cost_lines(
    client: &reqwest::Client,
    api_base_url: &str,
    admin_key: &str,
    date: NaiveDate,
) -> Result<Vec<DatedCostLine>, String> {
    let next_date = date
        .checked_add_days(Days::new(1))
        .ok_or_else(|| "invalid cost report date".to_string())?;
    let starting_at = date
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| "invalid cost report start time".to_string())?
        .and_utc()
        .to_rfc3339_opts(SecondsFormat::Secs, true);
    let ending_at = next_date
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| "invalid cost report end time".to_string())?
        .and_utc()
        .to_rfc3339_opts(SecondsFormat::Secs, true);
    let url = format!("{api_base_url}/v1/organizations/cost_report");
    let mut page: Option<String> = None;
    let mut lines = Vec::new();

    loop {
        let mut query = vec![
            ("starting_at", starting_at.clone()),
            ("ending_at", ending_at.clone()),
            ("bucket_width", "1d".to_string()),
            ("group_by[]", "workspace_id".to_string()),
            ("group_by[]", "description".to_string()),
            ("limit", "31".to_string()),
        ];
        if let Some(cursor) = page.as_ref() {
            query.push(("page", cursor.clone()));
        }

        let response = client
            .get(&url)
            .header("x-api-key", admin_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header(reqwest::header::USER_AGENT, "AURA-z-billing/1.0")
            .query(&query)
            .send()
            .await
            .map_err(|error| format!("request failed: {error}"))?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("Anthropic returned {status}: {body}"));
        }
        let report = response
            .json::<CostReport>()
            .await
            .map_err(|error| format!("invalid response: {error}"))?;

        for bucket in report.data {
            for line in bucket.results {
                lines.push(DatedCostLine {
                    starting_at: bucket.starting_at.clone(),
                    ending_at: bucket.ending_at.clone(),
                    line,
                });
            }
        }
        if !report.has_more {
            break;
        }
        page = report.next_page;
        if page.is_none() {
            return Err("Anthropic reported more pages without a cursor".to_string());
        }
    }

    Ok(lines)
}

#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
fn track_cost_line(mixpanel_token: &str, cost: &DatedCostLine) {
    let Ok(amount_cents) = cost.line.amount.parse::<f64>() else {
        tracing::warn!(amount = %cost.line.amount, "Skipping invalid Anthropic cost amount");
        return;
    };
    if !amount_cents.is_finite() || amount_cents < 0.0 {
        tracing::warn!(amount = %cost.line.amount, "Skipping invalid Anthropic cost amount");
        return;
    }
    let event_time = chrono::DateTime::parse_from_rfc3339(&cost.starting_at).map_or_else(
        |_| Utc::now().timestamp(),
        |timestamp| timestamp.timestamp(),
    );
    let insert_id = format!(
        "anthropic-cost:{}:{}:{}",
        cost.starting_at,
        cost.line.workspace_id.as_deref().unwrap_or("default"),
        cost.line.description.as_deref().unwrap_or("all")
    );

    mixpanel::track(
        Some(mixpanel_token),
        "anthropic_provider_cost",
        "anthropic-provider-ledger",
        serde_json::json!({
            "$insert_id": insert_id,
            "time": event_time,
            "provider": "anthropic",
            "actual_provider_cost_cents": amount_cents,
            "actual_provider_cost_microusd": (amount_cents * 10_000.0).round() as i64,
            "currency": cost.line.currency,
            "workspace_id": cost.line.workspace_id,
            "description": cost.line.description,
            "cost_type": cost.line.cost_type,
            "model": cost.line.model,
            "token_type": cost.line.token_type,
            "service_tier": cost.line.service_tier,
            "context_window": cost.line.context_window,
            "inference_geo": cost.line.inference_geo,
            "speed": cost.line.speed,
            "period_start": cost.starting_at,
            "period_end": cost.ending_at,
            "provider_cost_scope": "organization",
            "provider_cost_source": "anthropic_cost_api",
            "is_authoritative_provider_cost": true,
            "cost_api_excludes_priority_tier": true,
        }),
    );
}

#[cfg(test)]
mod tests {
    use super::fetch_cost_lines;
    use chrono::NaiveDate;
    use wiremock::{
        matchers::{header, method, path},
        Mock, MockServer, ResponseTemplate,
    };

    #[tokio::test]
    async fn fetches_and_parses_anthropic_cost_rows() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/organizations/cost_report"))
            .and(header("x-api-key", "admin-key"))
            .and(header("anthropic-version", "2023-06-01"))
            .and(header("user-agent", "AURA-z-billing/1.0"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [{
                    "starting_at": "2026-07-20T00:00:00Z",
                    "ending_at": "2026-07-21T00:00:00Z",
                    "results": [{
                        "amount": "123.45",
                        "currency": "USD",
                        "workspace_id": "wrkspc_1",
                        "description": "Claude Sonnet 5 output tokens",
                        "cost_type": "tokens",
                        "model": "claude-sonnet-5",
                        "token_type": "output_tokens",
                        "service_tier": "standard",
                        "context_window": "0-200k",
                        "inference_geo": "global",
                        "speed": "standard"
                    }]
                }],
                "has_more": false,
                "next_page": null
            })))
            .mount(&server)
            .await;

        let rows = fetch_cost_lines(
            &reqwest::Client::new(),
            &server.uri(),
            "admin-key",
            NaiveDate::from_ymd_opt(2026, 7, 20).unwrap(),
        )
        .await
        .expect("cost report");

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].line.amount, "123.45");
        assert_eq!(rows[0].line.model.as_deref(), Some("claude-sonnet-5"));
        assert_eq!(rows[0].starting_at, "2026-07-20T00:00:00Z");
    }
}
