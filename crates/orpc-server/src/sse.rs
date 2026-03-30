use std::pin::Pin;
use std::task::{Context, Poll};

use futures_core::Stream;
use orpc::ORPCError;
use orpc_procedure::{DynOutput, ProcedureError, ProcedureStream};

use crate::rpc::{RpcEnvelope, encode_rpc_error, procedure_error_to_orpc_error};

/// Check if a `ProcedureStream` is a subscription (multi-value).
///
/// Uses `size_hint`: `from_future` returns `(1, Some(1))` (single-value),
/// while `from_stream` returns `(0, None)` or similar (multi-value).
pub fn is_subscription(stream: &ProcedureStream) -> bool {
    !matches!(stream.size_hint(), (_, Some(1)))
}

/// Format a single SSE event string.
///
/// Produces: `event: {event_type}\nid: {id}\ndata: {data}\n\n`
/// If `id` is `None`, the `id:` line is omitted.
/// If `data` is empty, just `data:\n` is emitted.
pub fn format_sse_event(event_type: &str, id: Option<u64>, data: &str) -> String {
    let mut out = format!("event: {event_type}\n");
    if let Some(id) = id {
        out.push_str(&format!("id: {id}\n"));
    }
    if data.is_empty() {
        out.push_str("data:\n");
    } else {
        out.push_str(&format!("data: {data}\n"));
    }
    out.push('\n');
    out
}

/// Encode a `DynOutput` as SSE message data payload.
///
/// Returns the JSON string: `{"json": <value>}`
fn encode_sse_data(output: DynOutput) -> Result<String, ProcedureError> {
    let value = output.to_value()?;
    let envelope = RpcEnvelope {
        json: value,
        meta: vec![],
    };
    serde_json::to_string(&envelope)
        .map_err(|e| ProcedureError::Serialize(orpc_procedure::SerializeError::from(e)))
}

/// Encode an `ORPCError` as SSE error data payload.
fn encode_sse_error_data(err: &ORPCError) -> String {
    let (_, body) = encode_rpc_error(err);
    String::from_utf8(body).unwrap_or_default()
}

/// Convert a `ProcedureStream` into a `Stream` of SSE-formatted string chunks.
///
/// Emits:
/// - `event: message` with sequential ids for each `Ok(DynOutput)`
/// - `event: error` for `Err(ProcedureError)` (then terminates)
/// - `event: done` when the stream completes naturally
pub fn stream_to_sse(stream: ProcedureStream, start_id: u64) -> SseStream {
    SseStream {
        inner: stream,
        next_id: start_id,
        done: false,
        needs_flush: true,
    }
}

/// A `Stream` that converts `ProcedureStream` items into SSE-formatted strings.
///
/// Emits an initial comment (`: \n\n`) to flush response headers, matching
/// the behavior of the oRPC TypeScript server.
///
/// # Client disconnect lifecycle
///
/// When a client disconnects, the HTTP server (e.g. hyper/axum) stops polling
/// the response body and drops it, which drops this stream and the inner
/// `ProcedureStream`. The `SseKeepAlive` wrapper in the transport layer emits
/// periodic keep-alive comments, ensuring the server detects disconnection
/// within the keep-alive interval (typically 5 seconds) even when the inner
/// stream is idle.
pub struct SseStream {
    inner: ProcedureStream,
    next_id: u64,
    done: bool,
    /// Emit initial comment on first poll to flush headers.
    needs_flush: bool,
}

// ProcedureStream wraps Pin<Box<dyn Stream + Send>>, so it is Unpin and Send.
// SseStream's remaining fields are plain data, so SseStream is also Unpin and Send.

impl Stream for SseStream {
    type Item = Result<String, std::io::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();

        if this.done {
            return Poll::Ready(None);
        }

        // Emit initial comment to flush response headers (matches TS behavior)
        if this.needs_flush {
            this.needs_flush = false;
            return Poll::Ready(Some(Ok(": \n\n".to_string())));
        }

        match Pin::new(&mut this.inner).poll_next(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(None) => {
                // Stream ended naturally — emit done event
                this.done = true;
                let event = format_sse_event("done", None, "");
                Poll::Ready(Some(Ok(event)))
            }
            Poll::Ready(Some(Ok(output))) => {
                let id = this.next_id;
                this.next_id += 1;
                match encode_sse_data(output) {
                    Ok(data) => {
                        let event = format_sse_event("message", Some(id), &data);
                        Poll::Ready(Some(Ok(event)))
                    }
                    Err(err) => {
                        this.done = true;
                        let orpc_err = procedure_error_to_orpc_error(err);
                        let data = encode_sse_error_data(&orpc_err);
                        let event = format_sse_event("error", None, &data);
                        Poll::Ready(Some(Ok(event)))
                    }
                }
            }
            Poll::Ready(Some(Err(err))) => {
                this.done = true;
                let orpc_err = procedure_error_to_orpc_error(err);
                let data = encode_sse_error_data(&orpc_err);
                let event = format_sse_event("error", None, &data);
                Poll::Ready(Some(Ok(event)))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::{StreamExt, stream};

    #[test]
    fn format_message_event() {
        let event = format_sse_event("message", Some(0), r#"{"json":"hello"}"#);
        assert_eq!(
            event,
            "event: message\nid: 0\ndata: {\"json\":\"hello\"}\n\n"
        );
    }

    #[test]
    fn format_done_event() {
        let event = format_sse_event("done", None, "");
        assert_eq!(event, "event: done\ndata:\n\n");
    }

    #[test]
    fn format_error_event() {
        let event = format_sse_event("error", None, r#"{"json":{"code":"ERR"}}"#);
        assert_eq!(
            event,
            "event: error\ndata: {\"json\":{\"code\":\"ERR\"}}\n\n"
        );
    }

    #[test]
    fn is_subscription_from_future() {
        let stream = ProcedureStream::from_future(async { Ok(DynOutput::new(1u32)) });
        assert!(!is_subscription(&stream));
    }

    #[test]
    fn is_subscription_from_stream() {
        let stream = ProcedureStream::from_stream(stream::empty());
        assert!(is_subscription(&stream));
    }

    #[tokio::test]
    async fn sse_stream_single_item() {
        let inner = ProcedureStream::from_stream(stream::iter(vec![Ok(DynOutput::new("hello"))]));
        let mut sse = stream_to_sse(inner, 0);

        // First chunk is the initial flush comment
        let flush = sse.next().await.unwrap().unwrap();
        assert_eq!(flush, ": \n\n");

        let msg = sse.next().await.unwrap().unwrap();
        assert!(msg.starts_with("event: message\n"));
        assert!(msg.contains("id: 0\n"));
        assert!(msg.contains("\"hello\""));

        let done = sse.next().await.unwrap().unwrap();
        assert!(done.starts_with("event: done\n"));

        assert!(sse.next().await.is_none());
    }

    #[tokio::test]
    async fn sse_stream_multiple_items() {
        let inner = ProcedureStream::from_stream(stream::iter(vec![
            Ok(DynOutput::new(1u32)),
            Ok(DynOutput::new(2u32)),
            Ok(DynOutput::new(3u32)),
        ]));
        let sse = stream_to_sse(inner, 0);
        let events: Vec<String> = sse.map(|r| r.unwrap()).collect().await;

        // flush comment + 3 messages + 1 done = 5
        assert_eq!(events.len(), 5);
        assert_eq!(events[0], ": \n\n");
        assert!(events[1].contains("id: 0\n"));
        assert!(events[2].contains("id: 1\n"));
        assert!(events[3].contains("id: 2\n"));
        assert!(events[4].starts_with("event: done\n"));
    }

    #[tokio::test]
    async fn sse_stream_with_start_id() {
        let inner = ProcedureStream::from_stream(stream::iter(vec![Ok(DynOutput::new("a"))]));
        let mut sse = stream_to_sse(inner, 5);

        let _ = sse.next().await; // flush comment
        let msg = sse.next().await.unwrap().unwrap();
        assert!(msg.contains("id: 5\n"));
    }

    #[tokio::test]
    async fn sse_stream_error() {
        let inner = ProcedureStream::from_stream(stream::iter(vec![
            Ok(DynOutput::new("ok")),
            Err(ProcedureError::from(ORPCError::not_found("gone"))),
        ]));
        let sse = stream_to_sse(inner, 0);
        let events: Vec<String> = sse.map(|r| r.unwrap()).collect().await;

        // flush + 1 message + 1 error (no done)
        assert_eq!(events.len(), 3);
        assert_eq!(events[0], ": \n\n");
        assert!(events[1].starts_with("event: message\n"));
        assert!(events[2].starts_with("event: error\n"));
        assert!(events[2].contains("NOT_FOUND"));
    }

    #[tokio::test]
    async fn sse_stream_empty() {
        let inner = ProcedureStream::from_stream(stream::empty());
        let sse = stream_to_sse(inner, 0);
        let events: Vec<String> = sse.map(|r| r.unwrap()).collect().await;

        // flush + done
        assert_eq!(events.len(), 2);
        assert_eq!(events[0], ": \n\n");
        assert!(events[1].starts_with("event: done\n"));
    }
}
