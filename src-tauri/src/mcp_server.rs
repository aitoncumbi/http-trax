use axum::{
    extract::{Json, State},
    response::sse::{Event, Sse},
    routing::{get, post},
    Router,
};
use futures_util::stream::Stream;
use serde_json::Value;
use std::convert::Infallible;
use std::sync::Arc;
use std::sync::Mutex;
use tauri::AppHandle;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio_stream::wrappers::UnboundedReceiverStream;
use trax_mcp::Config;
use trax_mcp::Server;

pub struct McpServerManager {
    shutdown_tx: Mutex<Option<oneshot::Sender<()>>>,
}

impl McpServerManager {
    pub fn new() -> Self {
        Self {
            shutdown_tx: Mutex::new(None),
        }
    }

    pub fn start(&self, port: u16, _handle: AppHandle) {
        self.stop();

        let (tx, rx) = oneshot::channel();
        *self.shutdown_tx.lock().unwrap() = Some(tx);

        tokio::spawn(async move {
            let config = Config::default();
            let server = Server::new(config);
            // Channel to broadcast messages to all connected SSE clients
            let (broadcast_tx, _) = tokio::sync::broadcast::channel(16);

            let app = Router::new()
                .route("/sse", get(sse_handler))
                .route("/message", post(message_handler))
                .with_state(Arc::new(HttpState {
                    server: server.clone(),
                    broadcast_tx,
                }));

            let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
            let listener = tokio::net::TcpListener::bind(addr).await.unwrap();

            let _ = axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    rx.await.ok();
                })
                .await;
        });
    }

    pub fn stop(&self) {
        if let Some(tx) = self.shutdown_tx.lock().unwrap().take() {
            let _ = tx.send(());
        }
    }
}

pub struct HttpState {
    pub server: Arc<Server>,
    pub broadcast_tx: tokio::sync::broadcast::Sender<Value>,
}

async fn sse_handler(
    State(state): State<Arc<HttpState>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let (tx, rx) = mpsc::unbounded_channel::<Result<Event, Infallible>>();
    let mut broadcast_rx = state.broadcast_tx.subscribe();

    // Task to forward broadcasted messages to this specific client
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(15));
        loop {
            tokio::select! {
                _ = tick.tick() => {
                    if tx.send(Ok(Event::default().event("ping").data(""))).is_err() {
                        break;
                    }
                }
                Ok(msg) = broadcast_rx.recv() => {
                    let event = Event::default().event("message").data(msg.to_string());
                    if tx.send(Ok(event)).is_err() {
                        break;
                    }
                }
                else => break,
            }
        }
    });

    Sse::new(UnboundedReceiverStream::new(rx))
}

async fn message_handler(State(state): State<Arc<HttpState>>, Json(message): Json<Value>) {
    let server = state.server.clone();
    let broadcast_tx = state.broadcast_tx.clone();

    tokio::spawn(async move {
        if let Some(response) = server.handle(message).await {
            let _ = broadcast_tx.send(response);
        }
    });
}
