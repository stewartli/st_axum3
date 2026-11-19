use axum::{
    Json, Router,
    extract::{Path, Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use serde_json::json;
use tokio::sync::Mutex;
use tower_http::{services::ServeDir, trace::TraceLayer};
use tracing_subscriber::EnvFilter;

use std::sync::Arc;

#[tokio::main]
async fn main() {
    // 1. logger
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    // 2. state
    let db = AppState(Arc::new(Mutex::new(2)));

    // 3. server
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080")
        .await
        .unwrap_or_else(|x| panic!("fail to bind this addr: {x}"));

    let app = Router::new()
        .route("/health", get(handle_health))
        .route("/echo", get(handle_req))
        .route("/api/v1", get(handle_api))
        .route("/data/{id}", get(handle_data))
        .with_state(db)
        .fallback_service(ServeDir::new("./static"))
        .layer(TraceLayer::new_for_http());

    tracing::info!("server running at {}", listener.local_addr().unwrap());
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            tokio::signal::ctrl_c().await.unwrap();
            tracing::info!("shut down server");
        })
        .await
        .unwrap();
}

// 1. error
#[derive(Debug, thiserror::Error)]
enum AppErr {
    #[error("server down")]
    ServerDown,
    #[error("some error: {0}")]
    Other(String),
}

impl IntoResponse for AppErr {
    fn into_response(self) -> Response {
        let res = match &self {
            Self::ServerDown => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Other(_) => StatusCode::NOT_FOUND,
        };
        if res.is_server_error() {
            tracing::error!("AppErr::ServerDown");
        }
        (res, Json(json!({ "error": self.to_string() }))).into_response()
    }
}

// 2. state
#[derive(Clone)]
struct AppState(Arc<Mutex<i32>>);

async fn handle_health(State(db): State<AppState>) -> impl IntoResponse {
    let num = db.0.lock().await.wrapping_add(2);
    (
        StatusCode::IM_A_TEAPOT,
        format!("server running ok\ndb num = {num}"),
    )
}

// 3. request
async fn handle_req(req: Request) -> Response {
    let (part, body) = req.into_parts();
    let _bytes = match axum::body::to_bytes(body, 64 * 1024).await {
        Ok(x) => x,
        Err(_) => {
            return (StatusCode::PAYLOAD_TOO_LARGE, "unable to handle body\n").into_response();
        }
    };
    let mut res = String::new();
    res.push_str(&format!("method: {}\n", part.method));
    res.push_str(&format!("path: {}\n", part.uri.path()));
    for (name, val) in &part.headers {
        res.push_str(&format!("{name}: {}\n", val.to_str().unwrap_or("nothing")));
    }
    res.into_response()
}

// 4. result to response
#[derive(serde::Serialize, serde::Deserialize)]
struct HelloApi {
    version: u8,
    desc: String,
}

async fn handle_api() -> Json<HelloApi> {
    Json(HelloApi {
        version: 1,
        desc: "this is api version 1".to_string(),
    })
}

async fn handle_data(Path(id): Path<i32>) -> Result<String, AppErr> {
    if id > 10 {
        Ok(format!("num = {id}"))
    } else {
        Err(AppErr::ServerDown)
    }
}
