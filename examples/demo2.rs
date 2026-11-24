// Topic: response

use std::{convert::Infallible, time::Duration};

use axum::{
    Json, Router,
    body::{Body, Bytes},
    extract::Path,
    http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header},
    response::{AppendHeaders, Html, IntoResponse, IntoResponseParts, Response, ResponseParts},
    routing::get,
};

#[tokio::main]
async fn main() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080")
        .await
        .unwrap();

    let app = Router::new()
        .route(
            "/health",
            get(handle_header)
                .post(handle_build)
                .put(handle_csv)
                .patch(handle_cache),
        )
        .route(
            "/",
            get(handle_cookie)
                .put(handle_content_type)
                .patch(handle_mycreate)
                .post(handle_download),
        )
        .route("/stream", get(handle_stream));

    axum::serve(listener, app).await.unwrap();
}

// 1. header
// 1.1 builder
async fn handle_build() -> Response {
    Response::builder()
        .status(StatusCode::ACCEPTED)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .header("x-build", "build with rust axum")
        .body(Body::from("raw builder without helper\n"))
        .expect("header can not fail")
}

// 1.2 cookie
async fn handle_cookie() -> (AppendHeaders<[(HeaderName, HeaderValue); 1]>, &'static str) {
    (
        AppendHeaders([(
            header::SET_COOKIE,
            HeaderValue::from_static("name=li; HttpOnly"),
        )]),
        "you get cookie",
    )
}

// 1.3 accept: content-type
async fn handle_content_type(x: HeaderMap) -> Response {
    let acc = x
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    let data = serde_json::json!({
        "course": "axum", "topic": "response"
    });
    if acc.contains("application/json") {
        Json(data).into_response()
    } else if acc.contains("text/html") {
        Html("<h1>hello world</h1>").into_response()
    } else {
        format!("header => {acc}").into_response()
    }
}

// 1.4 download
async fn handle_download(Path(filename): Path<String>) -> Response {
    let file_content = "hello world you again\n";
    let file_name = filename
        .chars()
        .filter(|x| x.is_ascii_alphanumeric() || *x == '.' || *x == '-')
        .collect::<String>();
    let disposition = format!("attachment; filename = \"{file_name}\"");
    // inline [display] vs attachment [download]
    let res: Result<HeaderValue, _> = disposition.parse();
    match res {
        Ok(v) => (
            StatusCode::OK,
            [
                (header::CONTENT_DISPOSITION, v),
                (
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("application/octet-stream"),
                ),
            ],
            file_content,
        )
            .into_response(),
        Err(_) => (StatusCode::OK, Body::from(file_content)).into_response(),
    }
}

// 2. custom IntoResponseParts
struct TraceId {
    id: String,
}

impl IntoResponseParts for TraceId {
    type Error = (StatusCode, String);
    fn into_response_parts(self, mut res: ResponseParts) -> Result<ResponseParts, Self::Error> {
        let value = HeaderValue::from_str(&self.id).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("invalid trace id: {e}"),
            )
        })?;
        // modify header
        res.headers_mut()
            .insert(HeaderName::from_static("x-trace-id"), value);
        Ok(res)
    }
}

async fn handle_header() -> (
    StatusCode,
    TraceId,
    AppendHeaders<[(HeaderName, HeaderValue); 2]>,
    &'static str,
) {
    (
        StatusCode::IM_A_TEAPOT,
        TraceId {
            id: "trace_12".to_owned(),
        },
        AppendHeaders([
            (
                HeaderName::from_static("x-deprecation"),
                HeaderValue::from_static("hello"),
            ),
            (
                HeaderName::from_static("x-course"),
                HeaderValue::from_static("world"),
            ),
        ]),
        "header append example",
    )
}

enum MyCache {
    NoStore,
    Public,
    Private,
}

impl IntoResponseParts for MyCache {
    type Error = ();
    fn into_response_parts(self, mut res: ResponseParts) -> Result<ResponseParts, Self::Error> {
        let text = match self {
            Self::NoStore => String::from("nothing"),
            Self::Public => String::from("public"),
            Self::Private => String::from("private"),
        };
        let val =
            HeaderValue::from_str(&text).unwrap_or_else(|_| HeaderValue::from_static("no-store"));
        res.headers_mut().insert(header::CACHE_CONTROL, val);
        Ok(res)
    }
}

async fn handle_cache(Path(mode): Path<String>) -> impl IntoResponse {
    let res = match mode.as_str() {
        "public" => MyCache::Public,
        "private" => MyCache::Private,
        _ => MyCache::NoStore,
    };
    (res, format!("cache mode => {mode}\n"))
}

// 3. custom IntoResponse
struct MyCreate<T> {
    location: String,
    body: T,
}

impl<T: serde::Serialize> IntoResponse for MyCreate<T> {
    fn into_response(self) -> Response {
        match HeaderValue::from_str(&self.location) {
            Ok(l) => (
                StatusCode::CREATED,
                [(header::LOCATION, l)],
                Json(self.body),
            )
                .into_response(),
            Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "invalid location").into_response(),
        }
    }
}

async fn handle_mycreate(x: String) -> MyCreate<String> {
    // T: Handler impl FromRequestParts or FromRequest
    MyCreate {
        location: "no idea".to_string(),
        body: x,
    }
}

struct MyCsv(Vec<Vec<String>>);

impl IntoResponse for MyCsv {
    fn into_response(self) -> Response {
        let text = self
            .0
            .iter()
            .map(|row| row.join(","))
            .collect::<Vec<_>>()
            .join("\n");
        (
            [
                (
                    header::CONTENT_DISPOSITION,
                    "attachment; filename=\"data.csv\"",
                ),
                (header::CONTENT_TYPE, "text/csv; charset=utf-8"),
            ],
            text + "\n",
        )
            .into_response()
    }
}

async fn handle_csv() -> MyCsv {
    MyCsv(vec![])
}

// 4. stream
async fn handle_stream() -> Response {
    let chunks = futures::stream::unfold(1u32, async move |step| {
        if step > 5 {
            return None;
        }
        tokio::time::sleep(Duration::from_secs(10)).await;
        let chunk: Result<Bytes, Infallible> = Ok(Bytes::from(format!("chunk {step} of 5\n")));
        Some((chunk, step + 1))
    });
    (
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        Body::from_stream(chunks),
    )
        .into_response()
}
