use std::sync::Arc;

use axum::{
    Router as AxumRouter,
    body::Body,
    extract::Request,
    response::Response,
};
use http::StatusCode;
use orpc::Router;
use orpc_server::rpc;

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

    if parts.method != http::Method::POST {
        let err = orpc::ORPCError::new(
            orpc::ErrorCode::MethodNotAllowed,
            "Only POST is allowed for RPC",
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

    let input = match rpc::decode_rpc_request(&body_bytes) {
        Ok(input) => input,
        Err(err) => {
            let (status, body) = rpc::encode_rpc_error(&err);
            return json_response(status, body);
        }
    };

    let ctx = ctx_fn(&parts);
    let (status, body) = rpc::execute_rpc(procedure, ctx, input).await;
    json_response(status, body)
}

fn json_response(status: StatusCode, body: Vec<u8>) -> Response {
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap()
}
