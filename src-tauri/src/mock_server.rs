use axum::{
    body::Body,
    extract::{Request, State},
    http::{HeaderName, HeaderValue, Response, StatusCode},
    response::IntoResponse,
    routing::any,
    Router,
};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use tokio::sync::oneshot;
use trax_core::Workspace;

#[allow(dead_code)]
pub struct MockServerManager {
    shutdown_tx: Mutex<Option<oneshot::Sender<()>>>,
}

impl MockServerManager {
    pub fn new() -> Self {
        Self {
            shutdown_tx: Mutex::new(None),
        }
    }

    #[allow(dead_code)]
    pub fn start(&self, port: u16, workspace_path: String) {
        self.stop();

        let (tx, rx) = oneshot::channel();
        *self.shutdown_tx.lock().unwrap() = Some(tx);

        tokio::spawn(async move {
            let app = Router::new()
                .route("/{*path}", any(mock_handler))
                .route("/", any(mock_handler))
                .with_state(Arc::new(workspace_path));

            let addr = SocketAddr::from(([127, 0, 0, 1], port));
            if let Ok(listener) = tokio::net::TcpListener::bind(addr).await {
                let _ = axum::serve(listener, app)
                    .with_graceful_shutdown(async move {
                        rx.await.ok();
                    })
                    .await;
            }
        });
    }

    #[allow(dead_code)]
    pub fn stop(&self) {
        if let Some(tx) = self.shutdown_tx.lock().unwrap().take() {
            let _ = tx.send(());
        }
    }
}

#[allow(dead_code)]
async fn mock_handler(
    State(workspace_path): State<Arc<String>>,
    req: Request,
) -> impl IntoResponse {
    let ws = match Workspace::open(workspace_path.as_str()) {
        Ok(ws) => ws,
        Err(_) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, "Workspace not found").into_response()
        }
    };

    let tree = match ws.tree() {
        Ok(t) => t,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to read workspace tree",
            )
                .into_response()
        }
    };

    let req_path = req.uri().path();
    let req_method = req.method().as_str();

    // Naive matching: Look for a request where the URL ends with req_path and method matches
    // In a real implementation, we should parse the URL with variables and match it properly.
    // For now, let's find ANY saved response where the saved request's URL contains our path.

    let mut best_match = None;

    for col in tree.collections {
        for request_summary in col.requests() {
            if request_summary.method.eq_ignore_ascii_case(req_method) {
                if let Ok(responses) = ws.saved_responses(&request_summary.id) {
                    for saved in responses {
                        // Check if the saved request URL path matches the incoming path
                        if let Ok(url) = url::Url::parse(&saved.request.url) {
                            if url.path() == req_path || (req_path == "/" && url.path().is_empty())
                            {
                                best_match = Some(saved);
                                break;
                            }
                        } else {
                            // Fallback if URL parsing fails: basic string match
                            if saved.request.url.contains(req_path) {
                                best_match = Some(saved);
                                break;
                            }
                        }
                    }
                }
            }
            if best_match.is_some() {
                break;
            }
        }
        if best_match.is_some() {
            break;
        }
    }

    if let Some(saved) = best_match {
        let mut response = Response::builder().status(saved.response.status);

        if let Some(headers) = response.headers_mut() {
            for h in saved.response.headers {
                if let (Ok(name), Ok(value)) = (
                    HeaderName::try_from(h.key.as_str()),
                    HeaderValue::try_from(h.value.as_str()),
                ) {
                    headers.insert(name, value);
                }
            }
        }

        let body = if let Some(base64) = saved.response.body_base64 {
            use base64::Engine;
            match base64::engine::general_purpose::STANDARD.decode(base64) {
                Ok(bytes) => Body::from(bytes),
                Err(_) => Body::from(saved.response.body),
            }
        } else {
            Body::from(saved.response.body)
        };

        response.body(body).unwrap_or_else(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to build response",
            )
                .into_response()
        })
    } else {
        (StatusCode::NOT_FOUND, "No matching saved response found").into_response()
    }
}
