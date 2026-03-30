use std::future::Future;
use std::pin::Pin;

use futures_core::Stream;

use crate::error::ClientError;

/// A boxed stream of JSON values, used by [`Link::subscribe`].
pub type ValueStream = Pin<Box<dyn Stream<Item = Result<serde_json::Value, ClientError>> + Send>>;

/// Transport abstraction for oRPC client calls.
///
/// Mirrors the TypeScript `Link` interface from `@orpc/client`.
/// Implementors handle the actual HTTP (or IPC) communication.
///
/// Use [`RpcLink`](crate::RpcLink) for the standard HTTP RPC transport.
pub trait Link: Send + Sync {
    /// Execute a single-value RPC call (query or mutation).
    fn call(
        &self,
        path: &str,
        input: serde_json::Value,
    ) -> impl Future<Output = Result<serde_json::Value, ClientError>> + Send;

    /// Execute a subscription RPC call, returning an SSE stream of values.
    ///
    /// If `last_event_id` is provided, the server resumes from that event
    /// (SSE reconnection via `Last-Event-ID` header).
    fn subscribe(
        &self,
        path: &str,
        input: serde_json::Value,
        last_event_id: Option<u64>,
    ) -> impl Future<Output = Result<ValueStream, ClientError>> + Send;
}
