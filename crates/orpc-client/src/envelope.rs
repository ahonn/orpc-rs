use serde::{Deserialize, Serialize};

/// Wire format envelope for oRPC RPC protocol.
///
/// Duplicated from `orpc-server::rpc::RpcEnvelope` to avoid depending on `orpc-server`.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct RpcEnvelope<T> {
    pub json: T,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub meta: Vec<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_with_value() {
        let envelope = RpcEnvelope {
            json: serde_json::json!("hello"),
            meta: vec![],
        };
        let bytes = serde_json::to_vec(&envelope).unwrap();
        let parsed: RpcEnvelope<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(parsed.json, serde_json::json!("hello"));
        assert!(parsed.meta.is_empty());
    }

    #[test]
    fn meta_omitted_when_empty() {
        let envelope = RpcEnvelope {
            json: 42,
            meta: vec![],
        };
        let json_str = serde_json::to_string(&envelope).unwrap();
        assert!(!json_str.contains("meta"));
    }

    #[test]
    fn meta_included_when_present() {
        let envelope = RpcEnvelope {
            json: "x",
            meta: vec![serde_json::json!([1, "date"])],
        };
        let json_str = serde_json::to_string(&envelope).unwrap();
        assert!(json_str.contains("meta"));
    }
}
