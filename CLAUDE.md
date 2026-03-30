# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

orpc-rs is a Rust implementation of [oRPC](https://orpc.dev) — a type-safe RPC framework with first-class Tauri support. It enables building fully type-safe APIs in Rust with auto-generated TypeScript types, wire-compatible with `@orpc/client` and `@orpc/tanstack-query`.

## Build & Development Commands

```bash
# Build
cargo build                          # Build all workspace crates
cargo build -p orpc                  # Build a specific crate

# Test
cargo test --workspace               # Run all tests
cargo test -p orpc                   # Test a specific crate
cargo test -p orpc -- test_name      # Run a single test

# Lint & Format
cargo fmt --all -- --check           # Check formatting
cargo fmt --all                      # Auto-format
cargo clippy --all-targets -- -D warnings  # Lint (CI treats warnings as errors)

# Docs
cargo doc --no-deps --workspace      # Build documentation
```

Note: `tauri-plugin-orpc` requires system dependencies (webkit2gtk, etc.) on Linux. On macOS, it builds without extra setup.

## CI

CI (`.github/workflows/test.yml`) runs on push to main/master and PRs:
1. `cargo fmt --all -- --check`
2. `cargo clippy --all-targets -- -D warnings`
3. `cargo test --workspace --verbose`
4. `cargo doc --no-deps --workspace`

Release via `release-plz` (`.github/workflows/release.yml`): all crates are version-locked in a single `version_group`, published together to crates.io, with `@orpc-rs/tauri` auto-published to npm.

## Architecture

### Crate Dependency Graph

```
@orpc-rs/tauri (npm, TypeScript client)
       |
tauri-plugin-orpc ──→ orpc-server ──→ orpc-procedure
orpc-axum ───────────→ orpc-server       ↑
orpc-client ─────────→ orpc             orpc
orpc-specta ─────────→ orpc              ↓
orpc-macros ←──────── orpc          orpc-procedure
```

### Crate Responsibilities

| Crate | Purpose |
|-------|---------|
| `orpc-procedure` | Type-erased execution engine: `ErasedProcedure`, `ProcedureStream`, `DynInput`/`DynOutput`, `ProcedureError` |
| `orpc-macros` | Proc-macro crate: `#[orpc_service]` attribute generates router function + typed client struct from a trait |
| `orpc` | Type-safe builder API, middleware composition, `Router`, `router!` macro, `ORPCError`/`ErrorCode`, `ORPCFile` (file uploads) |
| `orpc-client` | Rust HTTP client: `Client<L: Link>`, `RpcLink` (reqwest-based), SSE subscription support |
| `orpc-server` | Wire protocol: RPC envelope encode/decode, SSE streaming, OpenAPI routing with path params |
| `orpc-axum` | Axum integration: HTTP → oRPC router with RPC + OpenAPI endpoints, SSE keep-alive, multipart file upload |
| `orpc-specta` | TypeScript type generation from Rust procedures via specta |
| `tauri-plugin-orpc` | Tauri v2 IPC plugin: serves oRPC routers over Tauri's IPC channel (zero HTTP) |
| `@orpc-rs/tauri` | TypeScript `TauriLink` for `@orpc/client` (published to npm) |

### `#[orpc_service]` Proc-Macro

The `orpc-macros` crate provides `#[orpc_service(context = Type)]` which transforms a trait into three artifacts:

```rust
#[orpc_service(context = AppCtx)]
pub trait PlanetApi {
    async fn find(&self, ctx: AppCtx, input: FindInput) -> Result<Planet, ORPCError>;
}
```

Generates:
1. **Transformed trait** — `async fn` desugared to `fn() -> impl Future + Send`
2. **Router function** — `pub fn planet_api_router(api: T) -> Router<AppCtx>` wrapping each method as an erased procedure
3. **Client struct** — `pub struct PlanetApiClient<L: Link = RpcLink>` with typed async methods (`.find(&input)`)

Method constraints: must have `&self` first param, context second, optional input third, returns `Result<O, ORPCError>`.

### Key Design Patterns

- **Type erasure at boundary**: Procedures are fully generic during building (`Builder<TBaseCtx, TCtx, TError>`), then erased to `ErasedProcedure<TCtx>` when added to `Router<TCtx>`.
- **Compile-time middleware composition**: Middleware layers chained via `MiddlewareChain` trait (`IdentityChain` + `ComposedChain`), not stored in a Vec.
- **Unified streaming**: Both queries and subscriptions use `ProcedureStream`, unifying single-value (`FutureStream`) and multi-value responses.
- **Wire compatibility**: RPC envelope format (`{"json": <data>, "meta": [...]}`) is identical to `@orpc/client`.
- **Dual transport**: Same `Router<TCtx>` serves both HTTP (axum) and IPC (tauri) without changes.
- **Panic safety**: Handlers are wrapped in `catch_unwind` to protect the server.
- **Two entry points**: `os::<Ctx>()` for direct procedure building; `#[orpc_service]` macro for trait-based definition with auto-generated router + client.

### Type System Flow

1. **Compile-time**: `Builder` → type-safe `.use_middleware()` → `.input()` → `.output()` → `.handler()`
2. **Erasure**: `Procedure<I, O, E>` → `ErasedProcedure<TCtx>` (input/output serialized to JSON)
3. **Runtime**: `Router<TCtx>` dispatches via `HashMap<String, ErasedProcedure<TCtx>>`
4. **Codegen**: `orpc-specta` walks the router and emits TypeScript types compatible with `@orpc/client`

## Workspace Layout

- `crates/` — All Rust library crates (8 crates)
- `packages/tauri/` — TypeScript npm package (`@orpc-rs/tauri`)
- `examples/axum-react/` — Web example (Rust axum server + React client)
- `examples/tauri-app/` — Desktop example (Tauri + React)
- `docs/` — Design analysis and implementation plan documents

## Testing Patterns

- Tests exist in both `crates/*/tests/` (integration tests) and inline `#[cfg(test)] mod tests` blocks within source files (unit tests)
- All async tests use `#[tokio::test]`
- Axum tests use Tower `ServiceExt::oneshot()` to test handlers without HTTP server
- Common fixtures: `AppCtx`/`AuthCtx` context types, `Planet`/`FindInput` data types
- HTTP request builders match `@orpc/client` wire format for compatibility verification

## Release

Managed by `release-plz` (config in `release-plz.toml`). All crates share a single version group (`"orpc"`). The main changelog is on the `orpc` crate and includes changes from all 7 sub-crates. Example crates are excluded from publishing.
