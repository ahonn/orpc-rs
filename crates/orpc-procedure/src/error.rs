use std::any::Any;
use std::fmt;

/// Error during input deserialization.
#[derive(Debug)]
pub struct DeserializeError(pub Box<dyn std::error::Error + Send + Sync>);

impl fmt::Display for DeserializeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for DeserializeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&*self.0)
    }
}

impl From<serde_json::Error> for DeserializeError {
    fn from(err: serde_json::Error) -> Self {
        DeserializeError(Box::new(err))
    }
}

impl From<erased_serde::Error> for DeserializeError {
    fn from(err: erased_serde::Error) -> Self {
        DeserializeError(Box::new(err))
    }
}

/// Error during output serialization.
#[derive(Debug)]
pub struct SerializeError(pub Box<dyn std::error::Error + Send + Sync>);

impl fmt::Display for SerializeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for SerializeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&*self.0)
    }
}

impl From<serde_json::Error> for SerializeError {
    fn from(err: serde_json::Error) -> Self {
        SerializeError(Box::new(err))
    }
}

/// Universal error type for the type-erased execution layer.
///
/// All errors that can occur during procedure execution are represented here.
/// Higher-level error types (e.g., ORPCError) convert into `ProcedureError`
/// via the `Resolver` variant.
#[derive(Debug)]
pub enum ProcedureError {
    /// Input deserialization failed (malformed JSON, type mismatch, etc.)
    Deserialize(DeserializeError),
    /// Output serialization failed
    Serialize(SerializeError),
    /// Handler panicked (caught by `catch_unwind`)
    Unwind(Box<dyn Any + Send>),
    /// Application-level error from user handler or middleware
    Resolver(Box<dyn std::error::Error + Send + Sync>),
}

impl fmt::Display for ProcedureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProcedureError::Deserialize(e) => write!(f, "deserialization error: {e}"),
            ProcedureError::Serialize(e) => write!(f, "serialization error: {e}"),
            ProcedureError::Unwind(_) => write!(f, "handler panicked"),
            ProcedureError::Resolver(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ProcedureError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ProcedureError::Deserialize(e) => Some(e),
            ProcedureError::Serialize(e) => Some(e),
            ProcedureError::Unwind(_) => None,
            ProcedureError::Resolver(e) => Some(&**e),
        }
    }
}

impl From<DeserializeError> for ProcedureError {
    fn from(err: DeserializeError) -> Self {
        ProcedureError::Deserialize(err)
    }
}

impl From<SerializeError> for ProcedureError {
    fn from(err: SerializeError) -> Self {
        ProcedureError::Serialize(err)
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use super::*;

    #[test]
    fn deserialize_error_display() {
        let err = DeserializeError(Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "bad input",
        )));
        assert_eq!(err.to_string(), "bad input");
    }

    #[test]
    fn serialize_error_display() {
        let err = SerializeError(Box::new(std::io::Error::other("write failed")));
        assert_eq!(err.to_string(), "write failed");
    }

    #[test]
    fn procedure_error_deserialize_variant() {
        let err = ProcedureError::Deserialize(DeserializeError(Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "bad json",
        ))));
        assert!(err.to_string().contains("deserialization error"));
        assert!(err.to_string().contains("bad json"));
        assert!(err.source().is_some());
    }

    #[test]
    fn procedure_error_serialize_variant() {
        let err = ProcedureError::Serialize(SerializeError(Box::new(std::io::Error::other(
            "encode failed",
        ))));
        assert!(err.to_string().contains("serialization error"));
        assert!(err.source().is_some());
    }

    #[test]
    fn procedure_error_unwind_variant() {
        let err = ProcedureError::Unwind(Box::new("panic message"));
        assert_eq!(err.to_string(), "handler panicked");
        assert!(err.source().is_none());
    }

    #[test]
    fn procedure_error_resolver_variant() {
        let err = ProcedureError::Resolver(Box::new(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "user not found",
        )));
        assert_eq!(err.to_string(), "user not found");
        assert!(err.source().is_some());
    }

    #[test]
    fn from_deserialize_error() {
        let de_err = DeserializeError(Box::new(std::io::Error::other("test")));
        let proc_err: ProcedureError = de_err.into();
        assert!(matches!(proc_err, ProcedureError::Deserialize(_)));
    }

    #[test]
    fn from_serialize_error() {
        let se_err = SerializeError(Box::new(std::io::Error::other("test")));
        let proc_err: ProcedureError = se_err.into();
        assert!(matches!(proc_err, ProcedureError::Serialize(_)));
    }

    #[test]
    fn deserialize_error_from_serde_json() {
        let json_err = serde_json::from_str::<String>("not json").unwrap_err();
        let de_err: DeserializeError = json_err.into();
        assert!(!de_err.to_string().is_empty());
    }

    #[test]
    fn procedure_error_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<ProcedureError>();
    }
}
