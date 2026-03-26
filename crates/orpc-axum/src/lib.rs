use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use axum::{
    Router as AxumRouter,
    body::Body,
    extract::Request,
    response::Response,
};
use futures_core::Stream;
use futures_util::StreamExt;
use http::StatusCode;
use orpc::Router;
use orpc_server::openapi::{self, RouteIndex};
use orpc_server::rpc::{self, RpcResponse};
use tokio::time::{Interval, interval};

/// Configuration for the oRPC axum integration.
pub struct ORPCConfig {
    /// URL prefix for RPC routes (e.g., "/rpc").
    /// Must start with `/` and must not end with `/`.
    pub prefix: String,
    /// Maximum request body size in bytes. Default: 10 MB.
    pub max_body_size: usize,
}

impl Default for ORPCConfig {
    fn default() -> Self {
        ORPCConfig {
            prefix: String::new(),
            max_body_size: 10 * 1024 * 1024,
        }
    }
}

/// Convert an oRPC `Router` into an Axum `Router`.
///
/// All procedures are served under the default prefix (root `/`).
/// Use [`into_router_with_config`] for a custom prefix.
///
/// # Arguments
/// - `router`: The oRPC Router containing all procedures.
/// - `ctx_fn`: Extracts the application context from HTTP request parts.
///
/// # Example
/// ```ignore
/// let app = orpc_axum::into_router(router, |_parts: &http::request::Parts| {
///     AppCtx { db: db_pool.clone() }
/// });
/// axum::serve(listener, app).await?;
/// ```
pub fn into_router<TCtx, F>(router: Router<TCtx>, ctx_fn: F) -> AxumRouter
where
    TCtx: Send + Sync + 'static,
    F: Fn(&http::request::Parts) -> TCtx + Clone + Send + Sync + 'static,
{
    into_router_with_config(router, ctx_fn, ORPCConfig::default())
}

/// Convert an oRPC `Router` into an Axum `Router` with custom configuration.
pub fn into_router_with_config<TCtx, F>(
    router: Router<TCtx>,
    ctx_fn: F,
    config: ORPCConfig,
) -> AxumRouter
where
    TCtx: Send + Sync + 'static,
    F: Fn(&http::request::Parts) -> TCtx + Clone + Send + Sync + 'static,
{
    let shared = Arc::new(SharedState { router, config });

    AxumRouter::new().fallback(move |req: Request| {
        let shared = shared.clone();
        let ctx_fn = ctx_fn.clone();
        async move { handle_rpc_request(shared, ctx_fn, req).await }
    })
}

struct SharedState<TCtx> {
    router: Router<TCtx>,
    config: ORPCConfig,
}

async fn handle_rpc_request<TCtx, F>(
    shared: Arc<SharedState<TCtx>>,
    ctx_fn: F,
    req: Request,
) -> Response
where
    TCtx: Send + Sync + 'static,
    F: Fn(&http::request::Parts) -> TCtx,
{
    let (parts, body) = req.into_parts();

    // RPC accepts POST (body) and GET (?data= query param)
    if parts.method != http::Method::POST && parts.method != http::Method::GET {
        let err = orpc::ORPCError::new(
            orpc::ErrorCode::MethodNotAllowed,
            "Only GET and POST are allowed for RPC",
        );
        let (status, body) = rpc::encode_rpc_error(&err);
        return json_response(status, body);
    }

    let path = parts.uri.path();
    let procedure_key = match rpc::path_to_procedure_key(path, &shared.config.prefix) {
        Some(key) => key,
        None => {
            let err = orpc::ORPCError::not_found(format!("Unknown path: {path}"));
            let (status, body) = rpc::encode_rpc_error(&err);
            return json_response(status, body);
        }
    };

    let procedure = match shared.router.get(&procedure_key) {
        Some(p) => p,
        None => {
            let err = orpc::ORPCError::not_found(format!(
                "Procedure not found: {procedure_key}"
            ));
            let (status, body) = rpc::encode_rpc_error(&err);
            return json_response(status, body);
        }
    };

    // GET: input from ?data= query param; POST: input from body
    let input = if parts.method == http::Method::GET {
        let data_param = parts.uri.query().and_then(extract_data_param);
        match data_param {
            Some(json_str) => match rpc::decode_rpc_request(json_str.as_bytes()) {
                Ok(input) => input,
                Err(err) => {
                    let (status, body) = rpc::encode_rpc_error(&err);
                    return json_response(status, body);
                }
            },
            None => orpc_procedure::DynInput::from_value(serde_json::Value::Null),
        }
    } else {
        let body_bytes = match axum::body::to_bytes(
            Body::new(body),
            shared.config.max_body_size,
        )
        .await
        {
            Ok(bytes) => bytes,
            Err(e) => {
                let err = orpc::ORPCError::bad_request(format!("Failed to read body: {e}"));
                let (status, body) = rpc::encode_rpc_error(&err);
                return json_response(status, body);
            }
        };

        match rpc::decode_rpc_request(&body_bytes) {
            Ok(input) => input,
            Err(err) => {
                let (status, body) = rpc::encode_rpc_error(&err);
                return json_response(status, body);
            }
        }
    };

    let last_event_id: Option<u64> = parts
        .headers
        .get("last-event-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok());

    let ctx = ctx_fn(&parts);

    match rpc::execute_rpc_auto(procedure, ctx, input, last_event_id).await {
        RpcResponse::Json { status, body } => json_response(status, body),
        RpcResponse::Sse { body_stream } => sse_response(body_stream),
    }
}

/// Extract the last `data` query parameter value from a query string.
/// Matches `@orpc/client`'s GET request format: `?data=<JSON.stringify(serialized)>`.
fn extract_data_param(query: &str) -> Option<String> {
    let mut result = None;
    for pair in query.split('&') {
        if let Some(value) = pair.strip_prefix("data=") {
            // URL-decode the value
            let decoded = percent_decode(value);
            result = Some(decoded);
        }
    }
    result
}

fn percent_decode(input: &str) -> String {
    let mut output = Vec::new();
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(
                &input[i + 1..i + 3],
                16,
            ) {
                output.push(byte);
                i += 3;
                continue;
            }
        } else if bytes[i] == b'+' {
            output.push(b' ');
            i += 1;
            continue;
        }
        output.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(output).unwrap_or_default()
}

fn json_response(status: StatusCode, body: Vec<u8>) -> Response {
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap()
}

/// Default SSE keep-alive interval (5 seconds), matching oRPC TypeScript server.
const SSE_KEEPALIVE_INTERVAL: Duration = Duration::from_secs(5);

fn sse_response(
    stream: Pin<Box<dyn Stream<Item = Result<String, std::io::Error>> + Send>>,
) -> Response {
    let stream = SseKeepAlive::new(stream, SSE_KEEPALIVE_INTERVAL);
    let body = Body::from_stream(stream.map(|r| r.map(bytes::Bytes::from)));
    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "text/event-stream")
        .header("cache-control", "no-cache")
        .body(body)
        .unwrap()
}

pin_project_lite::pin_project! {
    // Wraps an SSE stream with periodic keep-alive comments.
    // Emits `: \n\n` when no data has been sent within the interval,
    // preventing proxies and browsers from closing idle connections.
    struct SseKeepAlive<S> {
        #[pin]
        inner: S,
        keepalive: Interval,
        done: bool,
    }
}

impl<S> SseKeepAlive<S>
where
    S: Stream<Item = Result<String, std::io::Error>>,
{
    fn new(inner: S, keepalive_interval: Duration) -> Self {
        SseKeepAlive {
            inner,
            keepalive: interval(keepalive_interval),
            done: false,
        }
    }
}

impl<S> Stream for SseKeepAlive<S>
where
    S: Stream<Item = Result<String, std::io::Error>>,
{
    type Item = Result<String, std::io::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.project();

        if *this.done {
            return Poll::Ready(None);
        }

        // Data has priority over keep-alive
        match this.inner.poll_next(cx) {
            Poll::Ready(Some(item)) => {
                this.keepalive.reset();
                Poll::Ready(Some(item))
            }
            Poll::Ready(None) => {
                *this.done = true;
                Poll::Ready(None)
            }
            Poll::Pending => {
                // No data ready — check if keep-alive timer has fired
                match this.keepalive.poll_tick(cx) {
                    Poll::Ready(_) => Poll::Ready(Some(Ok(": \n\n".to_string()))),
                    Poll::Pending => Poll::Pending,
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// OpenAPI handler
// ---------------------------------------------------------------------------

/// Configuration for the OpenAPI handler.
pub struct OpenAPIConfig {
    /// URL prefix for API routes (e.g., "/api").
    pub prefix: String,
    /// Maximum request body size in bytes. Default: 10 MB.
    pub max_body_size: usize,
}

impl Default for OpenAPIConfig {
    fn default() -> Self {
        OpenAPIConfig {
            prefix: String::new(),
            max_body_size: 10 * 1024 * 1024,
        }
    }
}

/// Convert an oRPC `Router` into an Axum `Router` with REST-style OpenAPI routing.
///
/// Procedures with `Route` metadata (method + path) are served as REST endpoints.
/// Procedures without route metadata are not accessible via this handler.
///
/// # Example
/// ```ignore
/// // Procedures defined with Route metadata:
/// //   os::<AppCtx>().route(Route::get("/users/{id}")).handler(get_user)
/// //   os::<AppCtx>().route(Route::post("/users")).handler(create_user)
///
/// let app = orpc_axum::into_openapi_router(
///     router,
///     |_parts| AppCtx {},
///     OpenAPIConfig { prefix: "/api".into(), ..Default::default() },
/// );
/// // GET /api/users/123 → get_user with input { id: "123" }
/// // POST /api/users    → create_user with body
/// ```
pub fn into_openapi_router<TCtx, F>(
    router: Router<TCtx>,
    ctx_fn: F,
    config: OpenAPIConfig,
) -> AxumRouter
where
    TCtx: Send + Sync + 'static,
    F: Fn(&http::request::Parts) -> TCtx + Clone + Send + Sync + 'static,
{
    let route_index = RouteIndex::build(&router);
    let shared = Arc::new(OpenAPISharedState {
        router,
        route_index,
        config,
    });

    AxumRouter::new().fallback(move |req: Request| {
        let shared = shared.clone();
        let ctx_fn = ctx_fn.clone();
        async move { handle_openapi_request(shared, ctx_fn, req).await }
    })
}

struct OpenAPISharedState<TCtx> {
    router: Router<TCtx>,
    route_index: RouteIndex,
    config: OpenAPIConfig,
}

async fn handle_openapi_request<TCtx, F>(
    shared: Arc<OpenAPISharedState<TCtx>>,
    ctx_fn: F,
    req: Request,
) -> Response
where
    TCtx: Send + Sync + 'static,
    F: Fn(&http::request::Parts) -> TCtx,
{
    let (parts, body) = req.into_parts();

    let method = match openapi::http_method_to_orpc(&parts.method) {
        Some(m) => m,
        None => {
            let err = orpc::ORPCError::new(
                orpc::ErrorCode::MethodNotAllowed,
                format!("Unsupported method: {}", parts.method),
            );
            let (status, body) = openapi::encode_openapi_error(&err);
            return json_response(status, body);
        }
    };

    let path = parts.uri.path();
    let stripped_path = path
        .strip_prefix(&shared.config.prefix)
        .unwrap_or(path);

    let route_match = match shared.route_index.match_route(method, stripped_path) {
        Some(m) => m,
        None => {
            let err = orpc::ORPCError::not_found(format!("No route matches: {method} {path}"));
            let (status, body) = openapi::encode_openapi_error(&err);
            return json_response(status, body);
        }
    };

    let procedure = shared.router.get(route_match.procedure_key).unwrap();

    let body_bytes = match axum::body::to_bytes(
        Body::new(body),
        shared.config.max_body_size,
    )
    .await
    {
        Ok(bytes) => bytes,
        Err(e) => {
            let err = orpc::ORPCError::bad_request(format!("Failed to read body: {e}"));
            let (status, body) = openapi::encode_openapi_error(&err);
            return json_response(status, body);
        }
    };

    let input = match openapi::decode_openapi_request(
        &route_match.path_params,
        parts.uri.query(),
        &body_bytes,
        method,
    ) {
        Ok(input) => input,
        Err(err) => {
            let (status, body) = openapi::encode_openapi_error(&err);
            return json_response(status, body);
        }
    };

    let ctx = ctx_fn(&parts);
    let (status, body) = openapi::execute_openapi(procedure, ctx, input).await;
    json_response(status, body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn sse_keepalive_emits_comment_on_idle() {
        let inner = futures_util::stream::pending::<Result<String, std::io::Error>>();
        let stream = SseKeepAlive::new(inner, Duration::from_secs(5));
        tokio::pin!(stream);

        // With paused time, the runtime auto-advances to the next timer.
        // inner is Pending, so the keepalive fires at 5s.
        let item = stream.next().await.unwrap().unwrap();
        assert_eq!(item, ": \n\n");

        // Second keepalive fires at 10s
        let item = stream.next().await.unwrap().unwrap();
        assert_eq!(item, ": \n\n");
    }

    #[tokio::test]
    async fn sse_keepalive_passes_data_through() {
        let items: Vec<Result<String, std::io::Error>> = vec![
            Ok("event: message\ndata: 1\n\n".into()),
            Ok("event: done\ndata:\n\n".into()),
        ];
        let inner = futures_util::stream::iter(items);
        let stream = SseKeepAlive::new(inner, Duration::from_secs(5));
        let results: Vec<String> = stream.map(|r| r.unwrap()).collect().await;

        assert_eq!(results.len(), 2);
        assert!(results[0].contains("data: 1"));
        assert!(results[1].contains("event: done"));
    }

    #[tokio::test]
    async fn sse_keepalive_terminates_with_inner() {
        let inner = futures_util::stream::empty::<Result<String, std::io::Error>>();
        let stream = SseKeepAlive::new(inner, Duration::from_secs(5));
        let results: Vec<_> = stream.collect().await;
        assert!(results.is_empty());
    }
}
