use std::fmt;

use orpc_procedure::ProcedureError;
use serde::{Deserialize, Serialize};

/// Application-level RPC error, wire-compatible with `@orpc/client`.
///
/// Serializes as:
/// ```json
/// { "code": "NOT_FOUND", "status": 404, "message": "User not found", "data": null, "defined": false }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ORPCError {
    pub code: ErrorCode,
    pub message: String,
    pub status: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    pub defined: bool,
}

impl ORPCError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        let status = code.status();
        ORPCError {
            code,
            message: message.into(),
            status,
            data: None,
            defined: false,
        }
    }

    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = Some(data);
        self
    }

    pub fn with_status(mut self, status: u16) -> Self {
        self.status = status;
        self
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::BadRequest, message)
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Unauthorized, message)
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Forbidden, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::NotFound, message)
    }

    pub fn internal_server_error(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InternalServerError, message)
    }
}

impl fmt::Display for ORPCError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

impl std::error::Error for ORPCError {}

impl From<ORPCError> for ProcedureError {
    fn from(err: ORPCError) -> Self {
        ProcedureError::Resolver(Box::new(err))
    }
}

/// Error code enum, wire-compatible with oRPC TS.
///
/// All variants serialize as plain strings: `"BAD_REQUEST"`, `"NOT_FOUND"`, `"USER_BANNED"`.
/// `Custom(String)` serializes as the raw string value directly.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ErrorCode {
    BadRequest,
    Unauthorized,
    Forbidden,
    NotFound,
    MethodNotAllowed,
    NotAcceptable,
    Timeout,
    Conflict,
    PreconditionFailed,
    PayloadTooLarge,
    UnsupportedMediaType,
    UnprocessableContent,
    TooManyRequests,
    ClientClosedRequest,
    InternalServerError,
    NotImplemented,
    BadGateway,
    ServiceUnavailable,
    GatewayTimeout,
    /// User-defined error code. Serializes as the raw string value.
    Custom(String),
}

impl ErrorCode {
    fn as_str(&self) -> &str {
        match self {
            ErrorCode::BadRequest => "BAD_REQUEST",
            ErrorCode::Unauthorized => "UNAUTHORIZED",
            ErrorCode::Forbidden => "FORBIDDEN",
            ErrorCode::NotFound => "NOT_FOUND",
            ErrorCode::MethodNotAllowed => "METHOD_NOT_ALLOWED",
            ErrorCode::NotAcceptable => "NOT_ACCEPTABLE",
            ErrorCode::Timeout => "TIMEOUT",
            ErrorCode::Conflict => "CONFLICT",
            ErrorCode::PreconditionFailed => "PRECONDITION_FAILED",
            ErrorCode::PayloadTooLarge => "PAYLOAD_TOO_LARGE",
            ErrorCode::UnsupportedMediaType => "UNSUPPORTED_MEDIA_TYPE",
            ErrorCode::UnprocessableContent => "UNPROCESSABLE_CONTENT",
            ErrorCode::TooManyRequests => "TOO_MANY_REQUESTS",
            ErrorCode::ClientClosedRequest => "CLIENT_CLOSED_REQUEST",
            ErrorCode::InternalServerError => "INTERNAL_SERVER_ERROR",
            ErrorCode::NotImplemented => "NOT_IMPLEMENTED",
            ErrorCode::BadGateway => "BAD_GATEWAY",
            ErrorCode::ServiceUnavailable => "SERVICE_UNAVAILABLE",
            ErrorCode::GatewayTimeout => "GATEWAY_TIMEOUT",
            ErrorCode::Custom(code) => code.as_str(),
        }
    }

    fn from_str(s: &str) -> Self {
        match s {
            "BAD_REQUEST" => ErrorCode::BadRequest,
            "UNAUTHORIZED" => ErrorCode::Unauthorized,
            "FORBIDDEN" => ErrorCode::Forbidden,
            "NOT_FOUND" => ErrorCode::NotFound,
            "METHOD_NOT_ALLOWED" => ErrorCode::MethodNotAllowed,
            "NOT_ACCEPTABLE" => ErrorCode::NotAcceptable,
            "TIMEOUT" => ErrorCode::Timeout,
            "CONFLICT" => ErrorCode::Conflict,
            "PRECONDITION_FAILED" => ErrorCode::PreconditionFailed,
            "PAYLOAD_TOO_LARGE" => ErrorCode::PayloadTooLarge,
            "UNSUPPORTED_MEDIA_TYPE" => ErrorCode::UnsupportedMediaType,
            "UNPROCESSABLE_CONTENT" => ErrorCode::UnprocessableContent,
            "TOO_MANY_REQUESTS" => ErrorCode::TooManyRequests,
            "CLIENT_CLOSED_REQUEST" => ErrorCode::ClientClosedRequest,
            "INTERNAL_SERVER_ERROR" => ErrorCode::InternalServerError,
            "NOT_IMPLEMENTED" => ErrorCode::NotImplemented,
            "BAD_GATEWAY" => ErrorCode::BadGateway,
            "SERVICE_UNAVAILABLE" => ErrorCode::ServiceUnavailable,
            "GATEWAY_TIMEOUT" => ErrorCode::GatewayTimeout,
            other => ErrorCode::Custom(other.to_string()),
        }
    }
}

impl Serialize for ErrorCode {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ErrorCode {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Ok(ErrorCode::from_str(&s))
    }
}

impl ErrorCode {
    /// Map error code to default HTTP status code.
    pub fn status(&self) -> u16 {
        match self {
            ErrorCode::BadRequest => 400,
            ErrorCode::Unauthorized => 401,
            ErrorCode::Forbidden => 403,
            ErrorCode::NotFound => 404,
            ErrorCode::MethodNotAllowed => 405,
            ErrorCode::NotAcceptable => 406,
            ErrorCode::Timeout => 408,
            ErrorCode::Conflict => 409,
            ErrorCode::PreconditionFailed => 412,
            ErrorCode::PayloadTooLarge => 413,
            ErrorCode::UnsupportedMediaType => 415,
            ErrorCode::UnprocessableContent => 422,
            ErrorCode::TooManyRequests => 429,
            ErrorCode::ClientClosedRequest => 499,
            ErrorCode::InternalServerError => 500,
            ErrorCode::NotImplemented => 501,
            ErrorCode::BadGateway => 502,
            ErrorCode::ServiceUnavailable => 503,
            ErrorCode::GatewayTimeout => 504,
            ErrorCode::Custom(_) => 500,
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_code_serialization() {
        assert_eq!(
            serde_json::to_string(&ErrorCode::BadRequest).unwrap(),
            "\"BAD_REQUEST\""
        );
        assert_eq!(
            serde_json::to_string(&ErrorCode::NotFound).unwrap(),
            "\"NOT_FOUND\""
        );
        assert_eq!(
            serde_json::to_string(&ErrorCode::InternalServerError).unwrap(),
            "\"INTERNAL_SERVER_ERROR\""
        );
    }

    #[test]
    fn error_code_deserialization() {
        let code: ErrorCode = serde_json::from_str("\"UNAUTHORIZED\"").unwrap();
        assert_eq!(code, ErrorCode::Unauthorized);
    }

    #[test]
    fn error_code_custom() {
        let code = ErrorCode::Custom("USER_BANNED".into());
        assert_eq!(code.status(), 500);
        assert_eq!(code.to_string(), "USER_BANNED");
    }

    #[test]
    fn error_code_status_mapping() {
        assert_eq!(ErrorCode::BadRequest.status(), 400);
        assert_eq!(ErrorCode::NotFound.status(), 404);
        assert_eq!(ErrorCode::TooManyRequests.status(), 429);
        assert_eq!(ErrorCode::GatewayTimeout.status(), 504);
    }

    #[test]
    fn orpc_error_serialization() {
        let err =
            ORPCError::not_found("User not found").with_data(serde_json::json!({"userId": "123"}));
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["code"], "NOT_FOUND");
        assert_eq!(json["status"], 404);
        assert_eq!(json["message"], "User not found");
        assert_eq!(json["data"]["userId"], "123");
        assert_eq!(json["defined"], false);
    }

    #[test]
    fn orpc_error_display() {
        let err = ORPCError::unauthorized("Missing token");
        assert_eq!(err.to_string(), "[UNAUTHORIZED] Missing token");
    }

    #[test]
    fn orpc_error_into_procedure_error() {
        let err = ORPCError::not_found("gone");
        let proc_err: ProcedureError = err.into();
        assert!(matches!(proc_err, ProcedureError::Resolver(_)));
    }

    #[test]
    fn orpc_error_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<ORPCError>();
    }
}
