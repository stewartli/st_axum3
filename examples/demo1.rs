// Topic: basic

use axum::{
    Form, Json, Router,
    body::{Body, Bytes, HttpBody},
    extract::{
        ConnectInfo, FromRequest, FromRequestParts, MatchedPath, NestedPath,
        OptionalFromRequestParts, OriginalUri, Path, Query, RawPathParams, RawQuery, Request,
        State, rejection::BytesRejection,
    },
    http::{HeaderMap, HeaderValue, Method, StatusCode, Uri, Version, header},
    response::{Html, IntoResponse, NoContent, Redirect, Response},
    routing::{MethodFilter, any, get},
};
use tokio::sync::Mutex;
use tower_http::{services::ServeDir, trace::TraceLayer};
use tracing_subscriber::EnvFilter;

use std::{collections::HashMap, convert::Infallible, net::SocketAddr, sync::Arc};

#[tokio::main]
async fn main() {
    // 1. logger [RUST_LOG]
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

    // 3.1 route vs service vs fallback,
    let old_route = Router::new().route("/", get(async || "old route"));
    let nest_api_route = Router::new()
        .route("/v2", get(handle_redirect))
        // axum_extra::routing::RouterExt::route_with_tsr()
        .nest_service("/old", old_route);

    // 3.2 handler vs service,
    let merge_admin_route = Router::new()
        .route("/admin", get(handle_redirect))
        .route_service("/service", tower::service_fn(handle_service))
        .method_not_allowed_fallback(handle_api)
        .reset_fallback();

    // 3.3 any, on, head, options,
    let app = Router::new()
        .route("/health", get(handle_health))
        .route(
            "/echo1",
            get(handle_req)
                .head(handle_req)
                .post(handle_query)
                .put(handle_key),
        )
        .route(
            "/echo2",
            any(handle_any).on(MethodFilter::GET.or(MethodFilter::POST), handle_req),
        )
        .route(
            "/data/{id}/{*job}",
            get(handle_data)
                .put(handle_data1)
                .patch(handle_data2)
                .post(handle_form),
        )
        .route("/api/v1", get(handle_api).post(handle_204))
        .nest("/api", nest_api_route)
        .merge(merge_admin_route)
        .with_state(db)
        .fallback(handle_404)
        .fallback_service(ServeDir::new("./static"))
        .layer(TraceLayer::new_for_http())
        .into_make_service_with_connect_info::<SocketAddr>();

    tracing::info!("server running at {}", listener.local_addr().unwrap());
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            tokio::signal::ctrl_c().await.unwrap();
            tracing::info!("shut down server");
        })
        .await
        .unwrap();
}

// 1. custom error
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
        (res, Json(serde_json::json!({ "error": self.to_string() }))).into_response()
    }
}

// 2. response
async fn handle_any(x: Method) -> String {
    format!("you call {x}")
}

async fn handle_204() -> NoContent {
    NoContent
}

async fn handle_redirect() -> Redirect {
    Redirect::to("/health")
}

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

// 3. extractor
#[derive(Clone)]
struct AppState(Arc<Mutex<i32>>);

async fn handle_health(State(db): State<AppState>) -> impl IntoResponse {
    // statuscode first, body always last
    let num = db.0.lock().await.wrapping_add(2);
    (
        StatusCode::IM_A_TEAPOT,
        format!("server running ok\ndb num = {num}"),
    )
}

async fn handle_data(Path(id): Path<i32>) -> Result<Json<i32>, StatusCode> {
    let data = vec![1, 1, 2, 3, 4];
    data.into_iter()
        .find(|&x| x == id)
        .map(Json)
        .ok_or(StatusCode::NOT_MODIFIED)
}

async fn handle_data1(Path((id, job)): Path<(i32, String)>) -> Result<String, AppErr> {
    // http://127.0.0.1:8080/data/3/mas
    if id > 10 {
        Ok(format!("{job} num = {id}"))
    } else if id < 3 {
        Err(AppErr::ServerDown)
    } else {
        Err(AppErr::Other(String::from("other error happened")))
    }
}

async fn handle_data2(Path(x): Path<HashMap<String, String>>) -> Json<HashMap<String, String>> {
    Json(x)
}

async fn handle_404(Query(name): Query<String>) -> Html<String> {
    if name.contains("ca") {
        Html(format!("<h1>CA Analytics: {name}</h1>"))
    } else {
        Html(format!("<h1>No Idea What You Ask: {name}</h1>"))
    }
}

#[allow(clippy::too_many_arguments)]
async fn handle_query(
    Query(x): Query<HelloApi>,
    RawQuery(g): RawQuery,
    OriginalUri(d): OriginalUri,
    y: HeaderMap,
    h: RawPathParams,
    w: MatchedPath,
    c: NestedPath,
    u: ConnectInfo<SocketAddr>,
    a: Method,
    l: Uri,
    v: Version,
    z: Body,
) -> Json<serde_json::Value> {
    // more extractor: headers, bytes, route, ip, method, uri, version,
    let mut res = String::new();
    for (name, val) in y.iter() {
        if let Ok(v) = val.to_str() {
            res.push_str(&format!("{name}: {v}"));
        }
    }
    for (name, val) in &h {
        res.push_str(&format!("{name}: {val}"));
    }

    match g {
        Some(txt) => println!("{txt}"),
        None => println!("no idea"),
    }

    println!("route  => {}", w.as_str());
    println!("connect to => {}", u.ip());
    println!("original uri => {}", d.path());
    println!(
        "{a}, {v:?}, {}, {}, {}",
        c.as_str(),
        l.path(),
        l.query().unwrap()
    );
    println!("raw bytes => {:?}", z.size_hint());

    Json(serde_json::json!({
        "version": x.version,
        "desc": x.desc,
    }))
}

async fn handle_form(x: Form<HelloApi>) -> Json<serde_json::Value> {
    // async fn handle_form(x: RawForm) -> Json<serde_json::Value> {}
    // Option<Json<HelloApi>>, Result<Json<HelloApi>, ()>,
    Json(serde_json::json!({
        "version": x.version,
        "desc": x.desc,
    }))
}

// 4. custom extractor
struct MyKey(String);

enum MyKeyReject {
    Missing,
    Malformed,
}

impl IntoResponse for MyKeyReject {
    fn into_response(self) -> Response {
        let (s, m) = match self {
            Self::Missing => (StatusCode::UNAUTHORIZED, "missing key"),
            Self::Malformed => (StatusCode::BAD_REQUEST, "wrong format"),
        };
        (s, Json(serde_json::json!({"error": m}))).into_response()
    }
}

impl<S> OptionalFromRequestParts<S> for MyKey
where
    S: Send + Sync,
{
    type Rejection = MyKeyReject;
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &S,
    ) -> Result<Option<Self>, Self::Rejection> {
        if parts.headers.contains_key("x-api-key") {
            <MyKey as FromRequestParts<S>>::from_request_parts(parts, state)
                .await
                .map(Some)
        } else {
            Ok(None)
        }
    }
}

impl<S> FromRequestParts<S> for MyKey
where
    S: Send + Sync,
{
    type Rejection = MyKeyReject;
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let val = parts
            .headers
            .get("x-api-key")
            .ok_or(MyKeyReject::Missing)?
            .to_str()
            .map_err(|_| MyKeyReject::Malformed)?;
        if val.len() > 10 {
            Ok(Self(val.to_string()))
        } else {
            Err(MyKeyReject::Missing)
        }
    }
}

struct MyLine(Vec<String>);

#[allow(unused)]
enum MyLineReject {
    WrongType,
    BadBody(BytesRejection),
    NotUtf8,
}

impl IntoResponse for MyLineReject {
    fn into_response(self) -> Response {
        todo!()
    }
}

impl<S> FromRequest<S> for MyLine
where
    S: Send + Sync,
{
    type Rejection = MyLineReject;
    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let is_text = req
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|x| x.to_str().ok())
            .is_some_and(|x| x.starts_with("text/plain"));
        if !is_text {
            return Err(MyLineReject::WrongType);
        }
        let bytes = Bytes::from_request(req, state)
            .await
            .map_err(MyLineReject::BadBody)?;
        let text = std::str::from_utf8(&bytes).map_err(|_| MyLineReject::NotUtf8)?;
        Ok(MyLine(
            text.lines()
                .map(str::trim)
                .filter(|x| !x.is_empty())
                .map(String::from)
                .collect(),
        ))
    }
}

struct MyClient {
    addr: SocketAddr,
    agent: String,
}

impl<S> FromRequestParts<S> for MyClient
where
    S: Send + Sync,
{
    type Rejection = Response;
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        let ConnectInfo(addr) = ConnectInfo::<SocketAddr>::from_request_parts(parts, state)
            .await
            .map_err(IntoResponse::into_response)?;
        let agent = parts
            .headers
            .get(header::USER_AGENT)
            .and_then(|x| x.to_str().ok())
            .unwrap_or("unknown")
            .to_string();
        Ok(Self { addr, agent })
    }
}

async fn handle_key(client: MyClient, MyKey(x): MyKey, MyLine(y): MyLine) -> String {
    format!("{}, {}, {x} {}", client.addr, client.agent, y.join(","))
}

// 5. request
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

// 6. service
async fn handle_service(req: Request) -> Result<Response, Infallible> {
    let body = format!("url => {}", req.uri().path());
    let mut res = (StatusCode::OK, Body::from(body)).into_response();
    res.headers_mut()
        .insert("x-stewartli", HeaderValue::from_static("service_fn"));
    Ok(res)
}
