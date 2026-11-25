// Topic: middleware

use std::time::Duration;

use axum::{
    Router,
    extract::{Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::Response,
    routing::get,
};
use tower::ServiceBuilder;
use tower_http::{
    cors::{Any, CorsLayer},
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    timeout::TimeoutLayer,
    trace::{DefaultOnResponse, TraceLayer},
};
use tracing::Level;

#[tokio::main]
async fn main() {
    let apikey = String::from("mykey");
    let appstate = AppState { apikey };

    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080")
        .await
        .unwrap();

    let app = Router::new()
        .route("/", get(async || "hello world"))
        .layer(middleware::from_fn_with_state(appstate.clone(), handle_mw1))
        .layer(
            ServiceBuilder::new()
                .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
                .layer(TraceLayer::new_for_http().make_span_with(|req: &Request| {
                    let id = req
                        .headers()
                        .get("x-request-id")
                        .and_then(|v| v.to_str().ok());
                    tracing::info_span!("request", method = %req.method(), uri = %req.uri(), request_id = id)
                }).on_response(DefaultOnResponse::new().level(Level::INFO)))
                .layer(PropagateRequestIdLayer::x_request_id())
                .layer(TimeoutLayer::with_status_code(StatusCode::REQUEST_TIMEOUT, Duration::from_secs(10)))
                .layer(CorsLayer::new().allow_origin(Any).allow_headers(Any).allow_methods(Any))
        )
        .with_state(appstate);

    axum::serve(listener, app)
        .with_graceful_shutdown(handle_shutdown())
        .await
        .unwrap();
}

// 1. custom middleware
#[derive(Clone)]
struct AppState {
    apikey: String,
}

async fn handle_mw1(
    State(x): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, (StatusCode, String)> {
    let key = req.headers().get("x-key").and_then(|v| v.to_str().ok());
    if key != Some(x.apikey.as_str()) {
        return Err((StatusCode::UNAUTHORIZED, "not proper apikey".to_owned()));
    }
    Ok(next.run(req).await)
}

async fn handle_shutdown() {
    let a1 = async {
        tokio::signal::ctrl_c().await.expect("ctrl_c fails");
    };
    let a2 = std::future::pending::<()>();
    tokio::select! {
        _ = a1 => {},
        _ = a2 => {},
    }
}

// 2.
//
