use axum::{
    extract::{State, Json},
    response::sse::{Event, Sse},
    routing::{get, post},
    Router,
};
use futures_util::stream::Stream;
use std::{convert::Infallible, sync::Arc};
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;
use serde_json::Value;

use crate::Server;

pub struct HttpState {
    pub server: Arc<Server>,
    pub tx: mpsc::UnboundedSender<Value>,
}

pub fn router(server: Arc<Server>, tx: mpsc::UnboundedSender<Value>) -> Router {
    let state = Arc::new(HttpState { server, tx });

    Router::new()
        .route("/sse", get(sse_handler))
        .route("/message", post(message_handler))
        .with_state(state)
}

async fn sse_handler(
    State(state): State<Arc<HttpState>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let (tx, rx) = mpsc::unbounded_channel::<Result<Event, Infallible>>();

    // In a full implementation, we'd register this client to receive responses
    // For now, this is a placeholder that keeps the connection open
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(15));
        loop {
            tick.tick().await;
            if tx.send(Ok(Event::default().event("ping").data(""))).is_err() {
                break;
            }
        }
    });

    Sse::new(UnboundedReceiverStream::new(rx))
}

async fn message_handler(
    State(state): State<Arc<HttpState>>,
    Json(message): Json<Value>,
) {
    let server = state.server.clone();
    tokio::spawn(async move {
        if let Some(response) = server.handle(message).await {
            // In a full implementation, we'd route this back to the specific client
            // via their SSE connection. For now we just process it.
        }
    });
}
