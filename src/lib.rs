mod request_validator;
pub mod openapi_v30x;
pub mod openapi_v31x;
pub mod openapi_util;
pub mod spec_validator;

use std::fmt::{Display, Formatter};

#[derive(Debug)]
pub enum OpenApiValidationError {
    InvalidSchema(String),
    InvalidRequest(String),
    InvalidResponse(String),
    InvalidPath(String),
    InvalidMethod(String),
    InvalidContentType(String),
    InvalidAccept(String),
    InvalidQueryParameters(String),
    InvalidHeaders(String),
}

impl Display for OpenApiValidationError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            OpenApiValidationError::InvalidSchema(msg) => write!(f, "InvalidSchema: {}", msg),
            OpenApiValidationError::InvalidRequest(msg) => write!(f, "InvalidRequest: {}", msg),
            OpenApiValidationError::InvalidResponse(msg) => write!(f, "InvalidResponse: {}", msg),
            OpenApiValidationError::InvalidPath(msg) => write!(f, "InvalidPath: {}", msg),
            OpenApiValidationError::InvalidMethod(msg) => write!(f, "InvalidMethod: {}", msg),
            OpenApiValidationError::InvalidContentType(msg) => {
                write!(f, "InvalidContentType: {}", msg)
            }
            OpenApiValidationError::InvalidAccept(msg) => write!(f, "InvalidAccept: {}", msg),
            OpenApiValidationError::InvalidQueryParameters(msg) => {
                write!(f, "InvalidQueryParameters: {}", msg)
            }
            OpenApiValidationError::InvalidHeaders(msg) => write!(f, "InvalidHeaders: {}", msg),
        }
    }
}

impl std::error::Error for OpenApiValidationError {}