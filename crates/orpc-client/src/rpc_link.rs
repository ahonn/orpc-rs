use crate::envelope::RpcEnvelope;
use crate::error::ClientError;
use crate::link::{Link, ValueStream};
use crate::sse;

/// HTTP transport for the oRPC RPC protocol.
///
/// Converts dot-separated procedure paths to URL paths and sends
/// HTTP POST requests matching the `@orpc/client` wire format.
///
/// # Example
/// ```ignore
/// let link = RpcLink::new("http://localhost:3000/rpc");
/// let client = Client::with_link(link);
/// ```
pub struct RpcLink {
    http: reqwest::Client,
    base_url: String,
}

impl RpcLink {
    /// Create a new RPC link with the given base URL.
    ///
    /// Uses a default `reqwest::Client`.
    pub fn new(base_url: impl Into<String>) -> Self {
        RpcLink {
            http: reqwest::Client::new(),
            base_url: base_url.into(),
        }
    }

    /// Use a custom `reqwest::Client` (for proxy, TLS, auth, etc.).
    pub fn with_client(mut self, client: reqwest::Client) -> Self {
        self.http = client;
        self
    }

    fn build_url(&self, path: &str) -> String {
        let path_url = path.replace('.', "/");
        let base = self.base_url.trim_end_matches('/');
        format!("{base}/{path_url}")
    }

    fn build_body(input: serde_json::Value) -> serde_json::Value {
        if input.is_null() {
            // No input: send {} (matches @orpc/client behavior)
            serde_json::json!({})
        } else {
            serde_json::json!({ "json": input })
        }
    }
}

impl Link for RpcLink {
    async fn call(
        &self,
        path: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, ClientError> {
        let url = self.build_url(path);
        let body = Self::build_body(input);

        let response = self
            .http
            .post(&url)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;

        let status = response.status();
        let bytes = response.bytes().await?;

        if status.is_success() {
            let envelope: RpcEnvelope<serde_json::Value> =
                serde_json::from_slice(&bytes).map_err(ClientError::Deserialize)?;
            Ok(envelope.json)
        } else {
            let envelope: RpcEnvelope<orpc::ORPCError> =
                serde_json::from_slice(&bytes).map_err(ClientError::Deserialize)?;
            Err(ClientError::Rpc(envelope.json))
        }
    }

    async fn subscribe(
        &self,
        path: &str,
        input: serde_json::Value,
        last_event_id: Option<u64>,
    ) -> Result<ValueStream, ClientError> {
        let url = self.build_url(path);
        let body = Self::build_body(input);

        let mut request = self
            .http
            .post(&url)
            .header("content-type", "application/json")
            .json(&body);

        if let Some(id) = last_event_id {
            request = request.header("last-event-id", id.to_string());
        }

        let response = request.send().await?;
        let status = response.status();

        if !status.is_success() {
            let bytes = response.bytes().await?;
            let envelope: RpcEnvelope<orpc::ORPCError> =
                serde_json::from_slice(&bytes).map_err(ClientError::Deserialize)?;
            return Err(ClientError::Rpc(envelope.json));
        }

        let byte_stream = response.bytes_stream();
        Ok(sse::sse_to_values(byte_stream))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_url_simple() {
        let link = RpcLink::new("http://localhost:3000/rpc");
        assert_eq!(link.build_url("ping"), "http://localhost:3000/rpc/ping");
    }

    #[test]
    fn build_url_nested() {
        let link = RpcLink::new("http://localhost:3000/rpc");
        assert_eq!(
            link.build_url("planet.find"),
            "http://localhost:3000/rpc/planet/find"
        );
    }

    #[test]
    fn build_url_trailing_slash() {
        let link = RpcLink::new("http://localhost:3000/rpc/");
        assert_eq!(link.build_url("ping"), "http://localhost:3000/rpc/ping");
    }

    #[test]
    fn build_body_null_input() {
        let body = RpcLink::build_body(serde_json::Value::Null);
        assert_eq!(body, serde_json::json!({}));
    }

    #[test]
    fn build_body_with_input() {
        let body = RpcLink::build_body(serde_json::json!("Earth"));
        assert_eq!(body, serde_json::json!({"json": "Earth"}));
    }

    #[test]
    fn build_body_with_object() {
        let body = RpcLink::build_body(serde_json::json!({"name": "Earth"}));
        assert_eq!(body, serde_json::json!({"json": {"name": "Earth"}}));
    }
}
