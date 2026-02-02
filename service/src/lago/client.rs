//! Lago API client implementation.

use reqwest::Client;
use std::time::Duration;

use super::types::{
    metrics, CreateCustomerRequest, CreateEventRequest, CreateSubscriptionRequest, Customer,
    CustomerInput, CustomerResponse, Event, EventInput, EventResponse, LagoErrorResponse,
    Subscription, SubscriptionInput, SubscriptionResponse,
};

/// Error type for Lago operations.
#[derive(Debug, thiserror::Error)]
pub enum LagoError {
    /// HTTP request failed.
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// Lago API returned an error.
    #[error("Lago API error: {status} - {error}")]
    Api {
        /// HTTP status code.
        status: u16,
        /// Error message.
        error: String,
        /// Error code.
        code: Option<String>,
    },

    /// Serialization error.
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Configuration error.
    #[error("Configuration error: {0}")]
    Configuration(String),
}

/// Lago API client.
#[derive(Debug, Clone)]
pub struct LagoClient {
    client: Client,
    base_url: String,
    api_key: String,
}

impl LagoClient {
    /// Create a new Lago client.
    ///
    /// # Arguments
    ///
    /// * `base_url` - Lago API URL (e.g., `"http://localhost:3000"`)
    /// * `api_key` - Lago API key
    ///
    /// # Panics
    ///
    /// Panics if the HTTP client cannot be built (should not happen with default settings).
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to build HTTP client");

        Self {
            client,
            base_url: base_url.into().trim_end_matches('/').to_string(),
            api_key: api_key.into(),
        }
    }

    /// Create a customer in Lago.
    ///
    /// This should be called when a new account is created in z-billing.
    pub async fn create_customer(&self, input: CustomerInput) -> Result<Customer, LagoError> {
        let url = format!("{}/api/v1/customers", self.base_url);
        let request = CreateCustomerRequest { customer: input };

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        self.handle_response::<CustomerResponse>(response)
            .await
            .map(|r| r.customer)
    }

    /// Get a customer by external ID.
    pub async fn get_customer(&self, external_id: &str) -> Result<Option<Customer>, LagoError> {
        let url = format!("{}/api/v1/customers/{}", self.base_url, external_id);

        let response = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }

        self.handle_response::<CustomerResponse>(response)
            .await
            .map(|r| Some(r.customer))
    }

    /// Create a subscription for a customer.
    pub async fn create_subscription(
        &self,
        input: SubscriptionInput,
    ) -> Result<Subscription, LagoError> {
        let url = format!("{}/api/v1/subscriptions", self.base_url);
        let request = CreateSubscriptionRequest {
            subscription: input,
        };

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        self.handle_response::<SubscriptionResponse>(response)
            .await
            .map(|r| r.subscription)
    }

    /// Terminate a subscription.
    pub async fn terminate_subscription(
        &self,
        external_id: &str,
    ) -> Result<Subscription, LagoError> {
        let url = format!("{}/api/v1/subscriptions/{}", self.base_url, external_id);

        let response = self
            .client
            .delete(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await?;

        self.handle_response::<SubscriptionResponse>(response)
            .await
            .map(|r| r.subscription)
    }

    /// Send a usage event to Lago.
    ///
    /// This forwards usage events for analytics and dashboards.
    /// Note: z-billing handles the actual credit deduction, Lago is for reporting.
    pub async fn send_event(&self, input: EventInput) -> Result<Event, LagoError> {
        let url = format!("{}/api/v1/events", self.base_url);
        let request = CreateEventRequest { event: input };

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        self.handle_response::<EventResponse>(response)
            .await
            .map(|r| r.event)
    }

    /// Send a batch of usage events to Lago.
    ///
    /// Lago accepts events one at a time, so this sends them sequentially.
    /// For high volume, consider using a background queue.
    pub async fn send_events(&self, events: Vec<EventInput>) -> Result<Vec<Event>, LagoError> {
        let mut results = Vec::with_capacity(events.len());
        for event in events {
            results.push(self.send_event(event).await?);
        }
        Ok(results)
    }

    /// Send LLM token usage event.
    ///
    /// This is a convenience method for the common LLM usage case.
    pub async fn send_llm_usage(
        &self,
        transaction_id: &str,
        customer_id: &str,
        provider: &str,
        model: &str,
        agent_id: Option<&str>,
        input_tokens: u64,
        output_tokens: u64,
    ) -> Result<(), LagoError> {
        // Lago expects Unix timestamp (seconds)
        let timestamp = chrono::Utc::now().timestamp().to_string();

        // Send input tokens event
        if input_tokens > 0 {
            self.send_event(EventInput {
                transaction_id: format!("{transaction_id}_input"),
                external_customer_id: customer_id.to_string(),
                code: metrics::LLM_INPUT_TOKENS.to_string(),
                timestamp: timestamp.clone(),
                properties: Some(serde_json::json!({
                    "tokens": input_tokens,
                    "provider": provider,
                    "model": model,
                    "agent_id": agent_id,
                })),
                external_subscription_id: None,
            })
            .await?;
        }

        // Send output tokens event
        if output_tokens > 0 {
            self.send_event(EventInput {
                transaction_id: format!("{transaction_id}_output"),
                external_customer_id: customer_id.to_string(),
                code: metrics::LLM_OUTPUT_TOKENS.to_string(),
                timestamp,
                properties: Some(serde_json::json!({
                    "tokens": output_tokens,
                    "provider": provider,
                    "model": model,
                    "agent_id": agent_id,
                })),
                external_subscription_id: None,
            })
            .await?;
        }

        Ok(())
    }

    /// Send compute usage event.
    pub async fn send_compute_usage(
        &self,
        transaction_id: &str,
        customer_id: &str,
        agent_id: Option<&str>,
        cpu_hours: f64,
        memory_gb_hours: f64,
    ) -> Result<(), LagoError> {
        // Lago expects Unix timestamp (seconds)
        let timestamp = chrono::Utc::now().timestamp().to_string();

        // Send CPU hours event
        if cpu_hours > 0.0 {
            self.send_event(EventInput {
                transaction_id: format!("{transaction_id}_cpu"),
                external_customer_id: customer_id.to_string(),
                code: metrics::CPU_HOURS.to_string(),
                timestamp: timestamp.clone(),
                properties: Some(serde_json::json!({
                    "hours": cpu_hours,
                    "agent_id": agent_id,
                })),
                external_subscription_id: None,
            })
            .await?;
        }

        // Send memory GB hours event
        if memory_gb_hours > 0.0 {
            self.send_event(EventInput {
                transaction_id: format!("{transaction_id}_memory"),
                external_customer_id: customer_id.to_string(),
                code: metrics::MEMORY_GB_HOURS.to_string(),
                timestamp,
                properties: Some(serde_json::json!({
                    "gb_hours": memory_gb_hours,
                    "agent_id": agent_id,
                })),
                external_subscription_id: None,
            })
            .await?;
        }

        Ok(())
    }

    /// Handle API response and convert errors.
    async fn handle_response<T: serde::de::DeserializeOwned>(
        &self,
        response: reqwest::Response,
    ) -> Result<T, LagoError> {
        let status = response.status();

        if status.is_success() {
            return Ok(response.json().await?);
        }

        // Try to parse error response
        let error_body: Result<LagoErrorResponse, _> = response.json().await;

        match error_body {
            Ok(lago_error) => {
                // Include error_details if present
                let error_msg = if let Some(details) = &lago_error.error_details {
                    format!("{} - details: {}", lago_error.error, details)
                } else {
                    lago_error.error
                };
                Err(LagoError::Api {
                    status: lago_error.status,
                    error: error_msg,
                    code: lago_error.code,
                })
            }
            Err(_) => Err(LagoError::Api {
                status: status.as_u16(),
                error: format!("HTTP {status}"),
                code: None,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_creation() {
        let client = LagoClient::new("http://localhost:3000", "test-api-key");
        assert_eq!(client.base_url, "http://localhost:3000");
    }

    #[test]
    fn client_trims_trailing_slash() {
        let client = LagoClient::new("http://localhost:3000/", "test-api-key");
        assert_eq!(client.base_url, "http://localhost:3000");
    }
}
