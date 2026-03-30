# orpc-macros

Proc-macro for orpc-rs: generate typed server router and client from a trait definition.

## Overview

Provides the `#[orpc_service]` attribute macro that transforms a trait into three artifacts:

1. **Transformed trait** — `async fn` desugared to `fn() -> impl Future + Send`
2. **Router function** — `{trait_name}_router(api: T) -> Router<Ctx>` wrapping each method as an erased procedure
3. **Client struct** — `{TraitName}Client<L: Link>` with typed async methods for each procedure

## Example

```rust
use orpc::orpc_service;

#[orpc_service(context = AppCtx)]
pub trait PlanetApi {
    async fn ping(&self, ctx: AppCtx) -> Result<String, ORPCError>;
    async fn find_planet(&self, ctx: AppCtx, input: FindInput) -> Result<Planet, ORPCError>;
}

// Server: implement the trait and build a Router
struct MyApi;
impl PlanetApi for MyApi {
    async fn ping(&self, _ctx: AppCtx) -> Result<String, ORPCError> {
        Ok("pong".into())
    }
    async fn find_planet(&self, _ctx: AppCtx, input: FindInput) -> Result<Planet, ORPCError> {
        // ...
    }
}
let router = planet_api_router(MyApi);

// Client: typed RPC client
let client = PlanetApiClient::new("http://localhost:3000/rpc");
let planet = client.find_planet(&FindInput { name: "Earth".into() }).await?;
```

## Method Signatures

Each method must follow one of these patterns:

- **No input**: `async fn name(&self, ctx: Ctx) -> Result<Output, ORPCError>`
- **With input**: `async fn name(&self, ctx: Ctx, input: Input) -> Result<Output, ORPCError>`

The method name becomes the RPC procedure key (e.g., `find_planet` -> `"find_planet"`).
