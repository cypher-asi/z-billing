//! Z-Billing HTTP client implementation.

use reqwest::Client;
use std::time::Duration;

use crate::error::ClientError;
use crate::types::{
    ApiErrorResponse, BalanceResponse, BatchUsageRequest, BatchUsageResponse, CheckBalanceRequest,
    CheckBalanceResponse, ComputeUsageEvent, LlmUsageEvent, UsageMetric, UsageRequest,
    UsageResponse,
};

/// Z-Billing API client.
///
/// Provides methods for reporting usage and checking balances.
#[derive(Debug, Clone)]
pub struct ZBillingClient {
    client: Client,
    base_url: String,
    api_key: String,
    service_name: String,
}

impl ZBillingClient {
    /// Create a new z-billing client.
    ///
    /// # Arguments
    ///
    /// * `base_url` - Base URL of the z-billing service (e.g., `"http://z-billing:8080"`)
    /// * `api_key` - Service API key for authentication
    #[must_use]
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self::with_options(base_url, api_key, ClientOptions::default())
    }

    /// Create a new z-billing client with custom options.
    ///
    /// # Panics
    ///
    /// Panics if the HTTP client cannot be built (should not happen with default settings).
    #[must_use]
    pub fn with_options(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        options: ClientOptions,
    ) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(options.timeout_seconds))
            .build()
            .expect("Failed to build HTTP client");

        Self {
            client,
            base_url: base_url.into().trim_end_matches('/').to_string(),
            api_key: api_key.into(),
            service_name: options.service_name,
        }
    }

    /// Report LLM usage.
    ///
    /// This is a convenience method that constructs a usage request for LLM tokens.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or the server returns an error.
    pub async fn report_llm_usage(
        &self,
        event: LlmUsageEvent,
    ) -> Result<UsageResponse, ClientError> {
        let request = UsageRequest {
            event_id: event.event_id,
            user_id: event.user_id,
            agent_id: event.agent_id,
            metric: UsageMetric::LlmTokens {
                provider: event.provider,
                model: event.model,
                input_tokens: event.input_tokens,
                output_tokens: event.output_tokens,
            },
            cost_cents: None,
            metadata: event.metadata,
        };

        self.report_usage(request).await
    }

    /// Report compute usage.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or the server returns an error.
    pub async fn report_compute_usage(
        &self,
        event: ComputeUsageEvent,
    ) -> Result<UsageResponse, ClientError> {
        let request = UsageRequest {
            event_id: event.event_id,
            user_id: event.user_id,
            agent_id: event.agent_id,
            metric: UsageMetric::Compute {
                cpu_hours: event.cpu_hours,
                memory_gb_hours: event.memory_gb_hours,
            },
            cost_cents: None,
            metadata: event.metadata,
        };

        self.report_usage(request).await
    }

    /// Report a generic usage event.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or the server returns an error.
    pub async fn report_usage(&self, request: UsageRequest) -> Result<UsageResponse, ClientError> {
        let url = format!("{}/v1/usage", self.base_url);

        let response = self
            .client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("x-service-name", &self.service_name)
            .json(&request)
            .send()
            .await?;

        self.handle_response(response).await
    }

    /// Report multiple usage events in a batch.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or the server returns an error.
    pub async fn report_usage_batch(
        &self,
        events: Vec<UsageRequest>,
    ) -> Result<BatchUsageResponse, ClientError> {
        let url = format!("{}/v1/usage/batch", self.base_url);
        let request = BatchUsageRequest { events };

        let response = self
            .client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("x-service-name", &self.service_name)
            .json(&request)
            .send()
            .await?;

        self.handle_response(response).await
    }

    /// Check if a user has sufficient balance.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or the server returns an error.
    pub async fn check_balance(
        &self,
        user_id: impl Into<String>,
        required_cents: i64,
    ) -> Result<CheckBalanceResponse, ClientError> {
        let url = format!("{}/v1/usage/check", self.base_url);
        let request = CheckBalanceRequest {
            user_id: user_id.into(),
            required_cents,
        };

        let response = self
            .client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("x-service-name", &self.service_name)
            .json(&request)
            .send()
            .await?;

        self.handle_response(response).await
    }

    /// Get a user's current balance (requires user JWT, not service API key).
    ///
    /// This method is typically used by the user-facing dashboard, not by services.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or the server returns an error.
    pub async fn get_balance(&self, user_jwt: &str) -> Result<BalanceResponse, ClientError> {
        let url = format!("{}/v1/credits/balance", self.base_url);

        let response = self
            .client
            .get(&url)
            .header("authorization", format!("Bearer {user_jwt}"))
            .send()
            .await?;

        self.handle_response(response).await
    }

    /// Handle API response and convert errors.
    async fn handle_response<T: serde::de::DeserializeOwned>(
        &self,
        response: reqwest::Response,
    ) -> Result<T, ClientError> {
        let status = response.status();

        if status.is_success() {
            return Ok(response.json().await?);
        }

        // Try to parse error response
        let error_body: Result<ApiErrorResponse, _> = response.json().await;

        match error_body {
            Ok(api_error) => {
                let code = api_error.error.code.as_str();
                let message = api_error.error.message;

                // Map specific error codes to typed errors
                match code {
                    "insufficient_credits" => {
                        let balance = api_error
                            .error
                            .details
                            .as_ref()
                            .and_then(|d| d.get("balance"))
                            .and_then(serde_json::Value::as_i64)
                            .unwrap_or(0);
                        let required = api_error
                            .error
                            .details
                            .as_ref()
                            .and_then(|d| d.get("required"))
                            .and_then(serde_json::Value::as_i64)
                            .unwrap_or(0);

                        Err(ClientError::InsufficientCredits { balance, required })
                    }
                    "duplicate_event" => Err(ClientError::DuplicateEvent { event_id: message }),
                    "not_found" if message.contains("Account") => {
                        Err(ClientError::AccountNotFound {
                            user_id: message.replace("Account not found: ", ""),
                        })
                    }
                    _ => Err(ClientError::Api {
                        code: code.to_string(),
                        message,
                        status: status.as_u16(),
                    }),
                }
            }
            Err(_) => Err(ClientError::Api {
                code: "unknown".to_string(),
                message: format!("HTTP {status}"),
                status: status.as_u16(),
            }),
        }
    }
}

/// Client options for customization.
#[derive(Debug, Clone)]
pub struct ClientOptions {
    /// Request timeout in seconds (default: 30).
    pub timeout_seconds: u64,
    /// Service name to include in requests.
    pub service_name: String,
}

impl Default for ClientOptions {
    fn default() -> Self {
        Self {
            timeout_seconds: 30,
            service_name: "unknown".to_string(),
        }
    }
}

impl ClientOptions {
    /// Create options with a service name.
    #[must_use]
    pub fn with_service_name(name: impl Into<String>) -> Self {
        Self {
            service_name: name.into(),
            ..Self::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_creation() {
        let client = ZBillingClient::new("http://localhost:8080", "test-api-key");
        assert_eq!(client.base_url, "http://localhost:8080");
    }

    #[test]
    fn client_trims_trailing_slash() {
        let client = ZBillingClient::new("http://localhost:8080/", "test-api-key");
        assert_eq!(client.base_url, "http://localhost:8080");
    }

    #[test]
    fn client_options() {
        let options = ClientOptions::with_service_name("aura-runtime");
        let client = ZBillingClient::with_options("http://localhost:8080", "key", options);
        assert_eq!(client.service_name, "aura-runtime");
    }
}
