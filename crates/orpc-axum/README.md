# orpc-axum

Axum integration for oRPC routers.

## Features

- **RPC handler** — Serves all procedures under a single path prefix, supports both GET (`?data=`) and POST
- **OpenAPI handler** — REST-style routing based on `Route` metadata (`GET /users/{id}`, `POST /users`)
- **SSE streaming** — Auto-detected subscriptions served as `text/event-stream` with 5-second keep-alive
- **Last-Event-ID** — SSE reconnection support via the `Last-Event-ID` header

## Example

```rust
use orpc_axum::{into_router, into_openapi_router, ORPCConfig, OpenAPIConfig};

// RPC endpoint (used by @orpc/client RPCLink)
let rpc = into_router(router.clone(), |parts| AppCtx::from(parts));

// OpenAPI endpoint (REST-style, used by OpenAPILink or plain fetch)
let openapi = into_openapi_router(
    router,
    |parts| AppCtx::from(parts),
    OpenAPIConfig { prefix: "/api".into(), ..Default::default() },
);

let app = axum::Router::new()
    .nest("/rpc", rpc)
    .nest("/api", openapi);
```
