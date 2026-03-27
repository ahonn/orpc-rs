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

## Architecture

### Crate Dependency Graph

```
@orpc-rs/tauri (npm, TypeScript client)
       ↓
tauri-plugin-orpc ──→ orpc-server ──→ orpc-procedure
orpc-axum ───────────→ orpc-server       ↑
orpc-specta ─────────────────────────→ orpc
                                         ↓
                                    orpc-procedure
```

### Crate Responsibilities

| Crate | Purpose |
|-------|---------|
| `orpc-procedure` | Type-erased execution engine: `ErasedProcedure`, `ProcedureStream`, `DynInput`/`DynOutput`, `ProcedureError` |
| `orpc` | Type-safe builder API, middleware composition, `Router`, `router!` macro, `ORPCError`/`ErrorCode` |
| `orpc-server` | Wire protocol: RPC envelope encode/decode, SSE streaming, OpenAPI routing |
| `orpc-axum` | Axum integration: HTTP → oRPC router with RPC + OpenAPI endpoints |
| `orpc-specta` | TypeScript type generation from Rust procedures via specta |
| `tauri-plugin-orpc` | Tauri IPC plugin: serves oRPC routers over Tauri's IPC channel |
| `@orpc-rs/tauri` | TypeScript `TauriLink` for `@orpc/client` (published to npm) |

### Key Design Patterns

- **Type erasure at boundary**: Procedures are fully generic during building (`Builder<TBaseCtx, TCtx, TError>`), then erased to `ErasedProcedure<TCtx>` when added to `Router<TCtx>`.
- **Compile-time middleware composition**: Middleware layers chained via `MiddlewareChain` trait (`IdentityChain` + `ComposedChain`), not stored in a Vec.
- **Unified streaming**: Both queries and subscriptions use `ProcedureStream`, unifying single-value (`FutureStream`) and multi-value responses.
- **Wire compatibility**: RPC envelope format (`{"json": <data>, "meta": [...]}`) is identical to `@orpc/client`.
- **Dual transport**: Same `Router<TCtx>` serves both HTTP (axum) and IPC (tauri) without changes.
- **Panic safety**: Handlers are wrapped in `catch_unwind` to protect the server.

### Type System Flow

1. **Compile-time**: `Builder` → type-safe `.use_middleware()` → `.input()` → `.output()` → `.handler()`
2. **Erasure**: `Procedure<I, O, E>` → `ErasedProcedure<TCtx>` (input/output serialized to JSON)
3. **Runtime**: `Router<TCtx>` dispatches via `HashMap<String, ErasedProcedure<TCtx>>`
4. **Codegen**: `orpc-specta` walks the router and emits TypeScript types compatible with `@orpc/client`

## Workspace Layout

- `crates/` — All Rust library crates
- `packages/tauri/` — TypeScript npm package (`@orpc-rs/tauri`)
- `examples/axum-react/` — Web example (Rust axum server + React client)
- `examples/tauri-app/` — Desktop example (Tauri + React)

## Release

Managed by `release-plz` (config in `release-plz.toml`). Automated version bumping, changelog generation, and crates.io/npm publishing via GitHub Actions.
