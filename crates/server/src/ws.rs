//! WebSocket endpoint for live engine → UI events.
//!
//! On connect the server sends a `hello`, then forwards every `EngineEvent`
//! from the broadcast channel as JSON. Because it's a broadcast subscription,
//! progress keeps flowing to any/all connected UIs and survives reloads.

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::Response;
use serde_json::json;
use tokio::sync::broadcast::error::RecvError;

use crate::AppState;

pub async fn handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: AppState) {
    let hello = json!({ "type": "hello", "version": engine::VERSION });
    if socket
        .send(Message::Text(hello.to_string().into()))
        .await
        .is_err()
    {
        return;
    }

    let mut rx = state.events.subscribe();
    loop {
        tokio::select! {
            event = rx.recv() => match event {
                Ok(ev) => {
                    if let Ok(text) = serde_json::to_string(&ev) {
                        if socket.send(Message::Text(text.into())).await.is_err() {
                            break;
                        }
                    }
                }
                // Dropped some events because this client fell behind; keep going.
                Err(RecvError::Lagged(_)) => continue,
                Err(RecvError::Closed) => break,
            },
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Ping(p))) => { let _ = socket.send(Message::Pong(p)).await; }
                Some(Ok(Message::Close(_))) | None => break,
                _ => {}
            },
        }
    }
}
