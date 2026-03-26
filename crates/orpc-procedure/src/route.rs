/// HTTP route metadata for a procedure.
///
/// Carries method, path, tags, and OpenAPI documentation fields.
/// Lives in `orpc-procedure` for now; may move to `orpc-contract` in Phase 1c.
#[derive(Debug, Clone, Default)]
pub struct Route {
    pub method: Option<String>,
    pub path: Option<String>,
    pub tags: Vec<String>,
    pub summary: Option<String>,
    pub description: Option<String>,
    pub deprecated: bool,
}

impl Route {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(path: impl Into<String>) -> Self {
        Route {
            method: Some("GET".into()),
            path: Some(path.into()),
            ..Default::default()
        }
    }

    pub fn post(path: impl Into<String>) -> Self {
        Route {
            method: Some("POST".into()),
            path: Some(path.into()),
            ..Default::default()
        }
    }

    pub fn put(path: impl Into<String>) -> Self {
        Route {
            method: Some("PUT".into()),
            path: Some(path.into()),
            ..Default::default()
        }
    }

    pub fn delete(path: impl Into<String>) -> Self {
        Route {
            method: Some("DELETE".into()),
            path: Some(path.into()),
            ..Default::default()
        }
    }

    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    pub fn summary(mut self, summary: impl Into<String>) -> Self {
        self.summary = Some(summary.into());
        self
    }

    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn deprecated(mut self) -> Self {
        self.deprecated = true;
        self
    }
}

/// Extensible procedure metadata. Empty for Phase 1a.
#[derive(Debug, Clone, Default)]
pub struct Meta {}

/// Error map stub. Full implementation in Phase 1b with Schema trait.
#[derive(Debug, Clone, Default)]
pub struct ErrorMap {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_default() {
        let route = Route::new();
        assert!(route.method.is_none());
        assert!(route.path.is_none());
        assert!(route.tags.is_empty());
        assert!(!route.deprecated);
    }

    #[test]
    fn route_get_builder() {
        let route = Route::get("/users")
            .tag("users")
            .tag("admin")
            .summary("List users")
            .description("Returns all users")
            .deprecated();

        assert_eq!(route.method.as_deref(), Some("GET"));
        assert_eq!(route.path.as_deref(), Some("/users"));
        assert_eq!(route.tags, vec!["users", "admin"]);
        assert_eq!(route.summary.as_deref(), Some("List users"));
        assert_eq!(route.description.as_deref(), Some("Returns all users"));
        assert!(route.deprecated);
    }

    #[test]
    fn route_post() {
        let route = Route::post("/users");
        assert_eq!(route.method.as_deref(), Some("POST"));
        assert_eq!(route.path.as_deref(), Some("/users"));
    }

    #[test]
    fn route_clone() {
        let route = Route::get("/test").tag("api");
        let cloned = route.clone();
        assert_eq!(cloned.path, route.path);
        assert_eq!(cloned.tags, route.tags);
    }

    #[test]
    fn meta_default() {
        let _meta = Meta::default();
    }

    #[test]
    fn error_map_default() {
        let _map = ErrorMap::default();
    }
}
