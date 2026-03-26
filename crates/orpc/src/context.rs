/// Context is any user-defined type that is `Send + Sync + 'static`.
///
/// There is NO framework-level Context struct — users define their own.
/// Middleware transforms one context type into another at compile time:
///
/// ```ignore
/// struct AppCtx { db: DbPool, headers: HeaderMap }
/// struct AuthCtx { db: DbPool, user: User }
/// // auth middleware: AppCtx → AuthCtx
/// ```
pub trait Context: Send + Sync + 'static {}

// Blanket impl: any Send + Sync + 'static type is a valid Context.
impl<T: Send + Sync + 'static> Context for T {}
