use serde::de::DeserializeOwned;

use crate::error::{DeserializeError, ProcedureError};

/// Type-erased input — wraps raw data that can be deserialized on demand.
///
/// Used by the type-erased execution layer; the typed layer deserializes into `TInput`.
///
/// Two modes:
/// - `Deserializer`: lazy deserialization from raw bytes (HTTP body, query string, etc.)
/// - `Value`: pre-parsed JSON value (from batch requests, middleware inspection, etc.)
pub enum DynInput {
    /// Wraps a serde deserializer for lazy deserialization.
    Deserializer(Box<dyn erased_serde::Deserializer<'static> + Send>),
    /// Already-parsed JSON value.
    Value(serde_json::Value),
}

impl DynInput {
    /// Create from a JSON value.
    pub fn from_value(value: serde_json::Value) -> Self {
        DynInput::Value(value)
    }

    /// Deserialize into a concrete type. Consumes self (can only be called once).
    pub fn deserialize<T: DeserializeOwned>(self) -> Result<T, ProcedureError> {
        match self {
            DynInput::Deserializer(mut de) => erased_serde::deserialize(&mut *de)
                .map_err(|e| ProcedureError::Deserialize(DeserializeError::from(e))),
            DynInput::Value(v) => serde_json::from_value(v)
                .map_err(|e| ProcedureError::Deserialize(DeserializeError::from(e))),
        }
    }

    /// Peek at the value (only works if already materialized).
    pub fn as_value(&self) -> Option<&serde_json::Value> {
        match self {
            DynInput::Value(v) => Some(v),
            DynInput::Deserializer(_) => None,
        }
    }

    /// Materialize a `Deserializer` variant into a `Value` variant.
    ///
    /// Allows middleware to inspect input without permanently consuming it.
    /// If already a `Value`, returns self unchanged.
    pub fn materialize(self) -> Result<DynInput, ProcedureError> {
        match self {
            DynInput::Value(_) => Ok(self),
            DynInput::Deserializer(mut de) => {
                let v: serde_json::Value = erased_serde::deserialize(&mut *de)
                    .map_err(|e| ProcedureError::Deserialize(DeserializeError::from(e)))?;
                Ok(DynInput::Value(v))
            }
        }
    }
}

impl std::fmt::Debug for DynInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DynInput::Deserializer(_) => f.debug_tuple("DynInput::Deserializer").finish(),
            DynInput::Value(v) => f.debug_tuple("DynInput::Value").field(v).finish(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Deserialize, PartialEq)]
    struct TestInput {
        name: String,
        age: u32,
    }

    #[test]
    fn deserialize_from_value() {
        let input = DynInput::from_value(serde_json::json!({"name": "Alice", "age": 30}));
        let result: TestInput = input.deserialize().unwrap();
        assert_eq!(
            result,
            TestInput {
                name: "Alice".into(),
                age: 30
            }
        );
    }

    #[test]
    fn deserialize_from_value_type_mismatch() {
        let input = DynInput::from_value(serde_json::json!({"wrong": "fields"}));
        let result = input.deserialize::<TestInput>();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ProcedureError::Deserialize(_)));
    }

    #[test]
    fn as_value_on_value_variant() {
        let input = DynInput::from_value(serde_json::json!(42));
        assert_eq!(input.as_value(), Some(&serde_json::json!(42)));
    }

    #[test]
    fn materialize_value_is_noop() {
        let input = DynInput::from_value(serde_json::json!({"key": "value"}));
        let materialized = input.materialize().unwrap();
        assert_eq!(
            materialized.as_value(),
            Some(&serde_json::json!({"key": "value"}))
        );
    }

    #[test]
    fn debug_value_variant() {
        let input = DynInput::from_value(serde_json::json!(42));
        let debug = format!("{input:?}");
        assert!(debug.contains("Value"));
    }

    #[test]
    fn dyn_input_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<DynInput>();
    }
}
