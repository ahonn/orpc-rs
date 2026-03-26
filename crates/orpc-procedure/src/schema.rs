/// Type-erased schema interface for procedure input/output.
///
/// This is the type-erased counterpart of the typed `Schema` trait in the `orpc` crate.
/// Allows heterogeneous storage in `ErasedProcedure` while preserving schema information
/// for OpenAPI generation.
pub trait ErasedSchema: Send + Sync + 'static {
    /// Generate a JSON Schema representation.
    fn json_schema(&self) -> serde_json::Value;
}

/// No-op schema placeholder for procedures without schema validation.
pub struct NoSchema;

impl ErasedSchema for NoSchema {
    fn json_schema(&self) -> serde_json::Value {
        serde_json::Value::Null
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_schema_returns_null() {
        let schema = NoSchema;
        assert_eq!(schema.json_schema(), serde_json::Value::Null);
    }

    #[test]
    fn erased_schema_is_object_safe() {
        let _boxed: Box<dyn ErasedSchema> = Box::new(NoSchema);
    }

    #[test]
    fn erased_schema_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Box<dyn ErasedSchema>>();
    }
}
