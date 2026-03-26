use std::marker::PhantomData;
use std::sync::Arc;

use orpc_procedure::{
    DynInput, DynOutput, ErasedSchema, ErrorMap, Meta, ProcedureError, ProcedureStream, Route,
};
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::context::Context;
use crate::error::ORPCError;
use crate::handler::{BoxFuture, Handler};
use crate::middleware::{
    ComposedChain, IdentityChain, MiddlewareChain, MiddlewareCtx, MiddlewareOutput, ProcedureMeta,
};
use crate::procedure::Procedure;
use crate::schema::{Schema, SchemaAdapter};

/// Create a new procedure builder with the given context type.
///
/// This is the main entry point for building procedures:
/// ```ignore
/// let proc = os::<AppCtx>()
///     .use_middleware(auth)
///     .input(Identity::<GetUserInput>::new())
///     .handler(get_user);
/// ```
pub fn os<TCtx: Context>() -> Builder<TCtx, TCtx> {
    Builder {
        middleware_chain: Arc::new(IdentityChain),
        error_map: ErrorMap::default(),
        route: Route::default(),
        meta: Meta::default(),
        _phantom: PhantomData,
    }
}

/// Procedure builder with typestate pattern.
///
/// Tracks both `TBaseCtx` (initial context from router) and `TCtx` (current context
/// after middleware transformations). `TError` defaults to `ORPCError`.
pub struct Builder<TBaseCtx, TCtx, TError = ORPCError> {
    pub(crate) middleware_chain: Arc<dyn MiddlewareChain<TBaseCtx, TCtx>>,
    pub(crate) error_map: ErrorMap,
    pub(crate) route: Route,
    pub(crate) meta: Meta,
    pub(crate) _phantom: PhantomData<fn(TError)>,
}

impl<TBaseCtx: Context, TCtx: Context, TError> Builder<TBaseCtx, TCtx, TError> {
    /// Add middleware that transforms context: `TCtx → TNextCtx`.
    ///
    /// The middleware is composed into the chain at compile time (NOT stored in a Vec).
    pub fn use_middleware<TNextCtx, M>(self, m: M) -> Builder<TBaseCtx, TNextCtx, TError>
    where
        M: Fn(
                TCtx,
                MiddlewareCtx<TNextCtx>,
            ) -> BoxFuture<'static, Result<MiddlewareOutput, ProcedureError>>
            + Send
            + Sync
            + 'static,
        TNextCtx: Context,
    {
        Builder {
            middleware_chain: Arc::new(ComposedChain::new(self.middleware_chain, Arc::new(m))),
            error_map: self.error_map,
            route: self.route,
            meta: self.meta,
            _phantom: PhantomData,
        }
    }

    /// Set HTTP route metadata.
    pub fn route(mut self, route: Route) -> Self {
        self.route = route;
        self
    }

    /// Set the input schema, transitioning to `BuilderWithInput`.
    pub fn input<S: Schema>(self, schema: S) -> BuilderWithInput<TBaseCtx, TCtx, S::Output, TError> {
        BuilderWithInput {
            middleware_chain: self.middleware_chain,
            error_map: self.error_map,
            route: self.route,
            meta: self.meta,
            input_schema: Box::new(SchemaAdapter(schema)),
            _phantom: PhantomData,
        }
    }

    /// Set handler directly (no input schema — handler receives `()`).
    pub fn handler<F, TOutput>(self, f: F) -> Procedure<TBaseCtx, (), TOutput, TError>
    where
        F: Handler<TCtx, (), TOutput, TError>,
        TOutput: Serialize + Send + 'static,
        TError: Into<ProcedureError> + Send + 'static,
    {
        build_procedure(
            self.middleware_chain,
            f,
            None,
            None,
            self.error_map,
            self.route,
            self.meta,
        )
    }
}

/// Builder after `.input(schema)` has been called.
pub struct BuilderWithInput<TBaseCtx, TCtx, TInput, TError = ORPCError> {
    middleware_chain: Arc<dyn MiddlewareChain<TBaseCtx, TCtx>>,
    error_map: ErrorMap,
    route: Route,
    meta: Meta,
    input_schema: Box<dyn ErasedSchema>,
    _phantom: PhantomData<fn(TInput, TError)>,
}

impl<TBaseCtx: Context, TCtx: Context, TInput, TError>
    BuilderWithInput<TBaseCtx, TCtx, TInput, TError>
{
    /// Set the output schema, transitioning to `BuilderWithIO`.
    pub fn output<S: Schema>(
        self,
        schema: S,
    ) -> BuilderWithIO<TBaseCtx, TCtx, TInput, S::Output, TError> {
        BuilderWithIO {
            middleware_chain: self.middleware_chain,
            error_map: self.error_map,
            route: self.route,
            meta: self.meta,
            input_schema: self.input_schema,
            output_schema: Box::new(SchemaAdapter(schema)),
            _phantom: PhantomData,
        }
    }

    /// Set handler (with input schema, no output schema).
    pub fn handler<F, TOutput>(self, f: F) -> Procedure<TBaseCtx, TInput, TOutput, TError>
    where
        F: Handler<TCtx, TInput, TOutput, TError>,
        TInput: DeserializeOwned + Send + 'static,
        TOutput: Serialize + Send + 'static,
        TError: Into<ProcedureError> + Send + 'static,
    {
        build_procedure(
            self.middleware_chain,
            f,
            Some(self.input_schema),
            None,
            self.error_map,
            self.route,
            self.meta,
        )
    }
}

/// Builder after both `.input(schema)` and `.output(schema)` have been called.
pub struct BuilderWithIO<TBaseCtx, TCtx, TInput, TOutput, TError = ORPCError> {
    middleware_chain: Arc<dyn MiddlewareChain<TBaseCtx, TCtx>>,
    error_map: ErrorMap,
    route: Route,
    meta: Meta,
    input_schema: Box<dyn ErasedSchema>,
    output_schema: Box<dyn ErasedSchema>,
    _phantom: PhantomData<fn(TInput, TOutput, TError)>,
}

impl<TBaseCtx: Context, TCtx: Context, TInput, TOutput, TError>
    BuilderWithIO<TBaseCtx, TCtx, TInput, TOutput, TError>
{
    /// Set handler (with both input and output schemas).
    pub fn handler<F>(self, f: F) -> Procedure<TBaseCtx, TInput, TOutput, TError>
    where
        F: Handler<TCtx, TInput, TOutput, TError>,
        TInput: DeserializeOwned + Send + 'static,
        TOutput: Serialize + Send + 'static,
        TError: Into<ProcedureError> + Send + 'static,
    {
        build_procedure(
            self.middleware_chain,
            f,
            Some(self.input_schema),
            Some(self.output_schema),
            self.error_map,
            self.route,
            self.meta,
        )
    }
}

/// Internal: build the exec closure that composes middleware chain + handler.
///
/// This is the critical function that bakes `TInput`/`TOutput`/`TError` into
/// the type-erased `exec: Arc<dyn Fn(TBaseCtx, DynInput) -> ProcedureStream>`.
fn build_procedure<TBaseCtx, TCtx, TInput, TOutput, TError, F>(
    middleware_chain: Arc<dyn MiddlewareChain<TBaseCtx, TCtx>>,
    handler: F,
    input_schema: Option<Box<dyn ErasedSchema>>,
    output_schema: Option<Box<dyn ErasedSchema>>,
    error_map: ErrorMap,
    route: Route,
    meta: Meta,
) -> Procedure<TBaseCtx, TInput, TOutput, TError>
where
    TBaseCtx: Context,
    TCtx: Context,
    TInput: DeserializeOwned + Send + 'static,
    TOutput: Serialize + Send + 'static,
    TError: Into<ProcedureError> + Send + 'static,
    F: Handler<TCtx, TInput, TOutput, TError>,
{
    let handler = Arc::new(handler);
    let route_for_meta = route.clone();

    let exec = Arc::new(move |base_ctx: TBaseCtx, dyn_input: DynInput| {
        let handler = handler.clone();
        let chain = middleware_chain.clone();
        let _procedure_meta = ProcedureMeta {
            route: route_for_meta.clone(),
        };

        ProcedureStream::from_future(async move {
            chain
                .run(
                    base_ctx,
                    dyn_input,
                    Box::new(move |ctx: TCtx, input: DynInput| -> BoxFuture<'static, Result<DynOutput, ProcedureError>> {
                        Box::pin(async move {
                            let typed_input: TInput = input.deserialize()?;
                            let result = handler
                                .call(ctx, typed_input)
                                .await
                                .map_err(|e| -> ProcedureError { e.into() })?;
                            Ok(DynOutput::new(result))
                        })
                    }),
                )
                .await
        })
    });

    Procedure {
        exec,
        input_schema,
        output_schema,
        error_map,
        route,
        meta,
        _phantom: PhantomData,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::Identity;
    use futures_util::StreamExt;
    use serde::Deserialize;

    #[derive(Debug, Deserialize, Serialize)]
    struct GreetInput {
        name: String,
    }

    async fn greet_handler(_ctx: (), input: GreetInput) -> Result<String, ORPCError> {
        Ok(format!("Hello, {}!", input.name))
    }

    #[tokio::test]
    async fn basic_builder_no_middleware() {
        let proc = os::<()>()
            .route(Route::post("/greet"))
            .input(Identity::<GreetInput>::new())
            .handler(greet_handler);

        let erased = proc.into_erased();
        let input = DynInput::from_value(serde_json::json!({"name": "World"}));
        let mut stream = erased.exec((), input);
        let result = stream.next().await.unwrap().unwrap();
        assert_eq!(result.to_value().unwrap(), serde_json::json!("Hello, World!"));
    }

    #[tokio::test]
    async fn builder_with_middleware_context_switch() {
        struct AppCtx {
            user_id: u32,
        }
        struct AuthCtx {
            user: String,
        }

        let auth_mw = |ctx: AppCtx, mw: MiddlewareCtx<AuthCtx>| {
            Box::pin(async move {
                mw.next(AuthCtx {
                    user: format!("user-{}", ctx.user_id),
                })
                .await
            }) as BoxFuture<'static, Result<MiddlewareOutput, ProcedureError>>
        };

        async fn handler(ctx: AuthCtx, input: GreetInput) -> Result<String, ORPCError> {
            Ok(format!("Hello {}, from {}!", input.name, ctx.user))
        }

        let proc = os::<AppCtx>()
            .use_middleware(auth_mw)
            .input(Identity::<GreetInput>::new())
            .handler(handler);

        let erased = proc.into_erased();
        let input = DynInput::from_value(serde_json::json!({"name": "World"}));
        let mut stream = erased.exec(AppCtx { user_id: 42 }, input);
        let result = stream.next().await.unwrap().unwrap();
        assert_eq!(
            result.to_value().unwrap(),
            serde_json::json!("Hello World, from user-42!")
        );
    }

    #[tokio::test]
    async fn builder_no_input_handler() {
        async fn ping(_ctx: (), _input: ()) -> Result<String, ORPCError> {
            Ok("pong".into())
        }

        let proc = os::<()>().handler(ping);
        let erased = proc.into_erased();
        let input = DynInput::from_value(serde_json::json!(null));
        let mut stream = erased.exec((), input);
        let result = stream.next().await.unwrap().unwrap();
        assert_eq!(result.to_value().unwrap(), serde_json::json!("pong"));
    }

    #[tokio::test]
    async fn builder_with_output_schema() {
        let proc = os::<()>()
            .input(Identity::<GreetInput>::new())
            .output(Identity::<String>::new())
            .handler(greet_handler);

        assert!(proc.input_schema.is_some());
        assert!(proc.output_schema.is_some());

        let erased = proc.into_erased();
        let input = DynInput::from_value(serde_json::json!({"name": "Test"}));
        let mut stream = erased.exec((), input);
        let result = stream.next().await.unwrap().unwrap();
        assert_eq!(result.to_value().unwrap(), serde_json::json!("Hello, Test!"));
    }

    #[tokio::test]
    async fn multiple_calls_to_same_procedure() {
        let proc = os::<u32>()
            .input(Identity::<String>::new())
            .handler(|ctx: u32, input: String| async move {
                Ok::<_, ORPCError>(format!("{ctx}:{input}"))
            });

        let erased = proc.into_erased();

        for i in 0..3 {
            let input = DynInput::from_value(serde_json::json!(format!("call-{i}")));
            let mut stream = erased.exec(i, input);
            let result = stream.next().await.unwrap().unwrap();
            assert_eq!(
                result.to_value().unwrap(),
                serde_json::json!(format!("{i}:call-{i}"))
            );
        }
    }
}
