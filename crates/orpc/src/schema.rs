use std::marker::PhantomData;

use orpc_procedure::ErasedSchema;
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::error::ORPCError;

/// Unified Schema abstraction, counterpart to oRPC's Standard Schema.
///
/// Provides validation and JSON Schema generation for procedure input/output types.
pub trait Schema: Send + Sync + 'static {
    type Input: DeserializeOwned + Send;
    type Output: Serialize + Send;

    /// Validate and transform input.
    fn validate(&self, input: Self::Input) -> Result<Self::Output, ORPCError>;

    /// Generate JSON Schema representation (for OpenAPI generation).
    fn json_schema(&self) -> serde_json::Value;
}

/// No-validation pass-through schema. Counterpart to oRPC's `type<T>()`.
///
/// Input passes through unchanged — no validation, no transformation.
pub struct Identity<T>(PhantomData<T>);

impl<T> Identity<T> {
    pub fn new() -> Self {
        Identity(PhantomData)
    }
}

impl<T> Default for Identity<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: DeserializeOwned + Serialize + Send + Sync + 'static> Schema for Identity<T> {
    type Input = T;
    type Output = T;

    fn validate(&self, input: T) -> Result<T, ORPCError> {
        Ok(input)
    }

    fn json_schema(&self) -> serde_json::Value {
        serde_json::json!({})
    }
}

/// Adapter: wraps a typed `Schema` into a type-erased `ErasedSchema` for storage
/// in `ErasedProcedure`.
pub(crate) struct SchemaAdapter<S: Schema>(pub S);

impl<S: Schema> ErasedSchema for SchemaAdapter<S> {
    fn json_schema(&self) -> serde_json::Value {
        self.0.json_schema()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_passthrough() {
        let schema = Identity::<String>::new();
        let result = schema.validate("hello".to_string());
        assert_eq!(result.unwrap(), "hello");
    }

    #[test]
    fn identity_json_schema() {
        let schema = Identity::<u32>::new();
        assert_eq!(schema.json_schema(), serde_json::json!({}));
    }

    #[test]
    fn schema_adapter_erased() {
        let schema = Identity::<u32>::new();
        let erased: Box<dyn ErasedSchema> = Box::new(SchemaAdapter(schema));
        assert_eq!(erased.json_schema(), serde_json::json!({}));
    }
}
