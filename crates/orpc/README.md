# orpc

Type-safe API layer for building oRPC routers in Rust.

## Overview

The main user-facing crate. Provides a builder API for defining procedures with compile-time type safety, middleware composition, and router organization.

## Key Concepts

- **`os::<Ctx>()`** — Entry point for building procedures with the typestate pattern
- **`Router<TCtx>`** — Collection of type-erased procedures, keyed by dot-separated names
- **`router!` macro** — Declarative router definition with nested blocks
- **Middleware** — Compile-time composed context transformations via `use_middleware()`
- **`ORPCError`** — Typed error with HTTP status codes matching the oRPC protocol

## Example

```rust
use orpc::*;

async fn ping(_ctx: AppCtx, _input: ()) -> Result<String, ORPCError> {
    Ok("pong".into())
}

async fn find_planet(ctx: AppCtx, input: FindInput) -> Result<Planet, ORPCError> {
    ctx.db.find(&input.name)
        .ok_or_else(|| ORPCError::not_found("Planet not found"))
}

let router = router! {
    "ping" => os::<AppCtx>().handler(ping),
    "planet" => {
        "find" => os::<AppCtx>()
            .input(Identity::<FindInput>::new())
            .handler(find_planet),
    },
};
```
