use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use crate::error::ProcedureError;
use crate::input::DynInput;
use crate::route::{ErrorMap, Meta, Route};
use crate::schema::ErasedSchema;
use crate::stream::ProcedureStream;

/// Fully type-erased procedure. Only `TCtx` (= TBaseCtx) remains as generic.
///
/// This is what Router and transport integrations work with.
/// `TInput`/`TOutput`/`TError` are captured inside the `exec` closure —
/// deserialization and serialization happen internally.
pub struct ErasedProcedure<TCtx> {
    exec: Arc<dyn Fn(TCtx, DynInput) -> ProcedureStream + Send + Sync>,
    pub input_schema: Option<Box<dyn ErasedSchema>>,
    pub output_schema: Option<Box<dyn ErasedSchema>>,
    pub error_map: ErrorMap,
    pub route: Route,
    pub meta: Meta,
}

impl<TCtx> ErasedProcedure<TCtx> {
    /// Create a new procedure with the given execution closure.
    ///
    /// Schemas and error_map default to None/default.
    pub fn new(
        exec: impl Fn(TCtx, DynInput) -> ProcedureStream + Send + Sync + 'static,
        route: Route,
        meta: Meta,
    ) -> Self {
        ErasedProcedure {
            exec: Arc::new(exec),
            input_schema: None,
            output_schema: None,
            error_map: ErrorMap::default(),
            route,
            meta,
        }
    }

    /// Set the input schema.
    pub fn with_input_schema(mut self, schema: impl ErasedSchema) -> Self {
        self.input_schema = Some(Box::new(schema));
        self
    }

    /// Set the output schema.
    pub fn with_output_schema(mut self, schema: impl ErasedSchema) -> Self {
        self.output_schema = Some(Box::new(schema));
        self
    }

    /// Set the error map.
    pub fn with_error_map(mut self, error_map: ErrorMap) -> Self {
        self.error_map = error_map;
        self
    }

    /// Execute the procedure with type-erased input.
    ///
    /// Wraps the call in `catch_unwind` to prevent handler panics from
    /// crashing the server. Panics are caught and returned as
    /// `ProcedureError::Unwind`.
    ///
    /// **Note**: Only synchronous panics (during closure invocation) are caught.
    /// Panics inside the returned `ProcedureStream`'s async polling are NOT
    /// caught — this is a known limitation shared with rspc.
    pub fn exec(&self, ctx: TCtx, input: DynInput) -> ProcedureStream {
        match catch_unwind(AssertUnwindSafe(|| (self.exec)(ctx, input))) {
            Ok(stream) => stream,
            Err(panic) => ProcedureStream::error(ProcedureError::Unwind(panic)),
        }
    }
}

impl<TCtx> std::fmt::Debug for ErasedProcedure<TCtx> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ErasedProcedure")
            .field("route", &self.route)
            .field("meta", &self.meta)
            .field("has_input_schema", &self.input_schema.is_some())
            .field("has_output_schema", &self.output_schema.is_some())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::DynOutput;
    use futures_util::StreamExt;
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    struct TestInput {
        value: u32,
    }

    #[tokio::test]
    async fn basic_exec() {
        let proc = ErasedProcedure::new(
            |_ctx: (), input: DynInput| {
                let test_input: TestInput = input.deserialize().unwrap();
                ProcedureStream::from_future(async move {
                    Ok(DynOutput::new(test_input.value * 2))
                })
            },
            Route::get("/test"),
            Meta::default(),
        );

        let input = DynInput::from_value(serde_json::json!({"value": 21}));
        let mut stream = proc.exec((), input);
        let result = stream.next().await.unwrap().unwrap();
        assert_eq!(result.to_value().unwrap(), serde_json::json!(42));
    }

    #[tokio::test]
    async fn panic_safety() {
        let proc = ErasedProcedure::new(
            |_ctx: (), _input: DynInput| -> ProcedureStream {
                panic!("handler exploded");
            },
            Route::default(),
            Meta::default(),
        );

        let input = DynInput::from_value(serde_json::json!(null));
        let mut stream = proc.exec((), input);
        let result = stream.next().await.unwrap();
        assert!(matches!(result, Err(ProcedureError::Unwind(_))));
    }

    #[tokio::test]
    async fn multiple_calls_arc_shared() {
        let proc = ErasedProcedure::new(
            |ctx: u32, _input: DynInput| {
                ProcedureStream::from_future(async move { Ok(DynOutput::new(ctx + 1)) })
            },
            Route::default(),
            Meta::default(),
        );

        for i in 0..3 {
            let input = DynInput::from_value(serde_json::json!(null));
            let mut stream = proc.exec(i, input);
            let result = stream.next().await.unwrap().unwrap();
            assert_eq!(result.to_value().unwrap(), serde_json::json!(i + 1));
        }
    }

    #[test]
    fn route_and_meta_accessible() {
        let proc = ErasedProcedure::new(
            |_ctx: (), _input: DynInput| ProcedureStream::error(ProcedureError::Unwind(Box::new("unused"))),
            Route::get("/users").tag("users").summary("List users"),
            Meta::default(),
        );

        assert_eq!(proc.route.method.as_deref(), Some("GET"));
        assert_eq!(proc.route.path.as_deref(), Some("/users"));
        assert_eq!(proc.route.tags, vec!["users"]);
        assert_eq!(proc.route.summary.as_deref(), Some("List users"));
    }

    #[test]
    fn with_schemas() {
        use crate::schema::NoSchema;

        let proc = ErasedProcedure::new(
            |_ctx: (), _input: DynInput| ProcedureStream::error(ProcedureError::Unwind(Box::new("unused"))),
            Route::default(),
            Meta::default(),
        )
        .with_input_schema(NoSchema)
        .with_output_schema(NoSchema);

        assert!(proc.input_schema.is_some());
        assert!(proc.output_schema.is_some());
    }

    #[test]
    fn debug_output() {
        let proc = ErasedProcedure::new(
            |_ctx: (), _input: DynInput| ProcedureStream::error(ProcedureError::Unwind(Box::new("unused"))),
            Route::get("/test"),
            Meta::default(),
        );
        let debug = format!("{proc:?}");
        assert!(debug.contains("ErasedProcedure"));
        assert!(debug.contains("route"));
    }

    #[test]
    fn erased_procedure_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<ErasedProcedure<()>>();
    }
}
