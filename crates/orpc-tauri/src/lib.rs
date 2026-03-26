use std::sync::Arc;

use futures_util::StreamExt;
use orpc::Router;
use orpc_server::rpc;
use serde::Deserialize;
use tauri::ipc::Channel;
use tauri::plugin::{Builder as PluginBuilder, TauriPlugin};
use tauri::{Manager, Runtime, State};

/// IPC request from the TauriLink.
#[derive(Debug, Deserialize)]
struct RpcRequest {
    path: String,
    input: serde_json::Value,
}

type BoxFuture<T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send>>;
type HandlerFn = dyn Fn(serde_json::Value) -> BoxFuture<serde_json::Value> + Send + Sync;
type SubscriptionFn = dyn Fn(serde_json::Value, Channel<serde_json::Value>) -> BoxFuture<()> + Send + Sync;

/// Type-erased handlers stored as Tauri managed state.
struct RpcHandler {
    call: Arc<HandlerFn>,
    subscribe: Arc<SubscriptionFn>,
}

/// Create a Tauri plugin that serves an oRPC router via IPC.
///
/// Registers two IPC commands:
/// - `plugin:orpc|handle_rpc` — request-response (queries, mutations)
/// - `plugin:orpc|handle_rpc_subscription` — streaming via Channel (subscriptions)
///
/// # Example
/// ```ignore
/// tauri::Builder::default()
///     .plugin(tauri_plugin_orpc::init(router, |app_handle| AppCtx { ... }))
///     .run(tauri::generate_context!())
///     .unwrap();
/// ```
pub fn init<TCtx, R, F>(router: Router<TCtx>, ctx_fn: F) -> TauriPlugin<R>
where
    TCtx: Send + Sync + 'static,
    R: Runtime,
    F: Fn(&tauri::AppHandle<R>) -> TCtx + Send + Sync + 'static,
{
    let router = Arc::new(router);
    let ctx_fn = Arc::new(ctx_fn);

    PluginBuilder::<R>::new("orpc")
        .invoke_handler(tauri::generate_handler![handle_rpc_call, handle_rpc_subscription])
        .setup(move |app, _api| {
            let router_call = router.clone();
            let ctx_fn_call = ctx_fn.clone();
            let app_handle_call = app.clone();

            let router_sub = router.clone();
            let ctx_fn_sub = ctx_fn.clone();
            let app_handle_sub = app.clone();

            let handler = RpcHandler {
                call: Arc::new(move |request: serde_json::Value| {
                    let router = router_call.clone();
                    let ctx_fn = ctx_fn_call.clone();
                    let app_handle = app_handle_call.clone();
                    Box::pin(async move {
                        execute_rpc(&router, &*ctx_fn, &app_handle, request).await
                    })
                }),
                subscribe: Arc::new(move |request: serde_json::Value, channel: Channel<serde_json::Value>| {
                    let router = router_sub.clone();
                    let ctx_fn = ctx_fn_sub.clone();
                    let app_handle = app_handle_sub.clone();
                    Box::pin(async move {
                        execute_subscription(&router, &*ctx_fn, &app_handle, request, channel).await;
                    })
                }),
            };

            app.manage(handler);
            Ok(())
        })
        .build()
}

/// Request-response handler for queries and mutations.
#[tauri::command]
async fn handle_rpc_call(
    handler: State<'_, RpcHandler>,
    request: serde_json::Value,
) -> Result<serde_json::Value, String> {
    Ok((handler.call)(request).await)
}

/// Streaming handler for subscriptions via Tauri Channel.
#[tauri::command]
async fn handle_rpc_subscription(
    handler: State<'_, RpcHandler>,
    request: serde_json::Value,
    channel: Channel<serde_json::Value>,
) -> Result<(), String> {
    (handler.subscribe)(request, channel).await;
    Ok(())
}

async fn execute_rpc<TCtx, R, F>(
    router: &Router<TCtx>,
    ctx_fn: &F,
    app_handle: &tauri::AppHandle<R>,
    request: serde_json::Value,
) -> serde_json::Value
where
    TCtx: Send + Sync + 'static,
    R: Runtime,
    F: Fn(&tauri::AppHandle<R>) -> TCtx,
{
    let req: RpcRequest = match serde_json::from_value(request) {
        Ok(r) => r,
        Err(e) => {
            return make_error_response(400, "BAD_REQUEST", &format!("Invalid request: {e}"));
        }
    };

    let procedure = match router.get(&req.path) {
        Some(p) => p,
        None => {
            return make_error_response(404, "NOT_FOUND", &format!("Procedure not found: {}", req.path));
        }
    };

    let input_bytes = serde_json::to_vec(&req.input).unwrap_or_default();
    let input = match rpc::decode_rpc_request(&input_bytes) {
        Ok(i) => i,
        Err(err) => {
            let (status, body) = rpc::encode_rpc_error(&err);
            return serde_json::json!({
                "status": status.as_u16(),
                "body": serde_json::from_slice::<serde_json::Value>(&body).unwrap_or_default()
            });
        }
    };

    let ctx = ctx_fn(app_handle);
    let (status, body) = rpc::execute_rpc(procedure, ctx, input).await;

    serde_json::json!({
        "status": status.as_u16(),
        "body": serde_json::from_slice::<serde_json::Value>(&body).unwrap_or_default()
    })
}

async fn execute_subscription<TCtx, R, F>(
    router: &Router<TCtx>,
    ctx_fn: &F,
    app_handle: &tauri::AppHandle<R>,
    request: serde_json::Value,
    channel: Channel<serde_json::Value>,
) where
    TCtx: Send + Sync + 'static,
    R: Runtime,
    F: Fn(&tauri::AppHandle<R>) -> TCtx,
{
    let req: RpcRequest = match serde_json::from_value(request) {
        Ok(r) => r,
        Err(e) => {
            let _ = channel.send(serde_json::json!({
                "event": "error",
                "data": { "code": "BAD_REQUEST", "message": format!("Invalid request: {e}") }
            }));
            return;
        }
    };

    let procedure = match router.get(&req.path) {
        Some(p) => p,
        None => {
            let _ = channel.send(serde_json::json!({
                "event": "error",
                "data": { "code": "NOT_FOUND", "message": format!("Procedure not found: {}", req.path) }
            }));
            return;
        }
    };

    let input_bytes = serde_json::to_vec(&req.input).unwrap_or_default();
    let input = match rpc::decode_rpc_request(&input_bytes) {
        Ok(i) => i,
        Err(err) => {
            let _ = channel.send(serde_json::json!({
                "event": "error",
                "data": { "code": "BAD_REQUEST", "message": err.message }
            }));
            return;
        }
    };

    let ctx = ctx_fn(app_handle);
    let mut stream = procedure.exec(ctx, input);

    let mut id: u64 = 0;
    while let Some(item) = stream.next().await {
        match item {
            Ok(output) => {
                let value = output.to_value().unwrap_or_default();
                let _ = channel.send(serde_json::json!({
                    "event": "message",
                    "id": id,
                    "data": { "json": value }
                }));
                id += 1;
            }
            Err(err) => {
                let orpc_err = rpc::procedure_error_to_orpc_error(err);
                let _ = channel.send(serde_json::json!({
                    "event": "error",
                    "data": { "json": serde_json::to_value(&orpc_err).unwrap_or_default() }
                }));
                return;
            }
        }
    }

    let _ = channel.send(serde_json::json!({ "event": "done" }));
}

fn make_error_response(status: u16, code: &str, message: &str) -> serde_json::Value {
    serde_json::json!({
        "status": status,
        "body": {
            "json": {
                "code": code,
                "status": status,
                "message": message,
                "defined": false
            }
        }
    })
}
