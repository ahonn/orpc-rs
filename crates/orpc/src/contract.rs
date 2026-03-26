use std::any::TypeId;
use std::marker::PhantomData;

use orpc_procedure::{ErasedSchema, ErrorMap, Meta, Route};

use crate::error::ORPCError;
use crate::schema::{Schema, SchemaAdapter};

/// Typed contract procedure. Carries `TInput`/`TOutput`/`TError` in generics
/// for compile-time enforcement when implementing via `implement()`.
///
/// Created via the `oc()` builder:
/// ```ignore
/// let contract = oc()
///     .route(Route::get("/users/{id}"))
///     .input(Identity::<GetUserInput>::new())
///     .output(Identity::<User>::new())
///     .build();
/// // Type: ContractProcedure<GetUserInput, User, ORPCError>
/// ```
pub struct ContractProcedure<TInput = (), TOutput = (), TError = ORPCError> {
    pub(crate) input_schema: Option<Box<dyn ErasedSchema>>,
    pub(crate) output_schema: Option<Box<dyn ErasedSchema>>,
    pub(crate) error_map: ErrorMap,
    pub(crate) route: Route,
    pub(crate) meta: Meta,
    pub(crate) _phantom: PhantomData<fn(TInput, TOutput, TError)>,
}

/// Create a new contract builder.
pub fn oc() -> ContractBuilder {
    ContractBuilder {
        error_map: ErrorMap::default(),
        route: Route::default(),
        meta: Meta::default(),
    }
}

/// Contract builder (no input/output set yet).
pub struct ContractBuilder {
    error_map: ErrorMap,
    route: Route,
    meta: Meta,
}

impl ContractBuilder {
    /// Set HTTP route metadata.
    pub fn route(mut self, route: Route) -> Self {
        self.route = route;
        self
    }

    /// Set output schema only (no input), transitioning to `ContractBuilderWithOutput`.
    pub fn output<S: Schema>(self, schema: S) -> ContractBuilderWithOutput<S::Output> {
        ContractBuilderWithOutput {
            error_map: self.error_map,
            route: self.route,
            meta: self.meta,
            output_schema: Box::new(SchemaAdapter(schema)),
            _phantom: PhantomData,
        }
    }

    /// Set input schema, transitioning to `ContractBuilderWithInput`.
    pub fn input<S: Schema>(self, schema: S) -> ContractBuilderWithInput<S::Output> {
        ContractBuilderWithInput {
            error_map: self.error_map,
            route: self.route,
            meta: self.meta,
            input_schema: Box::new(SchemaAdapter(schema)),
            _phantom: PhantomData,
        }
    }

    /// Build a contract with no input/output types.
    pub fn build(self) -> ContractProcedure<(), (), ORPCError> {
        ContractProcedure {
            input_schema: None,
            output_schema: None,
            error_map: self.error_map,
            route: self.route,
            meta: self.meta,
            _phantom: PhantomData,
        }
    }
}

/// Contract builder after `.output(schema)` has been called (no input).
pub struct ContractBuilderWithOutput<TOutput> {
    error_map: ErrorMap,
    route: Route,
    meta: Meta,
    output_schema: Box<dyn ErasedSchema>,
    _phantom: PhantomData<fn(TOutput)>,
}

impl<TOutput> ContractBuilderWithOutput<TOutput> {
    /// Set HTTP route metadata.
    pub fn route(mut self, route: Route) -> Self {
        self.route = route;
        self
    }

    /// Build the contract (output only, no input).
    pub fn build(self) -> ContractProcedure<(), TOutput, ORPCError> {
        ContractProcedure {
            input_schema: None,
            output_schema: Some(self.output_schema),
            error_map: self.error_map,
            route: self.route,
            meta: self.meta,
            _phantom: PhantomData,
        }
    }
}

/// Contract builder after `.input(schema)` has been called.
pub struct ContractBuilderWithInput<TInput> {
    error_map: ErrorMap,
    route: Route,
    meta: Meta,
    input_schema: Box<dyn ErasedSchema>,
    _phantom: PhantomData<fn(TInput)>,
}

impl<TInput> ContractBuilderWithInput<TInput> {
    /// Set HTTP route metadata.
    pub fn route(mut self, route: Route) -> Self {
        self.route = route;
        self
    }

    /// Set output schema, transitioning to `ContractBuilderWithIO`.
    pub fn output<S: Schema>(self, schema: S) -> ContractBuilderWithIO<TInput, S::Output> {
        ContractBuilderWithIO {
            error_map: self.error_map,
            route: self.route,
            meta: self.meta,
            input_schema: self.input_schema,
            output_schema: Box::new(SchemaAdapter(schema)),
            _phantom: PhantomData,
        }
    }

    /// Build a contract with input only.
    pub fn build(self) -> ContractProcedure<TInput, (), ORPCError> {
        ContractProcedure {
            input_schema: Some(self.input_schema),
            output_schema: None,
            error_map: self.error_map,
            route: self.route,
            meta: self.meta,
            _phantom: PhantomData,
        }
    }
}

/// Contract builder after both `.input()` and `.output()` have been called.
pub struct ContractBuilderWithIO<TInput, TOutput> {
    error_map: ErrorMap,
    route: Route,
    meta: Meta,
    input_schema: Box<dyn ErasedSchema>,
    output_schema: Box<dyn ErasedSchema>,
    _phantom: PhantomData<fn(TInput, TOutput)>,
}

impl<TInput, TOutput> ContractBuilderWithIO<TInput, TOutput> {
    /// Set HTTP route metadata.
    pub fn route(mut self, route: Route) -> Self {
        self.route = route;
        self
    }

    /// Build the contract.
    pub fn build(self) -> ContractProcedure<TInput, TOutput, ORPCError> {
        ContractProcedure {
            input_schema: Some(self.input_schema),
            output_schema: Some(self.output_schema),
            error_map: self.error_map,
            route: self.route,
            meta: self.meta,
            _phantom: PhantomData,
        }
    }
}

/// Type-erased contract for heterogeneous storage in `ContractRouter`.
///
/// Retains `TypeId` for runtime debug assertions (primary enforcement is compile-time).
pub struct ErasedContract {
    pub input_schema: Option<Box<dyn ErasedSchema>>,
    pub output_schema: Option<Box<dyn ErasedSchema>>,
    pub error_map: ErrorMap,
    pub route: Route,
    pub meta: Meta,
    pub input_type_id: TypeId,
    pub output_type_id: TypeId,
}

impl<TInput: 'static, TOutput: 'static, TError: 'static>
    From<ContractProcedure<TInput, TOutput, TError>> for ErasedContract
{
    fn from(contract: ContractProcedure<TInput, TOutput, TError>) -> Self {
        ErasedContract {
            input_schema: contract.input_schema,
            output_schema: contract.output_schema,
            error_map: contract.error_map,
            route: contract.route,
            meta: contract.meta,
            input_type_id: TypeId::of::<TInput>(),
            output_type_id: TypeId::of::<TOutput>(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::Identity;
    use serde::{Deserialize, Serialize};

    #[derive(Deserialize, Serialize)]
    struct GetUserInput {
        id: String,
    }

    #[derive(Deserialize, Serialize)]
    struct User {
        name: String,
    }

    #[test]
    fn contract_builder_full() {
        let contract = oc()
            .route(Route::get("/users/{id}"))
            .input(Identity::<GetUserInput>::new())
            .output(Identity::<User>::new())
            .build();

        assert!(contract.input_schema.is_some());
        assert!(contract.output_schema.is_some());
        assert_eq!(contract.route.method, Some(orpc_procedure::HttpMethod::Get));
        assert_eq!(contract.route.path.as_deref(), Some("/users/{id}"));
    }

    #[test]
    fn contract_builder_input_only() {
        let contract = oc()
            .input(Identity::<GetUserInput>::new())
            .build();

        assert!(contract.input_schema.is_some());
        assert!(contract.output_schema.is_none());
    }

    #[test]
    fn contract_builder_no_io() {
        let contract = oc().route(Route::post("/ping")).build();

        assert!(contract.input_schema.is_none());
        assert!(contract.output_schema.is_none());
    }

    #[test]
    fn contract_to_erased() {
        let contract = oc()
            .input(Identity::<GetUserInput>::new())
            .output(Identity::<User>::new())
            .build();

        let erased: ErasedContract = contract.into();
        assert!(erased.input_schema.is_some());
        assert!(erased.output_schema.is_some());
        assert_eq!(erased.input_type_id, TypeId::of::<GetUserInput>());
        assert_eq!(erased.output_type_id, TypeId::of::<User>());
    }

    #[test]
    fn route_can_be_set_at_any_stage() {
        // Route on ContractBuilder
        let _ = oc().route(Route::get("/a")).build();

        // Route on ContractBuilderWithInput
        let _ = oc()
            .input(Identity::<String>::new())
            .route(Route::get("/b"))
            .build();

        // Route on ContractBuilderWithIO
        let _ = oc()
            .input(Identity::<String>::new())
            .output(Identity::<String>::new())
            .route(Route::get("/c"))
            .build();
    }
}
