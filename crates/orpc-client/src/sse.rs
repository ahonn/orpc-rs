use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use futures_core::Stream;

use crate::envelope::RpcEnvelope;
use crate::error::ClientError;

/// Parsed SSE event.
#[derive(Debug)]
struct SseEvent {
    event_type: String,
    data: String,
}

/// Parse a raw SSE event block into its fields.
///
/// An SSE block is text separated by `\n\n`, containing lines like:
/// `event: message`, `id: 0`, `data: {"json": ...}`
fn parse_event_block(block: &str) -> Option<SseEvent> {
    let mut event_type = String::new();
    let mut data_lines: Vec<&str> = Vec::new();

    for line in block.lines() {
        if let Some(value) = line.strip_prefix("event: ") {
            event_type = value.to_string();
        } else if let Some(value) = line.strip_prefix("data: ") {
            data_lines.push(value);
        } else if line == "data:" {
            data_lines.push("");
        } else if line.starts_with(':') || line.is_empty() {
            // Comment or blank line — ignore
            continue;
        }
    }

    if event_type.is_empty() {
        return None;
    }

    Some(SseEvent {
        event_type,
        data: data_lines.join("\n"),
    })
}

/// Convert a byte stream (from reqwest) into a stream of deserialized JSON values.
///
/// Handles the oRPC SSE protocol:
/// - `event: message` → yield the JSON value
/// - `event: error` → yield `Err(ClientError::Rpc(...))`
/// - `event: done` → end the stream
/// - Comments (`: ...`) → ignored (keep-alive)
pub(crate) fn sse_to_values(
    byte_stream: impl Stream<Item = Result<Bytes, reqwest::Error>> + Send + 'static,
) -> Pin<Box<dyn Stream<Item = Result<serde_json::Value, ClientError>> + Send>> {
    Box::pin(SseValueStream {
        inner: Box::pin(byte_stream),
        buffer: String::new(),
        done: false,
        pending_values: Vec::new(),
    })
}

struct SseValueStream {
    inner: Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send>>,
    buffer: String,
    done: bool,
    pending_values: Vec<Result<serde_json::Value, ClientError>>,
}

impl Stream for SseValueStream {
    type Item = Result<serde_json::Value, ClientError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();

        // Yield buffered values first
        if !this.pending_values.is_empty() {
            return Poll::Ready(Some(this.pending_values.remove(0)));
        }

        if this.done {
            return Poll::Ready(None);
        }

        loop {
            // Try to extract complete events from buffer
            if let Some(result) = try_extract_event(&mut this.buffer, &mut this.done) {
                return Poll::Ready(Some(result));
            }

            if this.done {
                return Poll::Ready(None);
            }

            // Need more data from the byte stream
            match this.inner.as_mut().poll_next(cx) {
                Poll::Ready(Some(Ok(bytes))) => {
                    let text = String::from_utf8_lossy(&bytes);
                    this.buffer.push_str(&text);
                    // Loop back to try extracting events from the expanded buffer
                }
                Poll::Ready(Some(Err(e))) => {
                    return Poll::Ready(Some(Err(ClientError::Transport(e))));
                }
                Poll::Ready(None) => {
                    this.done = true;
                    return Poll::Ready(None);
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

/// Try to extract and process complete SSE events from the buffer.
///
/// Returns `Some(result)` if a meaningful event was found,
/// `None` if the buffer doesn't contain a complete event yet.
/// Skips comment-only blocks and unknown event types automatically.
/// Sets `done = true` if the stream should end.
fn try_extract_event(
    buffer: &mut String,
    done: &mut bool,
) -> Option<Result<serde_json::Value, ClientError>> {
    loop {
        // Look for double newline (event boundary)
        let boundary = buffer.find("\n\n")?;
        let block = buffer[..boundary].to_string();
        *buffer = buffer[boundary + 2..].to_string();

        let event = match parse_event_block(&block) {
            Some(e) => e,
            None => continue, // Comment-only block, try next
        };

        match event.event_type.as_str() {
            "message" => {
                let envelope: RpcEnvelope<serde_json::Value> =
                    match serde_json::from_str(&event.data) {
                        Ok(e) => e,
                        Err(e) => {
                            return Some(Err(ClientError::Sse(format!(
                                "invalid message data: {e}"
                            ))));
                        }
                    };
                return Some(Ok(envelope.json));
            }
            "error" => {
                *done = true;
                let envelope: RpcEnvelope<orpc::ORPCError> = match serde_json::from_str(&event.data)
                {
                    Ok(e) => e,
                    Err(e) => {
                        return Some(Err(ClientError::Sse(format!("invalid error data: {e}"))));
                    }
                };
                return Some(Err(ClientError::Rpc(envelope.json)));
            }
            "done" => {
                *done = true;
                return None;
            }
            _ => continue, // Unknown event type, try next
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::StreamExt;

    fn bytes_stream(
        chunks: Vec<&str>,
    ) -> impl Stream<Item = Result<Bytes, reqwest::Error>> + Send + 'static {
        futures_util::stream::iter(
            chunks
                .into_iter()
                .map(|s| Ok(Bytes::from(s.to_string())))
                .collect::<Vec<_>>(),
        )
    }

    #[tokio::test]
    async fn parse_single_message() {
        let stream = bytes_stream(vec![
            ": \n\n",
            "event: message\nid: 0\ndata: {\"json\":\"hello\"}\n\n",
        ]);
        let values: Vec<_> = sse_to_values(stream).collect().await;
        assert_eq!(values.len(), 1);
        assert_eq!(values[0].as_ref().unwrap(), &serde_json::json!("hello"));
    }

    #[tokio::test]
    async fn parse_multiple_messages() {
        let stream = bytes_stream(vec![
            ": \n\nevent: message\nid: 0\ndata: {\"json\":1}\n\nevent: message\nid: 1\ndata: {\"json\":2}\n\nevent: done\ndata:\n\n",
        ]);
        let values: Vec<_> = sse_to_values(stream).collect().await;
        assert_eq!(values.len(), 2);
        assert_eq!(values[0].as_ref().unwrap(), &serde_json::json!(1));
        assert_eq!(values[1].as_ref().unwrap(), &serde_json::json!(2));
    }

    #[tokio::test]
    async fn parse_error_event() {
        let stream = bytes_stream(vec![
            ": \n\nevent: error\ndata: {\"json\":{\"code\":\"NOT_FOUND\",\"status\":404,\"message\":\"gone\",\"defined\":false}}\n\n",
        ]);
        let values: Vec<_> = sse_to_values(stream).collect().await;
        assert_eq!(values.len(), 1);
        let err = values[0].as_ref().unwrap_err();
        assert!(matches!(err, ClientError::Rpc(e) if e.code == orpc::ErrorCode::NotFound));
    }

    #[tokio::test]
    async fn parse_done_ends_stream() {
        let stream = bytes_stream(vec![
            ": \n\nevent: message\nid: 0\ndata: {\"json\":\"a\"}\n\nevent: done\ndata:\n\n",
        ]);
        let values: Vec<_> = sse_to_values(stream).collect().await;
        assert_eq!(values.len(), 1);
        assert_eq!(values[0].as_ref().unwrap(), &serde_json::json!("a"));
    }

    #[tokio::test]
    async fn chunks_split_across_boundaries() {
        // Event split across two chunks
        let stream = bytes_stream(vec![
            ": \n\nevent: mes",
            "sage\nid: 0\ndata: {\"json\":42}\n\n",
        ]);
        let values: Vec<_> = sse_to_values(stream).collect().await;
        assert_eq!(values.len(), 1);
        assert_eq!(values[0].as_ref().unwrap(), &serde_json::json!(42));
    }

    #[tokio::test]
    async fn ignores_keepalive_comments() {
        let stream = bytes_stream(vec![
            ": \n\n: \n\n: \n\nevent: message\nid: 0\ndata: {\"json\":true}\n\nevent: done\ndata:\n\n",
        ]);
        let values: Vec<_> = sse_to_values(stream).collect().await;
        assert_eq!(values.len(), 1);
        assert_eq!(values[0].as_ref().unwrap(), &serde_json::json!(true));
    }

    #[tokio::test]
    async fn empty_stream() {
        let stream = bytes_stream(vec![]);
        let values: Vec<_> = sse_to_values(stream).collect().await;
        assert!(values.is_empty());
    }

    #[test]
    fn parse_multiline_data() {
        let block = "event: message\nid: 0\ndata: line1\ndata: line2\n";
        let event = super::parse_event_block(block).unwrap();
        assert_eq!(event.event_type, "message");
        assert_eq!(event.data, "line1\nline2");
    }

    #[test]
    fn parse_done_empty_data() {
        let block = "event: done\ndata:\n";
        let event = super::parse_event_block(block).unwrap();
        assert_eq!(event.event_type, "done");
        assert_eq!(event.data, "");
    }
}
