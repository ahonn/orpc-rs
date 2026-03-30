use std::sync::Arc;

use futures_util::StreamExt;
use orpc::Router;
use orpc_procedure::ProcedureStream;
use orpc_server::rpc;
use orpc_server::sse;
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
type HandlerFn = dyn Fn(serde_json::Value, Channel<serde_json::Value>) -> BoxFuture<serde_json::Value>
    + Send
    + Sync;

/// Type-erased handler stored as Tauri managed state.
struct RpcHandler {
    handler: Arc<HandlerFn>,
}

/// Create a Tauri plugin that serves an oRPC router via IPC.
///
/// Registers a single IPC command `plugin:orpc|handle_rpc` that auto-detects
/// single-value vs subscription procedures. Single-value results are returned
/// directly; subscriptions are streamed via the Tauri Channel.
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
        .invoke_handler(tauri::generate_handler![handle_rpc])
        .setup(move |app, _api| {
            let router = router.clone();
            let ctx_fn = ctx_fn.clone();
            let app_handle = app.clone();

            app.manage(RpcHandler {
                handler: Arc::new(move |request, channel| {
                    let router = router.clone();
                    let ctx_fn = ctx_fn.clone();
                    let app_handle = app_handle.clone();
                    Box::pin(async move {
                        execute_rpc(&router, &*ctx_fn, &app_handle, request, channel).await
                    })
                }),
            });
            Ok(())
        })
        .build()
}

/// Unified handler: auto-detects single-value vs subscription.
///
/// For single-value procedures the JSON response is returned directly.
/// For subscriptions, streaming is spawned as a background task and a
/// `{"type": "subscription"}` marker is returned immediately while
/// events flow through the Channel.
#[tauri::command]
async fn handle_rpc(
    handler: State<'_, RpcHandler>,
    request: serde_json::Value,
    channel: Channel<serde_json::Value>,
) -> Result<serde_json::Value, String> {
    Ok((handler.handler)(request, channel).await)
}

/// Stream ProcedureStream items through a Tauri Channel.
///
/// Stops streaming when the channel is closed (frontend disconnected)
/// to avoid leaking background tasks and resources.
async fn stream_to_channel(mut stream: ProcedureStream, channel: Channel<serde_json::Value>) {
    let mut id: u64 = 0;
    while let Some(item) = stream.next().await {
        match item {
            Ok(output) => {
                let value = output.to_value().unwrap_or_default();
                if channel
                    .send(serde_json::json!({
                        "event": "message",
                        "id": id,
                        "data": { "json": value }
                    }))
                    .is_err()
                {
                    // Channel closed — frontend disconnected, stop streaming.
                    return;
                }
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

async fn execute_rpc<TCtx, R, F>(
    router: &Router<TCtx>,
    ctx_fn: &F,
    app_handle: &tauri::AppHandle<R>,
    request: serde_json::Value,
    channel: Channel<serde_json::Value>,
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
            return make_error_response(
                404,
                "NOT_FOUND",
                &format!("Procedure not found: {}", req.path),
            );
        }
    };

    let input_bytes = serde_json::to_vec(&req.input).unwrap_or_default();
    let input = match rpc::decode_rpc_request(&input_bytes) {
        Ok(i) => i,
        Err(err) => {
            let (status, body) = rpc::encode_rpc_error(&err);
            return serde_json::json!({
                "type": "response",
                "status": status.as_u16(),
                "body": serde_json::from_slice::<serde_json::Value>(&body).unwrap_or_default()
            });
        }
    };

    let ctx = ctx_fn(app_handle);
    let stream = procedure.exec(ctx, input);

    if sse::is_subscription(&stream) {
        tokio::spawn(async move {
            stream_to_channel(stream, channel).await;
        });
        return serde_json::json!({ "type": "subscription" });
    }

    // Single-value: consume first item
    let mut stream = stream;
    match stream.next().await {
        Some(Ok(output)) => match rpc::encode_rpc_success(output) {
            Ok((status, body)) => serde_json::json!({
                "type": "response",
                "status": status.as_u16(),
                "body": serde_json::from_slice::<serde_json::Value>(&body).unwrap_or_default()
            }),
            Err(err) => {
                let orpc_err = rpc::procedure_error_to_orpc_error(err);
                let (status, body) = rpc::encode_rpc_error(&orpc_err);
                serde_json::json!({
                    "type": "response",
                    "status": status.as_u16(),
                    "body": serde_json::from_slice::<serde_json::Value>(&body).unwrap_or_default()
                })
            }
        },
        Some(Err(err)) => {
            let orpc_err = rpc::procedure_error_to_orpc_error(err);
            let (status, body) = rpc::encode_rpc_error(&orpc_err);
            serde_json::json!({
                "type": "response",
                "status": status.as_u16(),
                "body": serde_json::from_slice::<serde_json::Value>(&body).unwrap_or_default()
            })
        }
        None => make_error_response(500, "INTERNAL_SERVER_ERROR", "Procedure returned no output"),
    }
}

fn make_error_response(status: u16, code: &str, message: &str) -> serde_json::Value {
    serde_json::json!({
        "type": "response",
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
