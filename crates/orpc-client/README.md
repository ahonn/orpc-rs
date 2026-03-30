# orpc-client

HTTP client for orpc-rs RPC servers.

## Overview

Provides a typed Rust client for calling oRPC procedures over HTTP, wire-compatible with `@orpc/client`. Supports both single-value calls (queries/mutations) and SSE streaming subscriptions.

## Key Concepts

- **`Client<L>`** — Main client struct, generic over a `Link` transport
- **`Link` trait** — Transport abstraction (implement for custom protocols like IPC)
- **`RpcLink`** — Default HTTP transport using reqwest, matching `@orpc/client` wire format
- **`ClientError`** — Unified error type covering transport, serialization, and RPC errors

## Example

```rust
use orpc_client::Client;

// Create a client
let client = Client::new("http://localhost:3000/rpc");

// Query / Mutation
let planet: Planet = client.call("planet.find", &FindInput { name: "Earth".into() }).await?;

// Subscription (SSE stream)
use futures_util::StreamExt;
let mut stream = client.subscribe::<Planet>("planet.stream", &()).await?;
while let Some(result) = stream.next().await {
    let planet = result?;
    println!("{planet:?}");
}
```

## Generated Client

When used with the `#[orpc_service]` macro, a typed client struct is auto-generated:

```rust
// Generated from #[orpc_service(context = AppCtx)] trait PlanetApi { ... }
let client = PlanetApiClient::new("http://localhost:3000/rpc");
let planet = client.find_planet(&FindInput { name: "Earth".into() }).await?;
```
