use std::sync::Arc;
use std::sync::Mutex;
use tauri::AppHandle;
use tokio::sync::oneshot;
use tokio::sync::mpsc;
use trax_mcp::Server;
use trax_mcp::Config;
use axum::{
    extract::{State, Json},
    response::sse::{Event, Sse},
    routing::{get, post},
    Router,
};
use futures_util::stream::Stream;
use tokio_stream::wrappers::UnboundedReceiverStream;
use std::convert::Infallible;
use serde_json::Value;

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
            let (msg_tx, _msg_rx) = mpsc::unbounded_channel::<Value>();

            let app = Router::new()
                .route("/sse", get(sse_handler))
                .route("/message", post(message_handler))
                .with_state(Arc::new(HttpState { server: server.clone(), _tx: msg_tx }));

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
    pub _tx: mpsc::UnboundedSender<Value>,
}

async fn sse_handler(
    State(_state): State<Arc<HttpState>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let (tx, rx) = mpsc::unbounded_channel::<Result<Event, Infallible>>();

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
        if let Some(_response) = server.handle(message).await {
            // Processing response
        }
    });
}
