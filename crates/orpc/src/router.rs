use std::collections::HashMap;

use orpc_procedure::ErasedProcedure;

/// Collection of type-erased procedures, keyed by name.
///
/// All procedures share the same `TCtx` (base context type).
pub struct Router<TCtx> {
    procedures: HashMap<String, ErasedProcedure<TCtx>>,
}

impl<TCtx> Router<TCtx> {
    pub fn new() -> Self {
        Router {
            procedures: HashMap::new(),
        }
    }

    /// Add a procedure. The value can be a `Procedure` (auto-converts via `Into<ErasedProcedure>`).
    pub fn procedure(mut self, key: impl Into<String>, proc: impl Into<ErasedProcedure<TCtx>>) -> Self {
        self.procedures.insert(key.into(), proc.into());
        self
    }

    /// Nest a sub-router under a prefix. Keys become `prefix.key`.
    pub fn nest(mut self, prefix: &str, router: Router<TCtx>) -> Self {
        for (key, proc) in router.procedures {
            self.procedures.insert(format!("{prefix}.{key}"), proc);
        }
        self
    }

    /// Look up a procedure by key.
    pub fn get(&self, key: &str) -> Option<&ErasedProcedure<TCtx>> {
        self.procedures.get(key)
    }

    /// Get all procedures.
    pub fn procedures(&self) -> &HashMap<String, ErasedProcedure<TCtx>> {
        &self.procedures
    }

    /// Get the number of procedures.
    pub fn len(&self) -> usize {
        self.procedures.len()
    }

    /// Check if the router is empty.
    pub fn is_empty(&self) -> bool {
        self.procedures.is_empty()
    }
}

impl<TCtx> Default for Router<TCtx> {
    fn default() -> Self {
        Self::new()
    }
}

/// Declarative macro for building routers.
///
/// ```ignore
/// let router = router! {
///     "ping" : ping_procedure,
///     "pong" : pong_procedure,
/// };
/// ```
///
/// For nested routing, use the `.nest()` method:
/// ```ignore
/// let planet_router = router! { "list" : list, "find" : find };
/// let router = router! { "ping" : ping }.nest("planet", planet_router);
/// ```
#[macro_export]
macro_rules! router {
    ($($key:literal : $proc:expr),* $(,)?) => {{
        $crate::Router::new()
        $(
            .procedure($key, $proc)
        )*
    }};
}

#[cfg(test)]
mod tests {
    use super::*;
    use orpc_procedure::{Meta, ProcedureStream, Route};

    fn dummy_procedure() -> ErasedProcedure<()> {
        ErasedProcedure::new(
            |_ctx, _input| ProcedureStream::from_future(async { Ok(orpc_procedure::DynOutput::new("ok")) }),
            Route::default(),
            Meta::default(),
        )
    }

    #[test]
    fn router_basic() {
        let router = Router::new()
            .procedure("ping", dummy_procedure())
            .procedure("pong", dummy_procedure());

        assert_eq!(router.len(), 2);
        assert!(router.get("ping").is_some());
        assert!(router.get("pong").is_some());
        assert!(router.get("missing").is_none());
    }

    #[test]
    fn router_nest() {
        let inner = Router::new()
            .procedure("list", dummy_procedure())
            .procedure("find", dummy_procedure());

        let router = Router::new()
            .procedure("ping", dummy_procedure())
            .nest("planet", inner);

        assert_eq!(router.len(), 3);
        assert!(router.get("ping").is_some());
        assert!(router.get("planet.list").is_some());
        assert!(router.get("planet.find").is_some());
    }

    #[test]
    fn router_macro_simple() {
        let r: Router<()> = router! {
            "ping" : dummy_procedure(),
            "pong" : dummy_procedure(),
        };
        assert_eq!(r.len(), 2);
    }

    #[test]
    fn router_macro_with_nest() {
        let inner: Router<()> = router! {
            "list" : dummy_procedure(),
            "find" : dummy_procedure(),
        };
        let r = router! { "ping" : dummy_procedure() }.nest("planet", inner);
        assert_eq!(r.len(), 3);
        assert!(r.get("planet.list").is_some());
    }

    #[test]
    fn router_empty() {
        let r: Router<()> = Router::new();
        assert!(r.is_empty());
    }
}
