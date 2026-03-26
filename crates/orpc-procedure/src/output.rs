use serde::Serialize;

use crate::error::{ProcedureError, SerializeError};

/// Type-erased output — wraps a serializable value.
///
/// The concrete output type is erased via `erased_serde::Serialize`,
/// allowing heterogeneous procedure outputs in the type-erased layer.
pub struct DynOutput(Box<dyn erased_serde::Serialize + Send>);

impl DynOutput {
    /// Create from any serializable type.
    pub fn new<T: Serialize + Send + 'static>(value: T) -> Self {
        DynOutput(Box::new(value))
    }

    /// Serialize to a JSON value.
    pub fn to_value(&self) -> Result<serde_json::Value, ProcedureError> {
        serde_json::to_value(&self.0)
            .map_err(|e| ProcedureError::Serialize(SerializeError::from(e)))
    }

    /// Serialize directly to a writer (for streaming responses).
    pub fn serialize_to<W: std::io::Write>(&self, writer: W) -> Result<(), ProcedureError> {
        serde_json::to_writer(writer, &self.0)
            .map_err(|e| ProcedureError::Serialize(SerializeError::from(e)))
    }
}

impl Serialize for DynOutput {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        erased_serde::serialize(&*self.0, serializer)
    }
}

impl std::fmt::Debug for DynOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DynOutput").finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct Planet {
        name: String,
        radius: u32,
    }

    #[test]
    fn new_and_to_value_string() {
        let output = DynOutput::new("hello".to_string());
        let value = output.to_value().unwrap();
        assert_eq!(value, serde_json::json!("hello"));
    }

    #[test]
    fn new_and_to_value_struct() {
        let output = DynOutput::new(Planet {
            name: "Earth".into(),
            radius: 6371,
        });
        let value = output.to_value().unwrap();
        assert_eq!(value, serde_json::json!({"name": "Earth", "radius": 6371}));
    }

    #[test]
    fn new_and_to_value_vec() {
        let output = DynOutput::new(vec![1, 2, 3]);
        let value = output.to_value().unwrap();
        assert_eq!(value, serde_json::json!([1, 2, 3]));
    }

    #[test]
    fn serialize_to_writer() {
        let output = DynOutput::new(42u32);
        let mut buf = Vec::new();
        output.serialize_to(&mut buf).unwrap();
        assert_eq!(std::str::from_utf8(&buf).unwrap(), "42");
    }

    #[test]
    fn serde_serialize_impl() {
        let output = DynOutput::new(Planet {
            name: "Mars".into(),
            radius: 3389,
        });
        let json = serde_json::to_string(&output).unwrap();
        let parsed: Planet = serde_json::from_str(&json).unwrap();
        assert_eq!(
            parsed,
            Planet {
                name: "Mars".into(),
                radius: 3389
            }
        );
    }

    #[test]
    fn debug_output() {
        let output = DynOutput::new(42u32);
        let debug = format!("{output:?}");
        assert!(debug.contains("DynOutput"));
    }

    #[test]
    fn dyn_output_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<DynOutput>();
    }
}
