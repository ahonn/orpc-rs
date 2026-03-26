use std::marker::PhantomData;
use std::sync::Arc;

use orpc_procedure::{ErasedSchema, ProcedureError};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::builder::{Builder, build_procedure};
use crate::context::Context;
use crate::contract::ContractProcedure;
use crate::handler::{BoxFuture, Handler};
use crate::middleware::{IdentityChain, MiddlewareCtx, MiddlewareOutput};
use crate::procedure::Procedure;

/// Create a `ContractImplementer` from a contract, bound to a specific base context type.
///
/// The returned implementer constrains the handler's input/output types to match the contract.
///
/// ```ignore
/// let proc = implement::<AppCtx, _, _, _>(contract)
///     .handler(get_user_handler);
/// // Compiler enforces: handler must be Fn(AppCtx, GetUserInput) -> Result<User, ORPCError>
/// ```
pub fn implement<TBaseCtx: Context, TInput, TOutput, TError>(
    contract: ContractProcedure<TInput, TOutput, TError>,
) -> ContractImplementer<TBaseCtx, TInput, TOutput, TError>
where
    TError: Into<ProcedureError> + Send + 'static,
{
    ContractImplementer {
        builder: Builder {
            middleware_chain: Arc::new(IdentityChain),
            error_map: contract.error_map,
            route: contract.route,
            meta: contract.meta,
            _phantom: PhantomData,
        },
        input_schema: contract.input_schema,
        output_schema: contract.output_schema,
        _phantom: PhantomData,
    }
}

/// Contract implementer before any middleware is applied.
///
/// Handler context type is `TBaseCtx` (same as the base context).
/// Input/output/error types are locked by the contract's generics.
pub struct ContractImplementer<TBaseCtx, TInput, TOutput, TError> {
    builder: Builder<TBaseCtx, TBaseCtx, TError>,
    input_schema: Option<Box<dyn ErasedSchema>>,
    output_schema: Option<Box<dyn ErasedSchema>>,
    _phantom: PhantomData<fn(TInput, TOutput)>,
}

impl<TBaseCtx: Context, TInput, TOutput, TError>
    ContractImplementer<TBaseCtx, TInput, TOutput, TError>
where
    TInput: DeserializeOwned + Send + 'static,
    TOutput: Serialize + Send + 'static,
    TError: Into<ProcedureError> + Send + 'static,
{
    /// Add middleware (context switching allowed).
    /// Transforms handler context from `TBaseCtx` to `TNextCtx`.
    pub fn use_middleware<TNextCtx, M>(
        self,
        m: M,
    ) -> ContractImplementerWithMw<TBaseCtx, TNextCtx, TInput, TOutput, TError>
    where
        M: Fn(
                TBaseCtx,
                MiddlewareCtx<TNextCtx>,
            ) -> BoxFuture<'static, Result<MiddlewareOutput, ProcedureError>>
            + Send
            + Sync
            + 'static,
        TNextCtx: Context,
    {
        ContractImplementerWithMw {
            builder: self.builder.use_middleware(m),
            input_schema: self.input_schema,
            output_schema: self.output_schema,
            _phantom: PhantomData,
        }
    }

    /// Set handler. Input/output types are constrained by the contract.
    /// Without middleware, the handler receives `TBaseCtx` directly.
    pub fn handler<F>(self, f: F) -> Procedure<TBaseCtx, TInput, TOutput, TError>
    where
        F: Handler<TBaseCtx, TInput, TOutput, TError>,
    {
        build_procedure(
            self.builder.middleware_chain,
            f,
            self.input_schema,
            None, // Contract validation is metadata-only for now
            self.output_schema,
            self.builder.error_map,
            self.builder.route,
            self.builder.meta,
        )
    }
}

/// Contract implementer after middleware has been applied.
///
/// Handler context type is `TCtx` (transformed by middleware).
/// Input/output types remain locked by the contract.
pub struct ContractImplementerWithMw<TBaseCtx, TCtx, TInput, TOutput, TError> {
    builder: Builder<TBaseCtx, TCtx, TError>,
    input_schema: Option<Box<dyn ErasedSchema>>,
    output_schema: Option<Box<dyn ErasedSchema>>,
    _phantom: PhantomData<fn(TInput, TOutput)>,
}

impl<TBaseCtx: Context, TCtx: Context, TInput, TOutput, TError>
    ContractImplementerWithMw<TBaseCtx, TCtx, TInput, TOutput, TError>
where
    TInput: DeserializeOwned + Send + 'static,
    TOutput: Serialize + Send + 'static,
    TError: Into<ProcedureError> + Send + 'static,
{
    /// Add more middleware (further context switching).
    pub fn use_middleware<TNextCtx, M>(
        self,
        m: M,
    ) -> ContractImplementerWithMw<TBaseCtx, TNextCtx, TInput, TOutput, TError>
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
        ContractImplementerWithMw {
            builder: self.builder.use_middleware(m),
            input_schema: self.input_schema,
            output_schema: self.output_schema,
            _phantom: PhantomData,
        }
    }

    /// Set handler. Input/output types are constrained by the contract.
    /// The handler receives `TCtx` (after middleware transformation).
    pub fn handler<F>(self, f: F) -> Procedure<TBaseCtx, TInput, TOutput, TError>
    where
        F: Handler<TCtx, TInput, TOutput, TError>,
    {
        build_procedure(
            self.builder.middleware_chain,
            f,
            self.input_schema,
            None, // Contract validation is metadata-only for now
            self.output_schema,
            self.builder.error_map,
            self.builder.route,
            self.builder.meta,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::oc;
    use crate::schema::Identity;
    use futures_util::StreamExt;
    use orpc_procedure::{DynInput, Route};
    use serde::Deserialize;

    #[derive(Debug, Deserialize, Serialize, PartialEq)]
    struct GetUserInput {
        id: u32,
    }

    #[derive(Debug, Deserialize, Serialize, PartialEq)]
    struct User {
        id: u32,
        name: String,
    }

    async fn get_user_handler(_ctx: (), input: GetUserInput) -> Result<User, crate::ORPCError> {
        Ok(User {
            id: input.id,
            name: format!("User {}", input.id),
        })
    }

    #[tokio::test]
    async fn implement_without_middleware() {
        let contract = oc()
            .route(Route::get("/users/{id}"))
            .input(Identity::<GetUserInput>::new())
            .output(Identity::<User>::new())
            .build();

        let proc = implement::<(), _, _, _>(contract).handler(get_user_handler);

        let erased = proc.into_erased();
        let input = DynInput::from_value(serde_json::json!({"id": 42}));
        let mut stream = erased.exec((), input);
        let result = stream.next().await.unwrap().unwrap();
        let user: User = serde_json::from_value(result.to_value().unwrap()).unwrap();
        assert_eq!(
            user,
            User {
                id: 42,
                name: "User 42".into()
            }
        );
    }

    #[tokio::test]
    async fn implement_with_middleware() {
        struct AppCtx {
            token: String,
        }
        struct AuthCtx {
            user_name: String,
        }

        let auth_mw = |ctx: AppCtx, mw: MiddlewareCtx<AuthCtx>| {
            Box::pin(async move {
                if ctx.token == "valid" {
                    mw.next(AuthCtx {
                        user_name: "Alice".into(),
                    })
                    .await
                } else {
                    Err(ProcedureError::Resolver(Box::new(
                        crate::ORPCError::unauthorized("Invalid token"),
                    )))
                }
            }) as BoxFuture<'static, Result<MiddlewareOutput, ProcedureError>>
        };

        async fn handler(ctx: AuthCtx, input: GetUserInput) -> Result<User, crate::ORPCError> {
            Ok(User {
                id: input.id,
                name: ctx.user_name,
            })
        }

        let contract = oc()
            .input(Identity::<GetUserInput>::new())
            .output(Identity::<User>::new())
            .build();

        let proc = implement::<AppCtx, _, _, _>(contract)
            .use_middleware(auth_mw)
            .handler(handler);

        let erased = proc.into_erased();

        // Valid token
        let input = DynInput::from_value(serde_json::json!({"id": 1}));
        let mut stream = erased.exec(
            AppCtx {
                token: "valid".into(),
            },
            input,
        );
        let result = stream.next().await.unwrap().unwrap();
        let user: User = serde_json::from_value(result.to_value().unwrap()).unwrap();
        assert_eq!(user.name, "Alice");

        // Invalid token
        let input = DynInput::from_value(serde_json::json!({"id": 1}));
        let mut stream = erased.exec(
            AppCtx {
                token: "bad".into(),
            },
            input,
        );
        let result = stream.next().await.unwrap();
        assert!(matches!(result, Err(ProcedureError::Resolver(_))));
    }

    #[test]
    fn contract_preserves_route_in_procedure() {
        let contract = oc()
            .route(Route::get("/users/{id}").tag("users"))
            .input(Identity::<GetUserInput>::new())
            .output(Identity::<User>::new())
            .build();

        let proc = implement::<(), _, _, _>(contract).handler(get_user_handler);
        let erased = proc.into_erased();
        assert_eq!(erased.route.method, Some(orpc_procedure::HttpMethod::Get));
        assert_eq!(erased.route.path.as_deref(), Some("/users/{id}"));
        assert_eq!(erased.route.tags, vec!["users"]);
    }
}
