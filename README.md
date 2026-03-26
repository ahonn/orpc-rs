# orpc-rs

Rust implementation of [oRPC](https://orpc.dev) — type-safe RPC with first-class Tauri support.

Build fully type-safe APIs in Rust with auto-generated TypeScript types, compatible with the official `@orpc/client` and `@orpc/tanstack-query` ecosystem.

## Features

- **Type-safe procedures** — Builder API with compile-time middleware composition
- **Wire-compatible** — Matches `@orpc/client` RPC protocol (request/response, SSE subscriptions)
- **TypeScript generation** — Auto-generate `Client<>` types via [specta](https://github.com/oscartbeaumont/specta), works directly with `@orpc/client`
- **Tauri IPC** — Zero-HTTP transport for desktop apps, with `Channel`-based subscriptions
- **Axum integration** — RPC + OpenAPI endpoints with SSE keep-alive
- **TanStack Query** — `useQuery` / `useMutation` via `@orpc/tanstack-query` out of the box

## Architecture

```
                        ┌─────────────────────────────────────────────────┐
                        │                   TypeScript                    │
                        │                                                 │
                        │  bindings.ts ──▶ @orpc/client ──▶ @orpc/tanstack-query
                        │       ▲              ▲                          │
                        │       │         TauriLink / RPCLink             │
                        └───────┼──────────────┼──────────────────────────┘
                                │              │
  ┌─────────────────────────────┼──────────────┼──────────┐
  │                Rust         │              │          │
  │                             │              │          │
  │  orpc-specta ───────────────┘              │          │
  │       ▲                                    │          │
  │       │                              ┌─────┴─────┐   │
  │    orpc (builder + router)           │ orpc-axum  │   │
  │       │                              │ orpc-tauri │   │
  │       ▼                              └─────┬─────┘   │
  │  orpc-procedure (type-erased engine)       │         │
  │       │                                    │         │
  │       └──────────── orpc-server ───────────┘         │
  │                  (wire protocol)                      │
  └───────────────────────────────────────────────────────┘
```

## Crates

| Crate | Description |
|-------|-------------|
| [`orpc-procedure`](crates/orpc-procedure) | Type-erased execution engine |
| [`orpc`](crates/orpc) | Type-safe builder API, router, middleware |
| [`orpc-server`](crates/orpc-server) | Wire protocol (RPC, SSE, OpenAPI) |
| [`orpc-axum`](crates/orpc-axum) | Axum HTTP integration |
| [`orpc-specta`](crates/orpc-specta) | TypeScript type generation |
| [`tauri-plugin-orpc`](crates/orpc-tauri) | Tauri v2 IPC plugin |

## Packages

| Package | Description |
|---------|-------------|
| [`@orpc-rs/tauri`](packages/tauri) | TauriLink for `@orpc/client` |

## Quick Start

### Rust — Define procedures

```rust
use orpc::*;
use orpc_specta::{specta, Type};

#[derive(Serialize, Deserialize, Type)]
struct Planet { id: u32, name: String }

async fn list_planets(ctx: AppCtx, _: ()) -> Result<Vec<Planet>, ORPCError> {
    Ok(ctx.db.list())
}

let router = router! {
    "planet" => {
        "list" => os::<AppCtx>()
            .output(specta::<Vec<Planet>>())
            .handler(list_planets),
    },
};

// Auto-generate TypeScript types
orpc_specta::export_ts(&router, "../src/bindings.ts")?;
```

### TypeScript — Consume with full type safety

```typescript
import { createORPCClient } from "@orpc/client"
import { createTanstackQueryUtils } from "@orpc/tanstack-query"
import { TauriLink } from "@orpc-rs/tauri"
import type { Procedures } from "./bindings"

const client = createORPCClient<Procedures>(TauriLink())
const orpc = createTanstackQueryUtils(client)

// Fully typed — input and output inferred from Rust
const { data } = useQuery(orpc.planet.list.queryOptions({}))
```

## Examples

| Example | Description |
|---------|-------------|
| [`axum-react`](examples/axum-react) | Web app — Axum server + React client with RPC, OpenAPI, and SSE |
| [`tauri-app`](examples/tauri-app) | Desktop app — Tauri IPC + TanStack Query, zero HTTP |

## License

MIT
