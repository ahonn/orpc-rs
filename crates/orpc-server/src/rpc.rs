use futures_util::StreamExt;
use http::StatusCode;
use orpc::ORPCError;
use orpc_procedure::{DynInput, DynOutput, ErasedProcedure, ProcedureError, SerializeError};
use serde::{Deserialize, Serialize};

/// Wire format envelope for oRPC RPC protocol.
///
/// Request from `@orpc/client`:
/// - No input: `{}`
/// - With input: `{"json": <data>}`
/// - With special types: `{"json": <data>, "meta": [...]}`
///
/// Response from server always includes `json`; `meta` is omitted when empty.
#[derive(Debug, Serialize, Deserialize)]
pub struct RpcEnvelope<T> {
    pub json: T,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub meta: Vec<serde_json::Value>,
}

/// Convert a URL path to a router procedure key.
///
/// Strips the configured prefix, then replaces `/` with `.`.
/// Returns `None` if the path doesn't start with the prefix or is empty after stripping.
///
/// # Examples
/// - `"/rpc/planet/list"` with prefix `"/rpc"` → `Some("planet.list")`
/// - `"/rpc/ping"` with prefix `"/rpc"` → `Some("ping")`
/// - `"/other/path"` with prefix `"/rpc"` → `None`
pub fn path_to_procedure_key(path: &str, prefix: &str) -> Option<String> {
    let stripped = path.strip_prefix(prefix)?;
    let stripped = stripped.strip_prefix('/').unwrap_or(stripped);
    if stripped.is_empty() {
        return None;
    }
    Some(stripped.replace('/', "."))
}

/// Decode an RPC request body into a `DynInput`.
///
/// Handles the actual `@orpc/client` wire format:
/// - `{}` → no input (null)
/// - `{"json": <value>}` → input value
/// - `{"json": <value>, "meta": [...]}` → input with type metadata
pub fn decode_rpc_request(body: &[u8]) -> Result<DynInput, ORPCError> {
    if body.is_empty() {
        return Ok(DynInput::from_value(serde_json::Value::Null));
    }

    #[derive(Deserialize)]
    struct Wire {
        json: Option<serde_json::Value>,
        #[serde(default)]
        meta: Vec<serde_json::Value>,
    }

    let wire: Wire = serde_json::from_slice(body)
        .map_err(|e| ORPCError::bad_request(format!("Invalid request body: {e}")))?;

    // `{}` (no json field) means undefined/no input → treat as null
    let _ = wire.meta; // meta is ignored for Phase 2a
    match wire.json {
        Some(value) => Ok(DynInput::from_value(value)),
        None => Ok(DynInput::from_value(serde_json::Value::Null)),
    }
}

/// Encode a successful `DynOutput` as an RPC response.
///
/// Produces `(HTTP 200, {"json": <value>, "meta": []})`.
pub fn encode_rpc_success(output: DynOutput) -> Result<(StatusCode, Vec<u8>), ProcedureError> {
    let value = output.to_value()?;
    let envelope = RpcEnvelope {
        json: value,
        meta: vec![],
    };
    let body = serde_json::to_vec(&envelope)
        .map_err(|e| ProcedureError::Serialize(SerializeError::from(e)))?;
    Ok((StatusCode::OK, body))
}

/// Convert a `ProcedureError` into an `ORPCError` for wire transmission.
///
/// - `Resolver` → attempts downcast to `ORPCError`, falls back to 500
/// - `Deserialize` → `BAD_REQUEST` (400)
/// - `Serialize` → `INTERNAL_SERVER_ERROR` (500)
/// - `Unwind` → `INTERNAL_SERVER_ERROR` (500)
pub fn procedure_error_to_orpc_error(err: ProcedureError) -> ORPCError {
    match err {
        ProcedureError::Resolver(boxed) => {
            // Try to downcast Box<dyn Error + Send + Sync> to ORPCError.
            // This works because ORPCError: Error + 'static.
            let boxed: Box<dyn std::error::Error> = boxed;
            match boxed.downcast::<ORPCError>() {
                Ok(orpc_err) => *orpc_err,
                Err(_) => ORPCError::internal_server_error("Internal server error"),
            }
        }
        ProcedureError::Deserialize(e) => {
            ORPCError::bad_request(format!("Bad request: {e}"))
        }
        ProcedureError::Serialize(_) => {
            ORPCError::internal_server_error("Response serialization failed")
        }
        ProcedureError::Unwind(_) => {
            ORPCError::internal_server_error("Internal server error")
        }
    }
}

/// Encode an `ORPCError` as an RPC error response.
///
/// Produces `(HTTP status, {"json": <orpc_error>, "meta": []})`.
pub fn encode_rpc_error(err: &ORPCError) -> (StatusCode, Vec<u8>) {
    let status = StatusCode::from_u16(err.status)
        .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let envelope = RpcEnvelope {
        json: serde_json::to_value(err).unwrap_or_default(),
        meta: vec![],
    };
    let body = serde_json::to_vec(&envelope).unwrap_or_default();
    (status, body)
}

/// Execute a procedure and produce the RPC response.
///
/// For Phase 2a MVP, takes only the first item from `ProcedureStream`.
/// Full streaming (SSE) is deferred to Phase 2c.
pub async fn execute_rpc<TCtx>(
    procedure: &ErasedProcedure<TCtx>,
    ctx: TCtx,
    input: DynInput,
) -> (StatusCode, Vec<u8>) {
    let mut stream = procedure.exec(ctx, input);
    match stream.next().await {
        Some(Ok(output)) => match encode_rpc_success(output) {
            Ok(result) => result,
            Err(err) => {
                let orpc_err = procedure_error_to_orpc_error(err);
                encode_rpc_error(&orpc_err)
            }
        },
        Some(Err(err)) => {
            let orpc_err = procedure_error_to_orpc_error(err);
            encode_rpc_error(&orpc_err)
        }
        None => {
            let err = ORPCError::internal_server_error("Procedure returned no output");
            encode_rpc_error(&err)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use orpc::ErrorCode;
    use orpc_procedure::{DeserializeError, ProcedureStream, Route, Meta};

    #[test]
    fn path_to_key_basic() {
        assert_eq!(
            path_to_procedure_key("/rpc/ping", "/rpc"),
            Some("ping".into())
        );
    }

    #[test]
    fn path_to_key_nested() {
        assert_eq!(
            path_to_procedure_key("/rpc/planet/list", "/rpc"),
            Some("planet.list".into())
        );
    }

    #[test]
    fn path_to_key_deeply_nested() {
        assert_eq!(
            path_to_procedure_key("/rpc/api/v1/users", "/rpc"),
            Some("api.v1.users".into())
        );
    }

    #[test]
    fn path_to_key_wrong_prefix() {
        assert_eq!(path_to_procedure_key("/other/ping", "/rpc"), None);
    }

    #[test]
    fn path_to_key_empty_after_prefix() {
        assert_eq!(path_to_procedure_key("/rpc", "/rpc"), None);
        assert_eq!(path_to_procedure_key("/rpc/", "/rpc"), None);
    }

    #[test]
    fn path_to_key_no_prefix() {
        assert_eq!(
            path_to_procedure_key("/ping", ""),
            Some("ping".into())
        );
    }

    #[test]
    fn decode_with_json_and_meta() {
        let body = br#"{"json": {"name": "World"}, "meta": []}"#;
        let input = decode_rpc_request(body).unwrap();
        assert_eq!(
            input.as_value().unwrap(),
            &serde_json::json!({"name": "World"})
        );
    }

    #[test]
    fn decode_with_json_only() {
        // @orpc/client sends this format when there are no special types
        let body = br#"{"json": "Earth"}"#;
        let input = decode_rpc_request(body).unwrap();
        assert_eq!(input.as_value().unwrap(), &serde_json::json!("Earth"));
    }

    #[test]
    fn decode_null_input() {
        let body = br#"{"json": null}"#;
        let input = decode_rpc_request(body).unwrap();
        assert_eq!(input.as_value().unwrap(), &serde_json::Value::Null);
    }

    #[test]
    fn decode_empty_object_no_input() {
        // @orpc/client sends {} when procedure has no input (undefined)
        let body = br#"{}"#;
        let input = decode_rpc_request(body).unwrap();
        assert_eq!(input.as_value().unwrap(), &serde_json::Value::Null);
    }

    #[test]
    fn decode_empty_body() {
        let body = b"";
        let input = decode_rpc_request(body).unwrap();
        assert_eq!(input.as_value().unwrap(), &serde_json::Value::Null);
    }

    #[test]
    fn decode_invalid_json() {
        let body = b"not json";
        let err = decode_rpc_request(body).unwrap_err();
        assert_eq!(err.code, ErrorCode::BadRequest);
    }

    #[test]
    fn encode_success_envelope() {
        let output = DynOutput::new("hello");
        let (status, body) = encode_rpc_success(output).unwrap();
        assert_eq!(status, StatusCode::OK);
        // meta is omitted when empty (skip_serializing_if)
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["json"], serde_json::json!("hello"));
        assert!(json.get("meta").is_none());
    }

    #[test]
    fn encode_error_envelope() {
        let err = ORPCError::not_found("User not found");
        let (status, body) = encode_rpc_error(&err);
        assert_eq!(status, StatusCode::NOT_FOUND);
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["json"]["code"], "NOT_FOUND");
        assert_eq!(json["json"]["status"], 404);
        assert_eq!(json["json"]["message"], "User not found");
    }

    #[test]
    fn error_mapping_resolver_orpc_error() {
        let orpc_err = ORPCError::not_found("gone");
        let proc_err = ProcedureError::Resolver(Box::new(orpc_err));
        let result = procedure_error_to_orpc_error(proc_err);
        assert_eq!(result.code, ErrorCode::NotFound);
        assert_eq!(result.message, "gone");
    }

    #[test]
    fn error_mapping_resolver_unknown() {
        let proc_err = ProcedureError::Resolver(Box::new(
            std::io::Error::new(std::io::ErrorKind::Other, "unknown"),
        ));
        let result = procedure_error_to_orpc_error(proc_err);
        assert_eq!(result.code, ErrorCode::InternalServerError);
    }

    #[test]
    fn error_mapping_deserialize() {
        let proc_err = ProcedureError::Deserialize(DeserializeError::from(
            serde_json::from_str::<String>("bad").unwrap_err(),
        ));
        let result = procedure_error_to_orpc_error(proc_err);
        assert_eq!(result.code, ErrorCode::BadRequest);
    }

    #[test]
    fn error_mapping_unwind() {
        let proc_err = ProcedureError::Unwind(Box::new("panic"));
        let result = procedure_error_to_orpc_error(proc_err);
        assert_eq!(result.code, ErrorCode::InternalServerError);
    }

    #[tokio::test]
    async fn execute_rpc_success() {
        let proc = ErasedProcedure::new(
            |_ctx: (), input: DynInput| {
                ProcedureStream::from_future(async move {
                    let val: String = input.deserialize()?;
                    Ok(DynOutput::new(format!("hello {val}")))
                })
            },
            Route::default(),
            Meta::default(),
        );

        let input = DynInput::from_value(serde_json::json!("world"));
        let (status, body) = execute_rpc(&proc, (), input).await;
        assert_eq!(status, StatusCode::OK);
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["json"], serde_json::json!("hello world"));
    }

    #[tokio::test]
    async fn execute_rpc_handler_error() {
        let proc = ErasedProcedure::new(
            |_ctx: (), _input: DynInput| {
                ProcedureStream::from_future(async move {
                    Err(ProcedureError::from(ORPCError::not_found("nope")))
                })
            },
            Route::default(),
            Meta::default(),
        );

        let input = DynInput::from_value(serde_json::json!(null));
        let (status, body) = execute_rpc(&proc, (), input).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["json"]["code"], "NOT_FOUND");
    }

    #[tokio::test]
    async fn execute_rpc_empty_stream() {
        let proc = ErasedProcedure::new(
            |_ctx: (), _input: DynInput| ProcedureStream::from_stream(futures_util::stream::empty()),
            Route::default(),
            Meta::default(),
        );

        let input = DynInput::from_value(serde_json::json!(null));
        let (status, body) = execute_rpc(&proc, (), input).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["json"]["code"], "INTERNAL_SERVER_ERROR");
    }
}
