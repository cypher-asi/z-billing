/// Fire-and-forget Mixpanel event tracking.
///
/// Sends events to the Mixpanel Track API. Silently returns if the token
/// is `None` or empty. Errors are logged but never propagated — analytics
/// must never break webhook processing or billing operations.

/// Track an event to Mixpanel. Fire-and-forget via `tokio::spawn`.
pub fn track(token: Option<&str>, event: &str, distinct_id: &str, extra: serde_json::Value) {
    let token = match token {
        Some(t) if !t.is_empty() => t.to_string(),
        _ => return,
    };
    let event = event.to_string();
    let distinct_id = distinct_id.to_string();

    tokio::spawn(async move {
        let mut properties = match extra {
            serde_json::Value::Object(map) => map,
            _ => serde_json::Map::new(),
        };
        properties.insert("distinct_id".into(), serde_json::json!(distinct_id));
        properties.insert("token".into(), serde_json::json!(token));

        let payload = serde_json::json!([{
            "event": event,
            "properties": serde_json::Value::Object(properties),
        }]);

        let client = reqwest::Client::new();
        match client
            .post("https://api.mixpanel.com/track")
            .header("Content-Type", "application/json")
            .header("Accept", "text/plain")
            .json(&payload)
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                tracing::debug!(event = %event, "Mixpanel event tracked");
            }
            Ok(resp) => {
                tracing::warn!(event = %event, status = %resp.status(), "Mixpanel track failed");
            }
            Err(err) => {
                tracing::warn!(event = %event, error = %err, "Mixpanel track error");
            }
        }
    });
}
