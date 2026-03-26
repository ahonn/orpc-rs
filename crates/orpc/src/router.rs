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

    /// Add a procedure. Accepts `Procedure` (auto-erased) or `ErasedProcedure`.
    /// Panics if the key already exists.
    pub fn procedure(mut self, key: impl Into<String>, proc: impl Into<ErasedProcedure<TCtx>>) -> Self {
        let key = key.into();
        if self.procedures.contains_key(&key) {
            panic!("duplicate procedure key: \"{key}\"");
        }
        self.procedures.insert(key, proc.into());
        self
    }

    /// Nest a sub-router under a prefix. Keys become `prefix.key`.
    /// Panics if any nested key collides with an existing key.
    pub fn nest(mut self, prefix: &str, router: Router<TCtx>) -> Self {
        for (key, proc) in router.procedures {
            let full_key = format!("{prefix}.{key}");
            if self.procedures.contains_key(&full_key) {
                panic!("duplicate procedure key: \"{full_key}\"");
            }
            self.procedures.insert(full_key, proc);
        }
        self
    }

    /// Insert a procedure (mutation style, used by `router!` macro).
    #[doc(hidden)]
    pub fn _insert(&mut self, key: impl Into<String>, proc: impl Into<ErasedProcedure<TCtx>>) {
        let key = key.into();
        if self.procedures.contains_key(&key) {
            panic!("duplicate procedure key: \"{key}\"");
        }
        self.procedures.insert(key, proc.into());
    }

    /// Insert a nested sub-router (mutation style, used by `router!` macro).
    #[doc(hidden)]
    pub fn _insert_nest(&mut self, prefix: &str, router: Router<TCtx>) {
        for (key, proc) in router.procedures {
            let full_key = format!("{prefix}.{key}");
            if self.procedures.contains_key(&full_key) {
                panic!("duplicate procedure key: \"{full_key}\"");
            }
            self.procedures.insert(full_key, proc);
        }
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

/// Declarative macro for building routers with optional nesting.
///
/// ```ignore
/// let router = router! {
///     "ping" => ping_procedure,
///     "planet" => {
///         "list" => list_procedure,
///         "find" => find_procedure,
///     },
/// };
/// // Keys: "ping", "planet.list", "planet.find"
/// ```
#[macro_export]
macro_rules! router {
    ($($tt:tt)*) => {{
        #[allow(unused_mut)]
        let mut __r = $crate::Router::new();
        $crate::__router_items!(__r, $($tt)*);
        __r
    }};
}

/// Internal helper macro for `router!`. Not part of the public API.
#[doc(hidden)]
#[macro_export]
macro_rules! __router_items {
    // Nested: "key" => { ... }, rest
    ($r:ident, $key:literal => { $($inner:tt)* } $(, $($rest:tt)*)?) => {
        $r._insert_nest($key, $crate::router!($($inner)*));
        $($crate::__router_items!($r, $($rest)*);)?
    };
    // Flat: "key" => expr, rest
    ($r:ident, $key:literal => $proc:expr $(, $($rest:tt)*)?) => {
        $r._insert($key, $proc);
        $($crate::__router_items!($r, $($rest)*);)?
    };
    // Base cases
    ($r:ident,) => {};
    ($r:ident) => {};
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
            "ping" => dummy_procedure(),
            "pong" => dummy_procedure(),
        };
        assert_eq!(r.len(), 2);
    }

    #[test]
    fn router_macro_with_manual_nest() {
        let inner: Router<()> = router! {
            "list" => dummy_procedure(),
            "find" => dummy_procedure(),
        };
        let r = router! { "ping" => dummy_procedure() }.nest("planet", inner);
        assert_eq!(r.len(), 3);
        assert!(r.get("planet.list").is_some());
    }

    #[test]
    fn router_macro_nested_block() {
        let r: Router<()> = router! {
            "ping" => dummy_procedure(),
            "planet" => {
                "list" => dummy_procedure(),
                "find" => dummy_procedure(),
            },
        };
        assert_eq!(r.len(), 3);
        assert!(r.get("ping").is_some());
        assert!(r.get("planet.list").is_some());
        assert!(r.get("planet.find").is_some());
    }

    #[test]
    fn router_macro_deep_nested() {
        let r: Router<()> = router! {
            "api" => {
                "v1" => {
                    "users" => dummy_procedure(),
                },
                "health" => dummy_procedure(),
            },
        };
        assert_eq!(r.len(), 2);
        assert!(r.get("api.v1.users").is_some());
        assert!(r.get("api.health").is_some());
    }

    #[test]
    fn router_empty() {
        let r: Router<()> = Router::new();
        assert!(r.is_empty());
    }
}
