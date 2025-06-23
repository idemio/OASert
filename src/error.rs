use crate::traverser::TraverserError;
use crate::types::primitive::OpenApiPrimitives;
use crate::types::version::VersionError;
use jsonschema::{ReferencingError, ValidationError as JsonSchemaValidationError};
use serde_json::Value;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone)]
pub enum Section {
    Specification(SpecificationSection),
    Payload(PayloadSection),
}

impl Display for Section {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Section::Specification(spec) => write!(f, "Specification --> {}", spec),
            Section::Payload(payload) => write!(f, "Payload --> {}", payload),
        }
    }
}

#[derive(Debug, Clone)]
pub enum PayloadSection {
    Body,
    Header,
    Query,
    Path,
    Security,
    Other,
}

impl Display for PayloadSection {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            PayloadSection::Body => write!(f, "body"),
            PayloadSection::Header => write!(f, "header"),
            PayloadSection::Query => write!(f, "query"),
            PayloadSection::Path => write!(f, "path"),
            PayloadSection::Security => write!(f, "security"),
            PayloadSection::Other => write!(f, "other"),
        }
    }
}

#[derive(Debug, Clone)]
pub enum SpecificationSection {
    Paths(OperationSection),
    Components(ComponentSection),
    Security,
    Other,
}

impl Display for SpecificationSection {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            SpecificationSection::Paths(operation) => write!(f, "paths --> {}", operation),
            SpecificationSection::Components(component) => {
                write!(f, "components --> {}", component)
            }
            SpecificationSection::Security => write!(f, "security"),
            SpecificationSection::Other => write!(f, "other"),
        }
    }
}

#[derive(Debug, Clone)]
pub enum ComponentSection {
    Schemas,
    Parameters,
    Responses,
}

impl Display for ComponentSection {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ComponentSection::Schemas => write!(f, "schemas"),
            ComponentSection::Parameters => write!(f, "parameters"),
            ComponentSection::Responses => write!(f, "responses"),
        }
    }
}

#[derive(Debug, Clone)]
pub enum OperationSection {
    Parameters,
    RequestBody,
    Responses,
    Security,
    Other,
}

impl Display for OperationSection {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            OperationSection::Parameters => write!(f, "parameters"),
            OperationSection::RequestBody => write!(f, "request body"),
            OperationSection::Responses => write!(f, "responses"),
            OperationSection::Security => write!(f, "security"),
            OperationSection::Other => write!(f, "other"),
        }
    }
}

#[derive(Debug)]
pub enum ValidationErrorType {
    SchemaValidationFailed(String, String),
    TraversalFailed(String, String),
    AssertionFailed(String),
    LoadingResourceFailed(String, String),
    VersionFailed(String, String),
    ValueExpected(String, Section),
    SectionExpected(Section),
    FieldExpected(String, Section),

    UnexpectedType {
        expected: OpenApiPrimitives,
        found: Value,
        section: Section,
    },
    UnableToParse(String, Section),
    CircularReference(String, Section),
    InvalidRef(String, Section),
}

impl ValidationErrorType {
    pub(crate) fn traversal_failed<T>(traversal_error: TraverserError, message: &T) -> Self
    where
        T: ToString + ?Sized,
    {
        ValidationErrorType::TraversalFailed(traversal_error.to_string(), message.to_string())
    }

    pub(crate) fn schema_validation_failed<T>(
        json_schema_error: JsonSchemaValidationError,
        message: &T,
    ) -> Self
    where
        T: ToString + ?Sized,
    {
        ValidationErrorType::SchemaValidationFailed(
            json_schema_error.to_string(),
            message.to_string(),
        )
    }

    pub(crate) fn resource_load_error<T>(error: ReferencingError, message: &T) -> Self
    where
        T: ToString + ?Sized,
    {
        ValidationErrorType::LoadingResourceFailed(error.to_string(), message.to_string())
    }

    pub(crate) fn assertion_failed<T>(message: &T) -> Self
    where
        T: ToString + ?Sized,
    {
        ValidationErrorType::AssertionFailed(message.to_string())
    }

    pub(crate) fn version_failed<T>(version_error: VersionError, message: &T) -> Self
    where
        T: ToString + ?Sized,
    {
        ValidationErrorType::VersionFailed(version_error.to_string(), message.to_string())
    }
}

impl Display for ValidationErrorType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationErrorType::LoadingResourceFailed(resource_error, msg) => {
                write!(
                    f,
                    "Loading resource failed for {} with error: {}",
                    resource_error, msg
                )
            }
            ValidationErrorType::AssertionFailed(msg) => write!(f, "Assertion failed: {}", msg),
            ValidationErrorType::TraversalFailed(msg, operation_id) => {
                write!(
                    f,
                    "Traversal failed for operation {} with error: {}",
                    operation_id, msg
                )
            }
            ValidationErrorType::SchemaValidationFailed(validation_error, msg) => {
                write!(f, "Schema Validation Failed: {} {}", msg, validation_error)
            }
            ValidationErrorType::SectionExpected(section) => {
                write!(f, "Section {} expected", section)
            }
            ValidationErrorType::FieldExpected(field, section) => {
                write!(f, "Field {} expected in {}", field, section)
            }
            ValidationErrorType::ValueExpected(msg, section) => {
                write!(f, "Value expected {} in {}", msg, section)
            }
            ValidationErrorType::VersionFailed(version_error, msg) => {
                write!(f, "Version Failed: {} {}", msg, version_error)
            }
            ValidationErrorType::UnableToParse(msg, section) => {
                write!(f, "Unable to parse {} in {}", msg, section)
            }
            ValidationErrorType::UnexpectedType {
                expected,
                found,
                section,
            } => {
                write!(
                    f,
                    "Expected {} but found {} in {}",
                    expected, found, section
                )
            }
            ValidationErrorType::CircularReference(msg, section) => {
                write!(f, "Circular reference {} in {}", msg, section)
            }
            ValidationErrorType::InvalidRef(msg, section) => {
                write!(f, "Invalid ref {} in {}", msg, section)
            }
        }
    }
}

impl PartialEq for ValidationErrorType {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                ValidationErrorType::UnexpectedType { .. },
                ValidationErrorType::UnexpectedType { .. },
            ) => true,
            (
                ValidationErrorType::FieldExpected(_, _),
                ValidationErrorType::FieldExpected(_, _),
            ) => true,
            (
                ValidationErrorType::SchemaValidationFailed(_, _),
                ValidationErrorType::SchemaValidationFailed(_, _),
            ) => true,
            (
                ValidationErrorType::ValueExpected(_, _),
                ValidationErrorType::ValueExpected(_, _),
            ) => true,
            (ValidationErrorType::SectionExpected(_), ValidationErrorType::SectionExpected(_)) => {
                true
            }
            (
                ValidationErrorType::CircularReference(_, _),
                ValidationErrorType::CircularReference(_, _),
            ) => true,
            (ValidationErrorType::InvalidRef(_, _), ValidationErrorType::InvalidRef(_, _)) => true,
            (
                ValidationErrorType::VersionFailed(_, _),
                ValidationErrorType::VersionFailed(_, _),
            ) => true,
            (_, _) => false,
        }
    }
}

impl std::error::Error for ValidationErrorType {}
