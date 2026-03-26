pub mod builder;
pub mod context;
pub mod error;
pub mod handler;
pub mod middleware;
pub mod procedure;
pub mod router;
pub mod schema;

pub use builder::{os, Builder, BuilderWithIO, BuilderWithInput};
pub use context::Context;
pub use error::{ErrorCode, ORPCError};
pub use handler::{BoxFuture, Handler};
pub use middleware::{MiddlewareCtx, MiddlewareOutput, ProcedureMeta};
pub use procedure::Procedure;
pub use router::Router;
pub use schema::{Identity, Schema};

// Re-exports from orpc-procedure
pub use orpc_procedure::{
    DynInput, DynOutput, ErasedProcedure, ErasedSchema, ErrorMap, Meta, ProcedureError,
    ProcedureStream, Route, State,
};
