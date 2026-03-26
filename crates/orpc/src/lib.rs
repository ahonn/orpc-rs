pub mod builder;
pub mod context;
pub mod contract;
pub mod error;
pub mod handler;
pub mod implement;
pub mod middleware;
pub mod procedure;
pub mod router;
pub mod schema;

pub use builder::{Builder, BuilderWithIO, BuilderWithInput, os};
pub use context::Context;
pub use contract::{
    ContractBuilder, ContractBuilderWithOutput, ContractProcedure, ErasedContract, oc,
};
pub use error::{ErrorCode, ORPCError};
pub use handler::{BoxFuture, Handler};
pub use implement::{ContractImplementer, ContractImplementerWithMw, implement};
pub use middleware::{MiddlewareCtx, MiddlewareOutput, ProcedureMeta, middleware_fn};
pub use procedure::Procedure;
pub use router::Router;
pub use schema::{Identity, Schema};

// Re-exports from orpc-procedure
pub use orpc_procedure::{
    DynInput, DynOutput, ErasedProcedure, ErasedSchema, ErrorMap, HttpMethod, Meta, ProcedureError,
    ProcedureStream, Route, State,
};
