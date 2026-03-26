use std::collections::HashMap;

use futures_util::StreamExt;
use http::StatusCode;
use orpc::ORPCError;
use orpc_procedure::{DynInput, DynOutput, ErasedProcedure, HttpMethod, ProcedureError, SerializeError};

use crate::rpc::procedure_error_to_orpc_error;

/// A compiled path pattern segment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathSegment {
    Literal(String),
    Param(String),
}

/// A compiled route entry linking an HTTP method + path pattern to a procedure key.
#[derive(Debug)]
pub struct CompiledRoute {
    pub method: HttpMethod,
    pub segments: Vec<PathSegment>,
    pub procedure_key: String,
}

/// Route index for matching `(method, path)` → procedure.
///
/// Built from a `Router` by scanning all procedures that have `Route.method` and `Route.path` set.
pub struct RouteIndex {
    routes: Vec<CompiledRoute>,
}

/// Result of matching a request against the route index.
pub struct RouteMatch<'a> {
    pub procedure_key: &'a str,
    pub path_params: HashMap<String, String>,
}

impl RouteIndex {
    /// Build a route index from a Router's procedures.
    ///
    /// Only procedures with both `Route.method` and `Route.path` are indexed.
    pub fn build<TCtx>(router: &orpc::Router<TCtx>) -> Self {
        let mut routes = Vec::new();
        for (key, proc) in router.procedures() {
            if let (Some(method), Some(path)) = (proc.route.method, proc.route.path.as_deref()) {
                routes.push(CompiledRoute {
                    method,
                    segments: compile_path_pattern(path),
                    procedure_key: key.clone(),
                });
            }
        }
        // Sort: routes with more literal segments first for deterministic matching.
        routes.sort_by(|a, b| {
            let a_literals = a.segments.iter().filter(|s| matches!(s, PathSegment::Literal(_))).count();
            let b_literals = b.segments.iter().filter(|s| matches!(s, PathSegment::Literal(_))).count();
            b_literals.cmp(&a_literals)
        });
        RouteIndex { routes }
    }

    /// Match an HTTP request against the route index.
    pub fn match_route(&self, method: HttpMethod, path: &str) -> Option<RouteMatch<'_>> {
        for route in &self.routes {
            if route.method != method {
                continue;
            }
            if let Some(params) = match_path(&route.segments, path) {
                return Some(RouteMatch {
                    procedure_key: &route.procedure_key,
                    path_params: params,
                });
            }
        }
        None
    }
}

/// Parse a path pattern into segments.
///
/// Example: `"/users/{id}/posts/{post_id}"` → `[Literal("users"), Param("id"), Literal("posts"), Param("post_id")]`
pub fn compile_path_pattern(pattern: &str) -> Vec<PathSegment> {
    pattern
        .split('/')
        .filter(|s| !s.is_empty())
        .map(|seg| {
            if let Some(param) = seg.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
                PathSegment::Param(param.to_string())
            } else {
                PathSegment::Literal(seg.to_string())
            }
        })
        .collect()
}

/// Match a URL path against compiled segments, extracting path parameters.
pub fn match_path(
    segments: &[PathSegment],
    path: &str,
) -> Option<HashMap<String, String>> {
    let path_parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

    if path_parts.len() != segments.len() {
        return None;
    }

    let mut params = HashMap::new();
    for (segment, part) in segments.iter().zip(path_parts.iter()) {
        match segment {
            PathSegment::Literal(lit) => {
                if lit != part {
                    return None;
                }
            }
            PathSegment::Param(name) => {
                params.insert(name.clone(), (*part).to_string());
            }
        }
    }
    Some(params)
}

/// Convert `http::Method` to `HttpMethod`.
pub fn http_method_to_orpc(method: &http::Method) -> Option<HttpMethod> {
    match *method {
        http::Method::GET => Some(HttpMethod::Get),
        http::Method::POST => Some(HttpMethod::Post),
        http::Method::PUT => Some(HttpMethod::Put),
        http::Method::DELETE => Some(HttpMethod::Delete),
        http::Method::PATCH => Some(HttpMethod::Patch),
        http::Method::HEAD => Some(HttpMethod::Head),
        http::Method::OPTIONS => Some(HttpMethod::Options),
        _ => None,
    }
}

/// Decode an OpenAPI-style request into `DynInput`.
///
/// Merges path params, query params, and request body into a single JSON object.
/// - GET/DELETE/HEAD: input from path params + query params only
/// - POST/PUT/PATCH: input from path params + query params + body
pub fn decode_openapi_request(
    path_params: &HashMap<String, String>,
    query: Option<&str>,
    body: &[u8],
    method: HttpMethod,
) -> Result<DynInput, ORPCError> {
    let mut merged = serde_json::Map::new();

    // Path params
    for (k, v) in path_params {
        merged.insert(k.clone(), serde_json::Value::String(v.clone()));
    }

    // Query params
    if let Some(qs) = query && !qs.is_empty() {
        let params: HashMap<String, String> = serde_urlencoded::from_str(qs)
            .map_err(|e| ORPCError::bad_request(format!("Invalid query string: {e}")))?;
        for (k, v) in params {
            merged.insert(k, serde_json::Value::String(v));
        }
    }

    // Body (only for methods with body)
    let has_body = matches!(method, HttpMethod::Post | HttpMethod::Put | HttpMethod::Patch);
    if has_body && !body.is_empty() {
        let body_value: serde_json::Value = serde_json::from_slice(body)
            .map_err(|e| ORPCError::bad_request(format!("Invalid request body: {e}")))?;
        if let Some(obj) = body_value.as_object() {
            for (k, v) in obj {
                merged.insert(k.clone(), v.clone());
            }
        } else {
            // Non-object body: use as-is if no other params
            if merged.is_empty() {
                return Ok(DynInput::from_value(body_value));
            }
        }
    }

    if merged.is_empty() {
        Ok(DynInput::from_value(serde_json::Value::Null))
    } else {
        Ok(DynInput::from_value(serde_json::Value::Object(merged)))
    }
}

/// Encode a successful OpenAPI response as plain JSON (no RPC envelope).
pub fn encode_openapi_success(output: DynOutput) -> Result<(StatusCode, Vec<u8>), ProcedureError> {
    let value = output.to_value()?;
    let body = serde_json::to_vec(&value)
        .map_err(|e| ProcedureError::Serialize(SerializeError::from(e)))?;
    Ok((StatusCode::OK, body))
}

/// Encode an OpenAPI error response as plain JSON ORPCError.
pub fn encode_openapi_error(err: &ORPCError) -> (StatusCode, Vec<u8>) {
    let status = StatusCode::from_u16(err.status)
        .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let body = serde_json::to_vec(err).unwrap_or_default();
    (status, body)
}

/// Execute a procedure via OpenAPI protocol (single-value only).
pub async fn execute_openapi<TCtx>(
    procedure: &ErasedProcedure<TCtx>,
    ctx: TCtx,
    input: DynInput,
) -> (StatusCode, Vec<u8>) {
    let mut stream = procedure.exec(ctx, input);
    match stream.next().await {
        Some(Ok(output)) => match encode_openapi_success(output) {
            Ok(result) => result,
            Err(err) => {
                let orpc_err = procedure_error_to_orpc_error(err);
                encode_openapi_error(&orpc_err)
            }
        },
        Some(Err(err)) => {
            let orpc_err = procedure_error_to_orpc_error(err);
            encode_openapi_error(&orpc_err)
        }
        None => {
            let err = ORPCError::internal_server_error("Procedure returned no output");
            encode_openapi_error(&err)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use orpc_procedure::{Meta, ProcedureStream, Route};

    #[test]
    fn compile_simple_path() {
        let segs = compile_path_pattern("/users");
        assert_eq!(segs, vec![PathSegment::Literal("users".into())]);
    }

    #[test]
    fn compile_path_with_params() {
        let segs = compile_path_pattern("/users/{id}");
        assert_eq!(segs, vec![
            PathSegment::Literal("users".into()),
            PathSegment::Param("id".into()),
        ]);
    }

    #[test]
    fn compile_nested_path() {
        let segs = compile_path_pattern("/users/{id}/posts/{post_id}");
        assert_eq!(segs, vec![
            PathSegment::Literal("users".into()),
            PathSegment::Param("id".into()),
            PathSegment::Literal("posts".into()),
            PathSegment::Param("post_id".into()),
        ]);
    }

    #[test]
    fn match_literal_path() {
        let segs = compile_path_pattern("/users");
        let result = match_path(&segs, "/users");
        assert!(result.is_some());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn match_path_with_param() {
        let segs = compile_path_pattern("/users/{id}");
        let result = match_path(&segs, "/users/123").unwrap();
        assert_eq!(result.get("id").unwrap(), "123");
    }

    #[test]
    fn match_nested_params() {
        let segs = compile_path_pattern("/users/{uid}/posts/{pid}");
        let result = match_path(&segs, "/users/alice/posts/42").unwrap();
        assert_eq!(result.get("uid").unwrap(), "alice");
        assert_eq!(result.get("pid").unwrap(), "42");
    }

    #[test]
    fn match_wrong_literal() {
        let segs = compile_path_pattern("/users/{id}");
        assert!(match_path(&segs, "/posts/123").is_none());
    }

    #[test]
    fn match_wrong_segment_count() {
        let segs = compile_path_pattern("/users/{id}");
        assert!(match_path(&segs, "/users/123/extra").is_none());
        assert!(match_path(&segs, "/users").is_none());
    }

    #[test]
    fn http_method_conversion() {
        assert_eq!(http_method_to_orpc(&http::Method::GET), Some(HttpMethod::Get));
        assert_eq!(http_method_to_orpc(&http::Method::POST), Some(HttpMethod::Post));
        assert_eq!(http_method_to_orpc(&http::Method::DELETE), Some(HttpMethod::Delete));
        assert!(http_method_to_orpc(&http::Method::CONNECT).is_none());
    }

    #[test]
    fn route_index_build_and_match() {
        let get_user = ErasedProcedure::new(
            |_ctx: (), _input: DynInput| ProcedureStream::from_future(async { Ok(DynOutput::new("ok")) }),
            Route::get("/users/{id}"),
            Meta::default(),
        );
        let list_users = ErasedProcedure::new(
            |_ctx: (), _input: DynInput| ProcedureStream::from_future(async { Ok(DynOutput::new("ok")) }),
            Route::get("/users"),
            Meta::default(),
        );
        let create_user = ErasedProcedure::new(
            |_ctx: (), _input: DynInput| ProcedureStream::from_future(async { Ok(DynOutput::new("ok")) }),
            Route::post("/users"),
            Meta::default(),
        );
        // Procedure without route metadata (should not be indexed)
        let ping = ErasedProcedure::new(
            |_ctx: (), _input: DynInput| ProcedureStream::from_future(async { Ok(DynOutput::new("pong")) }),
            Route::default(),
            Meta::default(),
        );

        let router = orpc::Router::new()
            .procedure("getUser", get_user)
            .procedure("listUsers", list_users)
            .procedure("createUser", create_user)
            .procedure("ping", ping);

        let index = RouteIndex::build(&router);

        let m = index.match_route(HttpMethod::Get, "/users/123").unwrap();
        assert_eq!(m.procedure_key, "getUser");
        assert_eq!(m.path_params.get("id").unwrap(), "123");

        let m = index.match_route(HttpMethod::Get, "/users").unwrap();
        assert_eq!(m.procedure_key, "listUsers");

        let m = index.match_route(HttpMethod::Post, "/users").unwrap();
        assert_eq!(m.procedure_key, "createUser");

        assert!(index.match_route(HttpMethod::Delete, "/users").is_none());
        assert!(index.match_route(HttpMethod::Get, "/ping").is_none());
    }

    #[test]
    fn decode_get_with_query() {
        let params = HashMap::from([("id".into(), "42".into())]);
        let input = decode_openapi_request(&params, Some("limit=10"), b"", HttpMethod::Get).unwrap();
        let val = input.as_value().unwrap();
        assert_eq!(val["id"], "42");
        assert_eq!(val["limit"], "10");
    }

    #[test]
    fn decode_post_with_body_and_params() {
        let params = HashMap::from([("id".into(), "1".into())]);
        let body = br#"{"name": "Alice"}"#;
        let input = decode_openapi_request(&params, None, body, HttpMethod::Post).unwrap();
        let val = input.as_value().unwrap();
        assert_eq!(val["id"], "1");
        assert_eq!(val["name"], "Alice");
    }

    #[test]
    fn decode_no_input() {
        let input = decode_openapi_request(&HashMap::new(), None, b"", HttpMethod::Get).unwrap();
        assert_eq!(input.as_value().unwrap(), &serde_json::Value::Null);
    }

    #[test]
    fn encode_openapi_success_plain_json() {
        let output = DynOutput::new(serde_json::json!({"name": "Earth"}));
        let (status, body) = encode_openapi_success(output).unwrap();
        assert_eq!(status, StatusCode::OK);
        let val: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(val["name"], "Earth");
        // No "json" envelope wrapper
        assert!(val.get("json").is_none());
    }

    #[test]
    fn encode_openapi_error_plain_json() {
        let err = ORPCError::not_found("nope");
        let (status, body) = encode_openapi_error(&err);
        assert_eq!(status, StatusCode::NOT_FOUND);
        let val: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(val["code"], "NOT_FOUND");
        // No "json" envelope wrapper
        assert!(val.get("json").is_none());
    }

    #[tokio::test]
    async fn execute_openapi_success() {
        let proc = ErasedProcedure::new(
            |_ctx: (), input: DynInput| {
                ProcedureStream::from_future(async move {
                    let name: String = input.deserialize()?;
                    Ok(DynOutput::new(format!("hello {name}")))
                })
            },
            Route::get("/greet/{name}"),
            Meta::default(),
        );

        let input = DynInput::from_value(serde_json::json!("world"));
        let (status, body) = execute_openapi(&proc, (), input).await;
        assert_eq!(status, StatusCode::OK);
        let val: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(val, "hello world");
    }
}
