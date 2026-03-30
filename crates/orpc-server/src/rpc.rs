use std::pin::Pin;

use futures_core::Stream;
use futures_util::StreamExt;
use http::StatusCode;
use orpc::{ORPCError, ORPCFile};
use orpc_procedure::{
    DynInput, DynOutput, ErasedProcedure, ProcedureError, ProcedureStream, SerializeError,
};
use serde::{Deserialize, Serialize};

use crate::sse;

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

    let mut value = wire.json.unwrap_or(serde_json::Value::Null);

    if !wire.meta.is_empty() {
        let entries = crate::meta::parse_meta(&wire.meta)?;
        crate::meta::apply_meta(&mut value, &entries)?;
    }

    Ok(DynInput::from_value(value))
}

/// Pre-parsed file data from a multipart form upload.
#[derive(Debug)]
pub struct MultipartFile {
    /// Raw file bytes.
    pub data: Vec<u8>,
    /// Original filename from the `Content-Disposition` header.
    pub name: Option<String>,
    /// MIME type from the part's `Content-Type` header.
    pub content_type: Option<String>,
}

/// Decode a multipart RPC request into a `DynInput`.
///
/// The transport layer (e.g. `orpc-axum`) parses the multipart body into:
/// - `data_json`: the `"data"` text field containing the wire envelope
/// - `files`: numbered file parts (`"0"`, `"1"`, ...) in order
///
/// Wire envelope format (inside `data_json`):
/// ```json
/// {
///   "json": {"title": "Photo", "avatar": {}},
///   "meta": [],
///   "maps": [["avatar"]]
/// }
/// ```
///
/// Each `maps[i]` is a JSON path pointing to where file `i` should be
/// injected in the `json` tree. Files are serialized as `ORPCFile` objects.
pub fn decode_rpc_multipart_request(
    data_json: &[u8],
    files: Vec<MultipartFile>,
) -> Result<DynInput, ORPCError> {
    #[derive(Deserialize)]
    struct MultipartWire {
        json: Option<serde_json::Value>,
        #[serde(default)]
        meta: Vec<serde_json::Value>,
        #[serde(default)]
        maps: Vec<serde_json::Value>,
    }

    let wire: MultipartWire = serde_json::from_slice(data_json)
        .map_err(|e| ORPCError::bad_request(format!("Invalid multipart data field: {e}")))?;

    let mut value = wire.json.unwrap_or(serde_json::Value::Null);

    // Inject files at the paths specified by `maps`.
    for (i, map_entry) in wire.maps.iter().enumerate() {
        let path_segments = map_entry
            .as_array()
            .ok_or_else(|| ORPCError::bad_request("maps entry must be an array"))?;

        let path = crate::meta::parse_path(path_segments)?;

        let file = files.get(i).ok_or_else(|| {
            ORPCError::bad_request(format!(
                "maps references file {i} but only {} files provided",
                files.len()
            ))
        })?;

        let orpc_file = ORPCFile::new(file.data.clone())
            .with_name(file.name.clone().unwrap_or_default())
            .with_content_type(file.content_type.clone().unwrap_or_default());

        let file_json = serde_json::to_value(&orpc_file).map_err(|e| {
            ORPCError::internal_server_error(format!("Failed to serialize file: {e}"))
        })?;

        let target = crate::meta::navigate_mut(&mut value, &path)?;
        *target = file_json;
    }

    // Apply meta transformations (BigInt, Date, etc.)
    if !wire.meta.is_empty() {
        let entries = crate::meta::parse_meta(&wire.meta)?;
        crate::meta::apply_meta(&mut value, &entries)?;
    }

    Ok(DynInput::from_value(value))
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
        ProcedureError::Deserialize(e) => ORPCError::bad_request(format!("Bad request: {e}")),
        ProcedureError::Serialize(_) => {
            ORPCError::internal_server_error("Response serialization failed")
        }
        ProcedureError::Unwind(_) => ORPCError::internal_server_error("Internal server error"),
    }
}

/// Encode an `ORPCError` as an RPC error response.
///
/// Produces `(HTTP status, {"json": <orpc_error>, "meta": []})`.
pub fn encode_rpc_error(err: &ORPCError) -> (StatusCode, Vec<u8>) {
    let status = StatusCode::from_u16(err.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
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

/// Response from [`execute_rpc_auto`]: either a single JSON body or an SSE stream.
pub enum RpcResponse {
    /// Single-value response (query/mutation).
    Json { status: StatusCode, body: Vec<u8> },
    /// Streaming response (subscription).
    Sse {
        body_stream: Pin<Box<dyn Stream<Item = Result<String, std::io::Error>> + Send>>,
    },
}

/// Execute a procedure, auto-detecting single-value vs subscription.
///
/// - Single-value (`from_future`): awaits result, returns `RpcResponse::Json`
/// - Subscription (`from_stream`): returns `RpcResponse::Sse` immediately
///
/// `last_event_id` supports SSE reconnection: events start from `last_event_id + 1`.
pub async fn execute_rpc_auto<TCtx>(
    procedure: &ErasedProcedure<TCtx>,
    ctx: TCtx,
    input: DynInput,
    last_event_id: Option<u64>,
) -> RpcResponse {
    let stream = procedure.exec(ctx, input);

    if sse::is_subscription(&stream) {
        let start_id = last_event_id.map(|id| id + 1).unwrap_or(0);
        RpcResponse::Sse {
            body_stream: Box::pin(sse::stream_to_sse(stream, start_id)),
        }
    } else {
        let (status, body) = consume_single_value(stream).await;
        RpcResponse::Json { status, body }
    }
}

/// Consume the first item from a ProcedureStream and encode as JSON.
async fn consume_single_value(mut stream: ProcedureStream) -> (StatusCode, Vec<u8>) {
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
    use orpc_procedure::{DeserializeError, Meta, ProcedureStream, Route};

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
        assert_eq!(path_to_procedure_key("/ping", ""), Some("ping".into()));
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
        let proc_err = ProcedureError::Resolver(Box::new(std::io::Error::other("unknown")));
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

    // --- Multipart decode tests ---

    #[test]
    fn multipart_decode_single_file() {
        let data = br#"{"json":{"title":"Photo","avatar":{}},"meta":[],"maps":[["avatar"]]}"#;
        let files = vec![MultipartFile {
            data: b"file-bytes".to_vec(),
            name: Some("photo.png".into()),
            content_type: Some("image/png".into()),
        }];

        let input = decode_rpc_multipart_request(data, files).unwrap();
        let value = input.as_value().unwrap();
        assert_eq!(value["title"], "Photo");
        assert!(value["avatar"]["data"].is_string());
        assert_eq!(value["avatar"]["name"], "photo.png");
        assert_eq!(value["avatar"]["contentType"], "image/png");

        // Verify base64 roundtrip
        let file: orpc::ORPCFile = serde_json::from_value(value["avatar"].clone()).unwrap();
        assert_eq!(file.data, b"file-bytes");
    }

    #[test]
    fn multipart_decode_multiple_files() {
        let data = br#"{"json":{"images":[{},{}]},"meta":[],"maps":[["images",0],["images",1]]}"#;
        let files = vec![
            MultipartFile {
                data: b"img-0".to_vec(),
                name: Some("a.png".into()),
                content_type: Some("image/png".into()),
            },
            MultipartFile {
                data: b"img-1".to_vec(),
                name: Some("b.jpg".into()),
                content_type: Some("image/jpeg".into()),
            },
        ];

        let input = decode_rpc_multipart_request(data, files).unwrap();
        let value = input.as_value().unwrap();

        let file0: orpc::ORPCFile = serde_json::from_value(value["images"][0].clone()).unwrap();
        assert_eq!(file0.data, b"img-0");
        assert_eq!(file0.name.as_deref(), Some("a.png"));

        let file1: orpc::ORPCFile = serde_json::from_value(value["images"][1].clone()).unwrap();
        assert_eq!(file1.data, b"img-1");
        assert_eq!(file1.name.as_deref(), Some("b.jpg"));
    }

    #[test]
    fn multipart_decode_with_meta() {
        // Maps + meta together: file at "avatar", undefined at "deleted"
        let data = br#"{"json":{"name":"Earth","avatar":{},"deleted":null},"meta":[[3,"deleted"]],"maps":[["avatar"]]}"#;
        let files = vec![MultipartFile {
            data: b"pic".to_vec(),
            name: None,
            content_type: None,
        }];

        let input = decode_rpc_multipart_request(data, files).unwrap();
        let value = input.as_value().unwrap();
        assert_eq!(value["name"], "Earth");
        assert!(value["avatar"]["data"].is_string());
        // "deleted" should be removed by Undefined meta
        assert!(value.get("deleted").is_none());
    }

    #[test]
    fn multipart_decode_missing_file() {
        let data = br#"{"json":{"file":{}},"meta":[],"maps":[["file"]]}"#;
        let files = vec![]; // no files provided
        let result = decode_rpc_multipart_request(data, files);
        assert!(result.is_err());
    }

    #[test]
    fn multipart_decode_no_maps() {
        let data = br#"{"json":{"name":"test"},"meta":[],"maps":[]}"#;
        let files = vec![];
        let input = decode_rpc_multipart_request(data, files).unwrap();
        assert_eq!(input.as_value().unwrap()["name"], "test");
    }

    #[test]
    fn multipart_decode_invalid_data_json() {
        let data = b"not json";
        let result = decode_rpc_multipart_request(data, vec![]);
        assert!(result.is_err());
    }
}
