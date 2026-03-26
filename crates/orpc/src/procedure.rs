use std::marker::PhantomData;
use std::sync::Arc;

use orpc_procedure::{
    DynInput, ErasedProcedure, ErasedSchema, ErrorMap, Meta, ProcedureStream, Route,
};

/// Type-safe Procedure. Retains full generic information at compile time.
///
/// Generic parameters:
/// - `TBaseCtx`: Initial context required to call this procedure (entry point)
/// - `TInput`: Deserialized input type
/// - `TOutput`: Handler return type
/// - `TError`: Error type
///
/// The `exec` closure captures `TInput`/`TOutput`/`TError` internally —
/// deserialization and serialization happen inside.
pub struct Procedure<TBaseCtx, TInput, TOutput, TError> {
    pub(crate) exec: Arc<dyn Fn(TBaseCtx, DynInput) -> ProcedureStream + Send + Sync>,
    pub(crate) input_schema: Option<Box<dyn ErasedSchema>>,
    pub(crate) output_schema: Option<Box<dyn ErasedSchema>>,
    pub(crate) error_map: ErrorMap,
    pub(crate) route: Route,
    pub(crate) meta: Meta,
    pub(crate) _phantom: PhantomData<fn(TInput, TOutput, TError)>,
}

/// Zero-cost conversion: erase `TInput`/`TOutput`/`TError`, keep `TBaseCtx`.
///
/// The `exec` closure already has type info baked in (deserialization + serialization
/// happen inside), so this is just a field move.
impl<TBaseCtx, TInput, TOutput, TError> From<Procedure<TBaseCtx, TInput, TOutput, TError>>
    for ErasedProcedure<TBaseCtx>
where
    TBaseCtx: Send + Sync + 'static,
{
    fn from(proc: Procedure<TBaseCtx, TInput, TOutput, TError>) -> Self {
        ErasedProcedure::new(
            move |ctx, input| (proc.exec)(ctx, input),
            proc.route,
            proc.meta,
        )
        // Transfer schemas and error_map
        .with_error_map(proc.error_map)
        // Schemas need conditional transfer
    }
}

// A more direct conversion that preserves schemas
impl<TBaseCtx, TInput, TOutput, TError> Procedure<TBaseCtx, TInput, TOutput, TError>
where
    TBaseCtx: Send + Sync + 'static,
{
    /// Convert to type-erased procedure, preserving all metadata.
    pub fn into_erased(self) -> ErasedProcedure<TBaseCtx> {
        let exec = self.exec;
        let mut erased = ErasedProcedure::new(move |ctx, input| exec(ctx, input), self.route, self.meta)
            .with_error_map(self.error_map);
        if let Some(schema) = self.input_schema {
            erased.input_schema = Some(schema);
        }
        if let Some(schema) = self.output_schema {
            erased.output_schema = Some(schema);
        }
        erased
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn procedure_type_params_are_phantom() {
        // Verify Procedure can be created with arbitrary type params
        let _proc: Procedure<(), String, Vec<u8>, std::io::Error> = Procedure {
            exec: Arc::new(|_ctx, _input| ProcedureStream::error(
                orpc_procedure::ProcedureError::Unwind(Box::new("test")),
            )),
            input_schema: None,
            output_schema: None,
            error_map: ErrorMap::default(),
            route: Route::default(),
            meta: Meta::default(),
            _phantom: PhantomData,
        };
    }

    #[test]
    fn into_erased_preserves_route() {
        let proc: Procedure<(), (), (), std::io::Error> = Procedure {
            exec: Arc::new(|_ctx, _input| ProcedureStream::error(
                orpc_procedure::ProcedureError::Unwind(Box::new("test")),
            )),
            input_schema: None,
            output_schema: None,
            error_map: ErrorMap::default(),
            route: Route::get("/test").tag("api"),
            meta: Meta::default(),
            _phantom: PhantomData,
        };

        let erased = proc.into_erased();
        assert_eq!(erased.route.path.as_deref(), Some("/test"));
        assert_eq!(erased.route.tags, vec!["api"]);
    }
}
