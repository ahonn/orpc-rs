use std::future::Future;
use std::marker::PhantomData;
use std::sync::Arc;

use orpc_procedure::{DynInput, DynOutput, ProcedureError, Route};
use serde::Serialize;

use crate::handler::BoxFuture;

/// Wrap an async fn as middleware, eliminating `Box::pin(...) as BoxFuture<...>` boilerplate.
///
/// Before:
/// ```ignore
/// let auth = |ctx: AppCtx, mw: MiddlewareCtx<AuthCtx>| {
///     Box::pin(async move { mw.next(AuthCtx { ... }).await })
///         as BoxFuture<'static, Result<MiddlewareOutput, ProcedureError>>
/// };
/// ```
///
/// After:
/// ```ignore
/// let auth = middleware_fn(|ctx: AppCtx, mw: MiddlewareCtx<AuthCtx>| async move {
///     mw.next(AuthCtx { ... }).await
/// });
/// ```
pub fn middleware_fn<TCtx, TNextCtx, F, Fut>(
    f: F,
) -> impl Fn(TCtx, MiddlewareCtx<TNextCtx>) -> BoxFuture<'static, Result<MiddlewareOutput, ProcedureError>>
       + Send
       + Sync
       + 'static
where
    F: Fn(TCtx, MiddlewareCtx<TNextCtx>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<MiddlewareOutput, ProcedureError>> + Send + 'static,
{
    move |ctx, mw| Box::pin(f(ctx, mw))
}

/// Type alias for the inner handler passed through the middleware chain.
type InnerHandler<TCtx> =
    Box<dyn FnOnce(TCtx, DynInput) -> BoxFuture<'static, Result<DynOutput, ProcedureError>> + Send>;

// ---------------------------------------------------------------------------
// Internal MiddlewareChain trait (NOT user-facing)
// ---------------------------------------------------------------------------

/// Internal trait for compile-time middleware composition.
///
/// Each layer transforms `TBaseCtx → TCurrentCtx` by chaining with the previous layer.
/// NOT exposed to users — users write middleware as closures.
pub(crate) trait MiddlewareChain<TBaseCtx, TCurrentCtx>: Send + Sync + 'static {
    /// Run the middleware chain.
    ///
    /// - `ctx`: The base context (entry point).
    /// - `input`: Type-erased input.
    /// - `meta`: Procedure metadata (route info, accessible to middleware).
    /// - `inner_handler`: The next step after all middleware (typically deserialization + user handler).
    ///   Consumed once per invocation (FnOnce).
    fn run(
        &self,
        ctx: TBaseCtx,
        input: DynInput,
        meta: ProcedureMeta,
        inner_handler: InnerHandler<TCurrentCtx>,
    ) -> BoxFuture<'static, Result<DynOutput, ProcedureError>>;
}

// ---------------------------------------------------------------------------
// IdentityChain — base case (no middleware applied)
// ---------------------------------------------------------------------------

/// Identity chain: passes context and input through unchanged.
/// Used when `TBaseCtx == TCurrentCtx` (no middleware added yet).
pub(crate) struct IdentityChain;

impl<TCtx: Send + 'static> MiddlewareChain<TCtx, TCtx> for IdentityChain {
    fn run(
        &self,
        ctx: TCtx,
        input: DynInput,
        _meta: ProcedureMeta,
        inner_handler: InnerHandler<TCtx>,
    ) -> BoxFuture<'static, Result<DynOutput, ProcedureError>> {
        inner_handler(ctx, input)
    }
}

// ---------------------------------------------------------------------------
// ComposedChain — adds one middleware layer
// ---------------------------------------------------------------------------

/// Composed chain: wraps an existing chain (`prev`) with a new middleware layer.
///
/// Generic parameters:
/// - `TBaseCtx`: Initial context (fixed across all layers)
/// - `TMidCtx`: Context produced by `prev` chain (input to this middleware)
/// - `TCurrentCtx`: Context produced by this middleware (output)
/// - `M`: The middleware closure type
pub(crate) struct ComposedChain<TBaseCtx, TMidCtx, TCurrentCtx, M> {
    prev: Arc<dyn MiddlewareChain<TBaseCtx, TMidCtx>>,
    middleware: Arc<M>,
    _phantom: PhantomData<fn(TBaseCtx, TMidCtx, TCurrentCtx)>,
}

impl<TBaseCtx, TMidCtx, TCurrentCtx, M> ComposedChain<TBaseCtx, TMidCtx, TCurrentCtx, M> {
    pub fn new(
        prev: Arc<dyn MiddlewareChain<TBaseCtx, TMidCtx>>,
        middleware: Arc<M>,
    ) -> Self {
        ComposedChain {
            prev,
            middleware,
            _phantom: PhantomData,
        }
    }
}

impl<TBaseCtx, TMidCtx, TCurrentCtx, M> MiddlewareChain<TBaseCtx, TCurrentCtx>
    for ComposedChain<TBaseCtx, TMidCtx, TCurrentCtx, M>
where
    TBaseCtx: Send + 'static,
    TMidCtx: Send + 'static,
    TCurrentCtx: Send + 'static,
    M: Fn(TMidCtx, MiddlewareCtx<TCurrentCtx>) -> BoxFuture<'static, Result<MiddlewareOutput, ProcedureError>>
        + Send
        + Sync
        + 'static,
{
    fn run(
        &self,
        ctx: TBaseCtx,
        input: DynInput,
        meta: ProcedureMeta,
        inner_handler: InnerHandler<TCurrentCtx>,
    ) -> BoxFuture<'static, Result<DynOutput, ProcedureError>> {
        let middleware = self.middleware.clone();
        let meta_for_mw = meta.clone();

        // Run the previous chain (TBaseCtx → TMidCtx), then invoke our middleware.
        // The inner handler for `prev` creates a MiddlewareCtx and calls our middleware.
        self.prev.run(
            ctx,
            input,
            meta,
            Box::new(move |mid_ctx: TMidCtx, input: DynInput| {
                let mw_ctx = MiddlewareCtx {
                    next_fn: inner_handler,
                    dyn_input: input,
                    meta: meta_for_mw,
                };
                Box::pin(async move {
                    let result = middleware(mid_ctx, mw_ctx).await?;
                    Ok(result.output)
                })
            }),
        )
    }
}

// ---------------------------------------------------------------------------
// User-facing types
// ---------------------------------------------------------------------------

/// Context provided to middleware closures.
///
/// Holds the continuation (`next_fn`) and the type-erased input. Provides methods to:
/// - `next(ctx)`: Continue to the next middleware/handler with a new context
/// - `next_with_input(ctx, input)`: Continue with a replaced input
/// - `output(value)`: Short-circuit and return output directly (for caching, rate-limiting)
/// - `input()`: Inspect the type-erased input without consuming it
pub struct MiddlewareCtx<TNextCtx> {
    next_fn: InnerHandler<TNextCtx>,
    dyn_input: DynInput,
    meta: ProcedureMeta,
}

impl<TNextCtx> MiddlewareCtx<TNextCtx> {
    /// Continue to the next middleware/handler with a new context.
    /// The original input is forwarded automatically.
    pub async fn next(self, ctx: TNextCtx) -> Result<MiddlewareOutput, ProcedureError> {
        let output = (self.next_fn)(ctx, self.dyn_input).await?;
        Ok(MiddlewareOutput { output })
    }

    /// Continue with a replaced input.
    pub async fn next_with_input(
        self,
        ctx: TNextCtx,
        input: DynInput,
    ) -> Result<MiddlewareOutput, ProcedureError> {
        let output = (self.next_fn)(ctx, input).await?;
        Ok(MiddlewareOutput { output })
    }

    /// Short-circuit: return output directly WITHOUT calling next.
    /// Useful for caching, rate limiting, etc.
    pub fn output<T: Serialize + Send + 'static>(
        self,
        value: T,
    ) -> Result<MiddlewareOutput, ProcedureError> {
        Ok(MiddlewareOutput {
            output: DynOutput::new(value),
        })
    }

    /// Inspect the type-erased input (only if already materialized as Value).
    /// Returns `None` if the input is a raw Deserializer.
    /// Call `materialize_input()` first to convert Deserializer to Value.
    pub fn input(&self) -> Option<&serde_json::Value> {
        self.dyn_input.as_value()
    }

    /// Materialize the input: convert `Deserializer` variant to `Value` variant.
    /// After this call, `input()` will return `Some`. No-op if already materialized.
    pub fn materialize_input(&mut self) -> Result<(), ProcedureError> {
        // Take ownership temporarily, materialize, put back
        let input = std::mem::replace(&mut self.dyn_input, DynInput::from_value(serde_json::Value::Null));
        self.dyn_input = input.materialize()?;
        Ok(())
    }

    /// Access procedure metadata.
    pub fn meta(&self) -> &ProcedureMeta {
        &self.meta
    }
}

/// Output wrapper returned by middleware.
pub struct MiddlewareOutput {
    pub output: DynOutput,
}

/// Procedure metadata accessible to middleware.
#[derive(Clone)]
pub struct ProcedureMeta {
    pub route: Route,
}

#[cfg(test)]
mod tests {
    use super::*;
    use orpc_procedure::DynInput;

    fn test_meta() -> ProcedureMeta {
        ProcedureMeta {
            route: Route::get("/test"),
        }
    }

    #[tokio::test]
    async fn identity_chain_passthrough() {
        let chain = IdentityChain;
        let input = DynInput::from_value(serde_json::json!(42));

        let result = chain
            .run(
                "context",
                input,
                test_meta(),
                Box::new(|ctx: &str, input: DynInput| {
                    Box::pin(async move {
                        let val: i32 = input.deserialize()?;
                        Ok(DynOutput::new(format!("{ctx}:{val}")))
                    })
                }),
            )
            .await
            .unwrap();

        assert_eq!(result.to_value().unwrap(), serde_json::json!("context:42"));
    }

    #[tokio::test]
    async fn composed_chain_context_switch() {
        // prev: IdentityChain (u32 → u32)
        let prev: Arc<dyn MiddlewareChain<u32, u32>> = Arc::new(IdentityChain);

        // middleware: u32 → String (context switch)
        let middleware = Arc::new(
            |ctx: u32, mw: MiddlewareCtx<String>| -> BoxFuture<'static, Result<MiddlewareOutput, ProcedureError>> {
                Box::pin(async move { mw.next(format!("user-{ctx}")).await })
            },
        );

        let chain = ComposedChain::new(prev, middleware);
        let input = DynInput::from_value(serde_json::json!("hello"));

        let result = chain
            .run(
                42u32,
                input,
                test_meta(),
                Box::new(|ctx: String, input: DynInput| {
                    Box::pin(async move {
                        let val: String = input.deserialize()?;
                        Ok(DynOutput::new(format!("{ctx}:{val}")))
                    })
                }),
            )
            .await
            .unwrap();

        assert_eq!(
            result.to_value().unwrap(),
            serde_json::json!("user-42:hello")
        );
    }

    #[tokio::test]
    async fn middleware_output_short_circuit() {
        let prev: Arc<dyn MiddlewareChain<(), ()>> = Arc::new(IdentityChain);

        let middleware = Arc::new(
            |_ctx: (), mw: MiddlewareCtx<()>| -> BoxFuture<'static, Result<MiddlewareOutput, ProcedureError>> {
                Box::pin(async move { mw.output("cached response") })
            },
        );

        let chain = ComposedChain::new(prev, middleware);
        let input = DynInput::from_value(serde_json::json!(null));

        let result = chain
            .run(
                (),
                input,
                test_meta(),
                Box::new(|_ctx: (), _input: DynInput| {
                    Box::pin(async move { panic!("should not be called") })
                }),
            )
            .await
            .unwrap();

        assert_eq!(
            result.to_value().unwrap(),
            serde_json::json!("cached response")
        );
    }

    #[tokio::test]
    async fn double_middleware_chain() {
        // Chain: u32 → String → (String, bool)
        let identity: Arc<dyn MiddlewareChain<u32, u32>> = Arc::new(IdentityChain);

        // First middleware: u32 → String
        let mw1 = Arc::new(
            |ctx: u32, mw: MiddlewareCtx<String>| -> BoxFuture<'static, Result<MiddlewareOutput, ProcedureError>> {
                Box::pin(async move { mw.next(format!("user-{ctx}")).await })
            },
        );
        let chain1: Arc<dyn MiddlewareChain<u32, String>> =
            Arc::new(ComposedChain::new(identity, mw1));

        // Second middleware: String → (String, bool)
        let mw2 = Arc::new(
            |ctx: String, mw: MiddlewareCtx<(String, bool)>| -> BoxFuture<'static, Result<MiddlewareOutput, ProcedureError>> {
                Box::pin(async move { mw.next((ctx, true)).await })
            },
        );
        let chain2 = ComposedChain::new(chain1, mw2);

        let input = DynInput::from_value(serde_json::json!("test"));
        let result = chain2
            .run(
                42u32,
                input,
                test_meta(),
                Box::new(|ctx: (String, bool), input: DynInput| {
                    Box::pin(async move {
                        let val: String = input.deserialize()?;
                        Ok(DynOutput::new(format!(
                            "{}:{}:{}",
                            ctx.0, ctx.1, val
                        )))
                    })
                }),
            )
            .await
            .unwrap();

        assert_eq!(
            result.to_value().unwrap(),
            serde_json::json!("user-42:true:test")
        );
    }

    #[test]
    fn middleware_ctx_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<MiddlewareCtx<()>>();
        assert_send::<MiddlewareOutput>();
    }
}
