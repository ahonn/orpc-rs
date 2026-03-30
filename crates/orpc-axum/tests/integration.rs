use axum::body::Body;

use http::Request;
use http_body_util::BodyExt;
use orpc::*;
use orpc_axum::ORPCConfig;
use serde::{Deserialize, Serialize};
use tower::ServiceExt;

// --- Test types ---

#[derive(Clone)]
struct AppCtx {
    user_agent: String,
}

struct AuthCtx {
    user: String,
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
struct Planet {
    name: String,
    radius: u32,
}

// --- Handlers ---

async fn ping(_ctx: AppCtx, _input: ()) -> Result<String, ORPCError> {
    Ok("pong".into())
}

async fn find_planet(_ctx: AppCtx, input: String) -> Result<Planet, ORPCError> {
    match input.as_str() {
        "Earth" => Ok(Planet {
            name: "Earth".into(),
            radius: 6371,
        }),
        _ => Err(ORPCError::not_found(format!("Planet '{input}' not found"))),
    }
}

async fn greet(ctx: AuthCtx, input: String) -> Result<String, ORPCError> {
    Ok(format!("Hello {input}, from {}", ctx.user))
}

// --- Helpers ---

fn build_test_router() -> Router<AppCtx> {
    router! {
        "ping" => os::<AppCtx>().handler(ping),
        "planet" => {
            "find" => os::<AppCtx>().input(Identity::<String>::new()).handler(find_planet),
        },
    }
}

fn ctx_from_parts(parts: &http::request::Parts) -> AppCtx {
    let user_agent = parts
        .headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string();
    AppCtx { user_agent }
}

/// Build a POST request matching what @orpc/client actually sends.
/// - No input: `{}`
/// - With input: `{"json": <value>}`
fn rpc_request(path: &str, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap()
}

async fn response_json(resp: axum::response::Response) -> (u16, serde_json::Value) {
    let status = resp.status().as_u16();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    (status, json)
}

// --- Tests ---

#[tokio::test]
async fn happy_path_ping() {
    let app = orpc_axum::into_router(build_test_router(), ctx_from_parts);

    // @orpc/client sends {} for no-input procedures
    let req = rpc_request("/ping", serde_json::json!({}));
    let resp = app.oneshot(req).await.unwrap();

    let (status, json) = response_json(resp).await;
    assert_eq!(status, 200);
    assert_eq!(json["json"], "pong");
}

#[tokio::test]
async fn nested_path_planet_find() {
    let app = orpc_axum::into_router(build_test_router(), ctx_from_parts);

    // @orpc/client sends {"json": <input>} without meta for plain types
    let req = rpc_request("/planet/find", serde_json::json!({"json": "Earth"}));
    let resp = app.oneshot(req).await.unwrap();

    let (status, json) = response_json(resp).await;
    assert_eq!(status, 200);
    let planet: Planet = serde_json::from_value(json["json"].clone()).unwrap();
    assert_eq!(
        planet,
        Planet {
            name: "Earth".into(),
            radius: 6371,
        }
    );
}

#[tokio::test]
async fn procedure_not_found() {
    let app = orpc_axum::into_router(build_test_router(), ctx_from_parts);

    let req = rpc_request("/unknown", serde_json::json!({}));
    let resp = app.oneshot(req).await.unwrap();

    let (status, json) = response_json(resp).await;
    assert_eq!(status, 404);
    assert_eq!(json["json"]["code"], "NOT_FOUND");
}

#[tokio::test]
async fn handler_returns_orpc_error() {
    let app = orpc_axum::into_router(build_test_router(), ctx_from_parts);

    let req = rpc_request("/planet/find", serde_json::json!({"json": "Vulcan"}));
    let resp = app.oneshot(req).await.unwrap();

    let (status, json) = response_json(resp).await;
    assert_eq!(status, 404);
    assert_eq!(json["json"]["code"], "NOT_FOUND");
    assert_eq!(json["json"]["status"], 404);
    assert!(json["json"]["message"].as_str().unwrap().contains("Vulcan"));
}

#[tokio::test]
async fn invalid_json_body() {
    let app = orpc_axum::into_router(build_test_router(), ctx_from_parts);

    let req = Request::builder()
        .method("POST")
        .uri("/ping")
        .header("content-type", "application/json")
        .body(Body::from("not json"))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();

    let (status, json) = response_json(resp).await;
    assert_eq!(status, 400);
    assert_eq!(json["json"]["code"], "BAD_REQUEST");
}

#[tokio::test]
async fn wrong_http_method() {
    let app = orpc_axum::into_router(build_test_router(), ctx_from_parts);

    let req = Request::builder()
        .method("PUT")
        .uri("/ping")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();

    let (status, json) = response_json(resp).await;
    assert_eq!(status, 405);
    assert_eq!(json["json"]["code"], "METHOD_NOT_ALLOWED");
}

#[tokio::test]
async fn rpc_get_with_data_query_param() {
    let app = orpc_axum::into_router(build_test_router(), ctx_from_parts);

    // GET /planet/find?data={"json":"Earth"} — matches @orpc/client GET mode
    let req = Request::builder()
        .method("GET")
        .uri("/planet/find?data=%7B%22json%22%3A%22Earth%22%7D")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();

    let (status, json) = response_json(resp).await;
    assert_eq!(status, 200);
    let planet: Planet = serde_json::from_value(json["json"].clone()).unwrap();
    assert_eq!(planet.name, "Earth");
}

#[tokio::test]
async fn rpc_get_without_data_param() {
    let app = orpc_axum::into_router(build_test_router(), ctx_from_parts);

    // GET /ping with no ?data= param → null input
    let req = Request::builder()
        .method("GET")
        .uri("/ping")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();

    let (status, json) = response_json(resp).await;
    assert_eq!(status, 200);
    assert_eq!(json["json"], "pong");
}

#[tokio::test]
async fn context_extraction_from_headers() {
    let router: Router<AppCtx> = router! {
        "echo_ua" => os::<AppCtx>().handler(
            |ctx: AppCtx, _input: ()| async move {
                Ok::<_, ORPCError>(ctx.user_agent)
            }
        ),
    };

    let app = orpc_axum::into_router(router, ctx_from_parts);

    let req = Request::builder()
        .method("POST")
        .uri("/echo_ua")
        .header("content-type", "application/json")
        .header("user-agent", "test-client/1.0")
        .body(Body::from(b"{}".to_vec()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();

    let (status, json) = response_json(resp).await;
    assert_eq!(status, 200);
    assert_eq!(json["json"], "test-client/1.0");
}

#[tokio::test]
async fn with_middleware_chain() {
    let auth_mw = middleware_fn(|ctx: AppCtx, mw: MiddlewareCtx<AuthCtx>| async move {
        mw.next(AuthCtx {
            user: format!("agent:{}", ctx.user_agent),
        })
        .await
    });

    let router: Router<AppCtx> = router! {
        "greet" => os::<AppCtx>()
            .use_middleware(auth_mw)
            .input(Identity::<String>::new())
            .handler(greet),
    };

    let app = orpc_axum::into_router(router, ctx_from_parts);

    let req = Request::builder()
        .method("POST")
        .uri("/greet")
        .header("content-type", "application/json")
        .header("user-agent", "curl")
        .body(Body::from(
            serde_json::to_vec(&serde_json::json!({"json": "World"})).unwrap(),
        ))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();

    let (status, json) = response_json(resp).await;
    assert_eq!(status, 200);
    assert_eq!(json["json"], "Hello World, from agent:curl");
}

#[tokio::test]
async fn custom_prefix() {
    let config = ORPCConfig {
        prefix: "/api/rpc".to_string(),
        ..Default::default()
    };

    let app = orpc_axum::into_router_with_config(build_test_router(), ctx_from_parts, config);

    let req = rpc_request("/api/rpc/ping", serde_json::json!({}));
    let resp = app.oneshot(req).await.unwrap();

    let (status, json) = response_json(resp).await;
    assert_eq!(status, 200);
    assert_eq!(json["json"], "pong");
}

#[tokio::test]
async fn response_content_type_is_json() {
    let app = orpc_axum::into_router(build_test_router(), ctx_from_parts);

    let req = rpc_request("/ping", serde_json::json!({}));
    let resp = app.oneshot(req).await.unwrap();

    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "application/json"
    );
}

// --- SSE Subscription Tests ---

#[tokio::test]
async fn sse_subscription_stream() {
    // For SSE, we need a handler that returns a stream.
    // The current builder's .handler() wraps in from_future (single-value).
    // To test SSE, we need a raw ErasedProcedure with from_stream.
    use orpc_procedure::*;
    let counter_proc = ErasedProcedure::new(
        |_ctx: AppCtx, _input: DynInput| {
            let items = vec![
                Ok(DynOutput::new(1u32)),
                Ok(DynOutput::new(2u32)),
                Ok(DynOutput::new(3u32)),
            ];
            ProcedureStream::from_stream(futures_util::stream::iter(items))
        },
        Route::default(),
        Meta::default(),
    );

    let router: Router<AppCtx> = Router::new().procedure("counter", counter_proc);
    let app = orpc_axum::into_router(router, ctx_from_parts);

    let req = rpc_request("/counter", serde_json::json!({}));
    let resp = app.oneshot(req).await.unwrap();

    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "text/event-stream"
    );

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8(body.to_vec()).unwrap();

    assert!(text.contains("event: message\n"));
    assert!(text.contains("id: 0\n"));
    assert!(text.contains("id: 1\n"));
    assert!(text.contains("id: 2\n"));
    assert!(text.contains("event: done\n"));
}

#[tokio::test]
async fn sse_subscription_error_mid_stream() {
    use orpc_procedure::*;

    let proc = ErasedProcedure::new(
        |_ctx: AppCtx, _input: DynInput| {
            let items: Vec<Result<DynOutput, ProcedureError>> = vec![
                Ok(DynOutput::new("ok")),
                Err(ProcedureError::from(ORPCError::internal_server_error(
                    "boom",
                ))),
            ];
            ProcedureStream::from_stream(futures_util::stream::iter(items))
        },
        Route::default(),
        Meta::default(),
    );

    let router: Router<AppCtx> = Router::new().procedure("fail", proc);
    let app = orpc_axum::into_router(router, ctx_from_parts);

    let req = rpc_request("/fail", serde_json::json!({}));
    let resp = app.oneshot(req).await.unwrap();

    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "text/event-stream"
    );

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8(body.to_vec()).unwrap();

    assert!(text.contains("event: message\n"));
    assert!(text.contains("event: error\n"));
    assert!(text.contains("INTERNAL_SERVER_ERROR"));
    // No done event after error
    assert!(!text.contains("event: done\n"));
}

#[tokio::test]
async fn single_value_still_returns_json() {
    // Ensure from_future procedures still return JSON, not SSE
    let app = orpc_axum::into_router(build_test_router(), ctx_from_parts);

    let req = rpc_request("/ping", serde_json::json!({}));
    let resp = app.oneshot(req).await.unwrap();

    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "application/json"
    );
    let (status, json) = response_json(resp).await;
    assert_eq!(status, 200);
    assert_eq!(json["json"], "pong");
}

// --- OpenAPI Tests ---

fn build_openapi_router() -> Router<AppCtx> {
    use orpc_procedure::*;

    let get_user = ErasedProcedure::new(
        |_ctx: AppCtx, input: DynInput| {
            ProcedureStream::from_future(async move {
                #[derive(serde::Deserialize)]
                struct Input {
                    id: String,
                }
                let inp: Input = input.deserialize()?;
                Ok(DynOutput::new(
                    serde_json::json!({"id": inp.id, "name": "Alice"}),
                ))
            })
        },
        Route::get("/users/{id}"),
        Meta::default(),
    );

    let create_user = ErasedProcedure::new(
        |_ctx: AppCtx, input: DynInput| {
            ProcedureStream::from_future(async move {
                #[derive(serde::Deserialize)]
                struct Input {
                    name: String,
                }
                let inp: Input = input.deserialize()?;
                Ok(DynOutput::new(
                    serde_json::json!({"id": "new", "name": inp.name}),
                ))
            })
        },
        Route::post("/users"),
        Meta::default(),
    );

    let list_users = ErasedProcedure::new(
        |_ctx: AppCtx, input: DynInput| {
            ProcedureStream::from_future(async move {
                #[derive(serde::Deserialize)]
                struct Input {
                    limit: Option<String>,
                }
                let inp: Input = input.deserialize()?;
                let limit = inp.limit.unwrap_or("10".into());
                Ok(DynOutput::new(
                    serde_json::json!({"users": [], "limit": limit}),
                ))
            })
        },
        Route::get("/users"),
        Meta::default(),
    );

    Router::new()
        .procedure("getUser", get_user)
        .procedure("createUser", create_user)
        .procedure("listUsers", list_users)
}

#[tokio::test]
async fn openapi_get_with_path_params() {
    let config = orpc_axum::OpenAPIConfig::default();
    let app = orpc_axum::into_openapi_router(build_openapi_router(), ctx_from_parts, config);

    let req = Request::builder()
        .method("GET")
        .uri("/users/42")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();

    let (status, json) = response_json(resp).await;
    assert_eq!(status, 200);
    assert_eq!(json["id"], "42");
    assert_eq!(json["name"], "Alice");
}

#[tokio::test]
async fn openapi_post_with_body() {
    let config = orpc_axum::OpenAPIConfig::default();
    let app = orpc_axum::into_openapi_router(build_openapi_router(), ctx_from_parts, config);

    let req = Request::builder()
        .method("POST")
        .uri("/users")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"name":"Bob"}"#))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();

    let (status, json) = response_json(resp).await;
    assert_eq!(status, 200);
    assert_eq!(json["name"], "Bob");
}

#[tokio::test]
async fn openapi_get_with_query_params() {
    let config = orpc_axum::OpenAPIConfig::default();
    let app = orpc_axum::into_openapi_router(build_openapi_router(), ctx_from_parts, config);

    let req = Request::builder()
        .method("GET")
        .uri("/users?limit=5")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();

    let (status, json) = response_json(resp).await;
    assert_eq!(status, 200);
    assert_eq!(json["limit"], "5");
}

#[tokio::test]
async fn openapi_no_matching_route() {
    let config = orpc_axum::OpenAPIConfig::default();
    let app = orpc_axum::into_openapi_router(build_openapi_router(), ctx_from_parts, config);

    let req = Request::builder()
        .method("DELETE")
        .uri("/users/42")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();

    let (status, json) = response_json(resp).await;
    assert_eq!(status, 404);
    assert_eq!(json["code"], "NOT_FOUND");
}

#[tokio::test]
async fn openapi_with_prefix() {
    let config = orpc_axum::OpenAPIConfig {
        prefix: "/api".into(),
        ..Default::default()
    };
    let app = orpc_axum::into_openapi_router(build_openapi_router(), ctx_from_parts, config);

    let req = Request::builder()
        .method("GET")
        .uri("/api/users/7")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();

    let (status, json) = response_json(resp).await;
    assert_eq!(status, 200);
    assert_eq!(json["id"], "7");
}

// --- Multipart file upload tests ---

#[derive(Debug, Deserialize, Serialize)]
struct UploadInput {
    title: String,
    file: ORPCFile,
}

async fn upload_handler(_ctx: AppCtx, input: UploadInput) -> Result<String, ORPCError> {
    Ok(format!(
        "{}:{}:{}",
        input.title,
        input.file.name.unwrap_or_default(),
        input.file.data.len()
    ))
}

fn build_upload_router() -> Router<AppCtx> {
    router! {
        "upload" => os::<AppCtx>().input(Identity::<UploadInput>::new()).handler(upload_handler),
    }
}

/// Build a raw multipart/form-data request body.
fn build_multipart_body(
    data_json: &str,
    files: &[(&str, &str, &[u8])], // (name, content_type, data)
) -> (String, Vec<u8>) {
    let boundary = "----orpc-test-boundary";
    let mut body = Vec::new();

    // "data" field
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(b"Content-Disposition: form-data; name=\"data\"\r\n");
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(data_json.as_bytes());
    body.extend_from_slice(b"\r\n");

    // File fields
    for (i, (name, content_type, data)) in files.iter().enumerate() {
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(
            format!("Content-Disposition: form-data; name=\"{i}\"; filename=\"{name}\"\r\n")
                .as_bytes(),
        );
        body.extend_from_slice(format!("Content-Type: {content_type}\r\n").as_bytes());
        body.extend_from_slice(b"\r\n");
        body.extend_from_slice(data);
        body.extend_from_slice(b"\r\n");
    }

    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    let content_type = format!("multipart/form-data; boundary={boundary}");
    (content_type, body)
}

#[tokio::test]
async fn multipart_file_upload_rpc() {
    let router = build_upload_router();
    let app = orpc_axum::into_router(router, ctx_from_parts);

    let data_json = r#"{"json":{"title":"My Doc","file":{}},"meta":[],"maps":[["file"]]}"#;
    let (content_type, body) =
        build_multipart_body(data_json, &[("readme.txt", "text/plain", b"hello world")]);

    let req = Request::builder()
        .method("POST")
        .uri("/upload")
        .header("content-type", content_type)
        .body(Body::from(body))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    let (status, json) = response_json(resp).await;
    assert_eq!(status, 200);
    assert_eq!(json["json"], "My Doc:readme.txt:11");
}

#[tokio::test]
async fn multipart_missing_data_field() {
    let router = build_upload_router();
    let app = orpc_axum::into_router(router, ctx_from_parts);

    let boundary = "----test";
    let body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"0\"; filename=\"f.txt\"\r\n\r\ndata\r\n--{boundary}--\r\n"
    );

    let req = Request::builder()
        .method("POST")
        .uri("/upload")
        .header(
            "content-type",
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from(body))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    let (status, _) = response_json(resp).await;
    assert_eq!(status, 400);
}

#[tokio::test]
async fn json_request_still_works_with_upload_router() {
    let router = build_upload_router();
    let app = orpc_axum::into_router(router, ctx_from_parts);

    // A regular JSON request should still work (no multipart)
    let req = rpc_request("/upload", serde_json::json!({}));
    let resp = app.oneshot(req).await.unwrap();
    // Will fail deserialization since UploadInput expects title+file, but should be 400/500, not panic
    let status = resp.status().as_u16();
    assert!(status >= 400);
}
