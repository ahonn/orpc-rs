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

pub use builder::{os, Builder, BuilderWithIO, BuilderWithInput};
pub use context::Context;
pub use contract::{
    oc, ContractBuilder, ContractBuilderWithOutput, ContractProcedure, ErasedContract,
};
pub use error::{ErrorCode, ORPCError};
pub use handler::{BoxFuture, Handler};
pub use implement::{implement, ContractImplementer, ContractImplementerWithMw};
pub use middleware::{middleware_fn, MiddlewareCtx, MiddlewareOutput, ProcedureMeta};
pub use procedure::Procedure;
pub use router::Router;
pub use schema::{Identity, Schema};

// Re-exports from orpc-procedure
pub use orpc_procedure::{
    DynInput, DynOutput, ErasedProcedure, ErasedSchema, ErrorMap, HttpMethod, Meta,
    ProcedureError, ProcedureStream, Route, State,
};
