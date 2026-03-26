use std::future::Future;
use std::pin::Pin;

/// Pinned, boxed, Send future. Used throughout the middleware and handler system.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Handler function signature. Auto-implemented for async fn with matching signature.
///
/// Prefer named async functions over closures for better type inference:
/// ```ignore
/// async fn list_planets(ctx: AppCtx, input: ListInput) -> Result<Vec<Planet>, ORPCError> { ... }
/// os.handler(list_planets)
/// ```
pub trait Handler<TCtx, TInput, TOutput, TError>: Send + Sync + 'static {
    fn call(&self, ctx: TCtx, input: TInput) -> BoxFuture<'static, Result<TOutput, TError>>;
}

/// Blanket impl for `Fn(TCtx, TInput) -> Future<Output = Result<TOutput, TError>>`.
///
/// This allows both named async fns and closures to be used as handlers.
impl<F, Fut, TCtx, TInput, TOutput, TError> Handler<TCtx, TInput, TOutput, TError> for F
where
    F: Fn(TCtx, TInput) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<TOutput, TError>> + Send + 'static,
    TCtx: Send + 'static,
    TInput: Send + 'static,
    TOutput: Send + 'static,
    TError: Send + 'static,
{
    fn call(&self, ctx: TCtx, input: TInput) -> BoxFuture<'static, Result<TOutput, TError>> {
        Box::pin((self)(ctx, input))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn sample_handler(_ctx: (), input: String) -> Result<String, String> {
        Ok(format!("Hello, {input}"))
    }

    #[tokio::test]
    async fn named_async_fn_as_handler() {
        let result = Handler::call(&sample_handler, (), "World".to_string()).await;
        assert_eq!(result.unwrap(), "Hello, World");
    }

    #[tokio::test]
    async fn closure_as_handler() {
        let handler = |_ctx: (), input: u32| async move { Ok::<_, String>(input * 2) };
        let result = Handler::call(&handler, (), 21).await;
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn handler_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Box<dyn Handler<(), String, String, String>>>();
    }
}
