use std::fmt;

use orpc::ORPCError;

/// Unified error type for the oRPC client.
///
/// Covers transport errors (network), wire protocol errors (bad JSON),
/// and application-level RPC errors from the server.
#[derive(Debug)]
pub enum ClientError {
    /// HTTP transport error (network, DNS, TLS, timeout).
    Transport(reqwest::Error),
    /// Failed to serialize the request input.
    Serialize(serde_json::Error),
    /// Failed to deserialize the response body.
    Deserialize(serde_json::Error),
    /// Server returned an oRPC error response (4xx/5xx).
    Rpc(ORPCError),
    /// SSE stream protocol error (malformed event, unexpected format).
    Sse(String),
}

impl fmt::Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ClientError::Transport(e) => write!(f, "transport error: {e}"),
            ClientError::Serialize(e) => write!(f, "serialize error: {e}"),
            ClientError::Deserialize(e) => write!(f, "deserialize error: {e}"),
            ClientError::Rpc(e) => write!(f, "rpc error: {e}"),
            ClientError::Sse(msg) => write!(f, "sse error: {msg}"),
        }
    }
}

impl std::error::Error for ClientError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ClientError::Transport(e) => Some(e),
            ClientError::Serialize(e) => Some(e),
            ClientError::Deserialize(e) => Some(e),
            ClientError::Rpc(e) => Some(e),
            _ => None,
        }
    }
}

impl From<reqwest::Error> for ClientError {
    fn from(err: reqwest::Error) -> Self {
        ClientError::Transport(err)
    }
}

impl From<ORPCError> for ClientError {
    fn from(err: ORPCError) -> Self {
        ClientError::Rpc(err)
    }
}
