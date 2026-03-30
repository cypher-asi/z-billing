//! WebSocket handler for real-time balance updates.

use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::response::IntoResponse;
use serde::Deserialize;
use tokio::sync::broadcast;

use crate::error::ApiError;
use crate::state::AppState;

const PING_INTERVAL: Duration = Duration::from_secs(30);

#[derive(Debug, Deserialize)]
pub struct WsQuery {
    pub token: Option<String>,
}

pub async fn ws_balance(
    State(state): State<Arc<AppState>>,
    Query(query): Query<WsQuery>,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, ApiError> {
    let token = query.token.ok_or(ApiError::Unauthorized)?;

    // Validate JWT and extract user ID
    let claims = crate::auth::validate_jwt(&token, &state)
        .await
        .map_err(|_| ApiError::Unauthorized)?;

    let user_id = claims
        .user_id()
        .ok_or(ApiError::Unauthorized)?
        .to_string();

    let rx = state.balance_tx.subscribe();

    Ok(ws.on_upgrade(|socket| handle_ws(socket, rx, user_id)))
}

async fn handle_ws(
    mut socket: WebSocket,
    mut rx: broadcast::Receiver<String>,
    user_id: String,
) {
    let mut ping_interval = tokio::time::interval(PING_INTERVAL);

    loop {
        tokio::select! {
            result = rx.recv() => {
                match result {
                    Ok(msg) => {
                        // Only forward messages belonging to this user
                        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&msg) {
                            if parsed.get("userId").and_then(|v| v.as_str()) != Some(&user_id) {
                                continue;
                            }
                        }
                        if socket.send(Message::Text(msg.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(skipped = n, "WebSocket client lagged, skipped events");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            result = socket.recv() => {
                match result {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Pong(_))) => {}
                    _ => {}
                }
            }
            _ = ping_interval.tick() => {
                if socket.send(Message::Ping(vec![].into())).await.is_err() {
                    break;
                }
            }
        }
    }
}
