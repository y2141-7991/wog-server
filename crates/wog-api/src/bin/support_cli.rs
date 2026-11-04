//! Tiny terminal client for the support-chat WebSocket, for manual testing.
//!
//! ```text
//! support_cli --token <access token> [--url ws://127.0.0.1:3000/api/v1/support/ws] [--origin <origin>]
//! ```
//!
//! Get a token with `POST /api/v1/auth/login` (the `access_token` field).
//! Every server event is printed as one line of JSON. Input lines:
//!
//! ```text
//! hello there              send a message (users: to their own conversation)
//! /to <conversation> text  send a message into a conversation (admins)
//! /join <conversation>     start receiving that conversation's messages (admins)
//! /leave <conversation>    stop (admins)
//! /typing [conversation]   typing indicator on;  /stop [conversation] off
//! /read [conversation]     mark the other side's messages read
//! /close <conversation>    /reopen <conversation>   /assign <conversation>   (admins)
//! /ping                    application-level ping
//! /raw {"type":"..."}      send arbitrary JSON (to try malformed input)
//! ```
//!
//! Plain `ws://` only; for `wss://` use `websocat` (see the README).

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Message, client::IntoClientRequest, http::HeaderValue},
};

fn arg(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1).cloned())
}

fn command(line: &str) -> Option<Value> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let Some(rest) = line.strip_prefix('/') else {
        return Some(json!({
            "type": "send_message",
            "body": line,
            "client_msg_id": uuid::Uuid::new_v4().to_string(),
        }));
    };
    let mut parts = rest.splitn(3, ' ');
    let name = parts.next().unwrap_or("");
    let first = parts.next();
    let second = parts.next();
    let conv = first.map(|c| json!(c));
    Some(match name {
        "ping" => json!({"type": "ping"}),
        "to" => json!({
            "type": "send_message",
            "conversation_id": conv,
            "body": second.unwrap_or(""),
            "client_msg_id": uuid::Uuid::new_v4().to_string(),
        }),
        "join" => json!({"type": "join_conversation", "conversation_id": conv}),
        "leave" => json!({"type": "leave_conversation", "conversation_id": conv}),
        "typing" => json!({"type": "typing", "conversation_id": conv, "is_typing": true}),
        "stop" => json!({"type": "typing", "conversation_id": conv, "is_typing": false}),
        "read" => json!({"type": "mark_read", "conversation_id": conv}),
        "close" => json!({"type": "close_conversation", "conversation_id": conv}),
        "reopen" => json!({"type": "reopen_conversation", "conversation_id": conv}),
        "assign" => json!({"type": "assign_to_me", "conversation_id": conv}),
        "raw" => {
            // Send exactly what was typed after "/raw ", valid JSON or not.
            return Some(Value::String(
                rest.trim_start_matches("raw").trim().to_string(),
            ));
        }
        _ => {
            eprintln!("unknown command /{name}");
            return None;
        }
    })
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let url = arg("--url").unwrap_or_else(|| "ws://127.0.0.1:3000/api/v1/support/ws".into());
    let token = arg("--token").or_else(|| std::env::var("SUPPORT_TOKEN").ok());
    let Some(token) = token else {
        eprintln!("usage: support_cli --token <access token> [--url ws://...] [--origin <origin>]");
        std::process::exit(2);
    };

    let mut request = url.as_str().into_client_request()?;
    request.headers_mut().insert(
        "Authorization",
        HeaderValue::from_str(&format!("Bearer {token}"))?,
    );
    if let Some(origin) = arg("--origin") {
        request
            .headers_mut()
            .insert("Origin", HeaderValue::from_str(&origin)?);
    }

    let (socket, _) = connect_async(request).await?;
    eprintln!("connected to {url}; type a message, or /ping /join /to /read ...");
    let (mut tx, mut rx) = socket.split();

    let printer = tokio::spawn(async move {
        while let Some(Ok(msg)) = rx.next().await {
            match msg {
                Message::Text(t) => println!("< {t}"),
                Message::Close(c) => {
                    eprintln!("closed by server: {c:?}");
                    break;
                }
                _ => {}
            }
        }
    });

    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let Some(value) = command(&line) else {
            continue;
        };
        let text = match value {
            Value::String(raw) => raw,
            other => other.to_string(),
        };
        if tx.send(Message::Text(text.into())).await.is_err() {
            break;
        }
    }
    let _ = tx.close().await;
    printer.abort();
    Ok(())
}
