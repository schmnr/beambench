use std::sync::Arc;

use axum::Router;
use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;
use axum::routing::get;
use beambench_service::ServiceContext;
use beambench_service::events;
use tokio::time::{Duration, interval};

pub fn router() -> Router<Arc<ServiceContext>> {
    Router::new().route("/", get(ws_handler))
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(ctx): State<Arc<ServiceContext>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, ctx))
}

async fn handle_socket(mut socket: WebSocket, ctx: Arc<ServiceContext>) {
    let mut rx = ctx.events.subscribe();
    let mut heartbeat = interval(Duration::from_secs(10));

    let snapshot = match events::system_snapshot_payload(&ctx) {
        Ok(payload) => ctx.event_message("system.snapshot", payload),
        Err(err) => ctx.event_message(
            "system.warning",
            events::system_warning_payload(format!("Failed to build snapshot: {err}")),
        ),
    };
    if socket.send(Message::Text(snapshot.into())).await.is_err() {
        return;
    }

    loop {
        tokio::select! {
            // Forward broadcast events to WebSocket client
            result = rx.recv() => {
                match result {
                    Ok(msg) => {
                        if socket.send(Message::Text(msg.into())).await.is_err() {
                            // Client disconnected
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        let warning = ctx.event_message(
                            "system.warning",
                            events::system_warning_payload(format!("Dropped {n} events (slow consumer)")),
                        );
                        let _ = socket.send(Message::Text(warning.to_string().into())).await;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        break;
                    }
                }
            }
            // Send heartbeat every 10 seconds
            _ = heartbeat.tick() => {
                let ping = ctx.event_message("system.heartbeat", events::system_heartbeat_payload());
                if socket.send(Message::Text(ping.into())).await.is_err() {
                    break;
                }
            }
            // Receive messages from client (handle close gracefully)
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {} // Ignore other client messages
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_snapshot_message_uses_stable_envelope() {
        let ctx = Arc::new(ServiceContext::new());
        let msg = ctx.event_message(
            "system.snapshot",
            events::system_snapshot_payload(&ctx).unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&msg).unwrap();
        assert_eq!(parsed["type"], "system.snapshot");
        assert_eq!(parsed["id"], 1);
        assert!(parsed["timestamp"].is_string());
        assert!(parsed["payload"]["app"]["version"].is_string());
    }

    #[test]
    fn system_warning_message_uses_stable_envelope() {
        let ctx = Arc::new(ServiceContext::new());
        let msg = ctx.event_message(
            "system.warning",
            events::system_warning_payload("Dropped 5 events"),
        );
        let parsed: serde_json::Value = serde_json::from_str(&msg).unwrap();
        assert_eq!(parsed["type"], "system.warning");
        assert_eq!(parsed["payload"]["message"], "Dropped 5 events");
    }
}
