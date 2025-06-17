use crate::types::primitive::OpenApiPrimitives;
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
            Section::Specification(spec) => write!(f, "specification {}", spec),
            Section::Payload(payload) => write!(f, "payload {}", payload),
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
            SpecificationSection::Paths(operation) => write!(f, "paths {}", operation),
            SpecificationSection::Components(component) => write!(f, "components {}", component),
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

//#[derive(Debug)]
//pub struct ValidationError<'a> {
//    pub instance: Cow<'a, Value>,
//    pub path: Cow<'a, JsonPath>,
//    pub section: Section,
//    pub error: ValidationErrorType,
//}
//
//impl<'a> ValidationError<'a> {
//
//    pub(crate) fn unsupported_spec_version(
//        instance: &'a Value,
//        json_path: &'a JsonPath,
//        section: &Section,
//    ) -> ValidationError<'a> {
//        Self {
//            instance: Cow::Borrowed(instance),
//            path: Cow::Borrowed(json_path),
//            section: section.clone(),
//            error: ValidationErrorType::UnsupportedSpecVersion("Unsupported spec version"),
//        }
//    }
//
//    pub(crate) fn schema_validation_failed(
//        instance: &'a Value,
//        json_path: &'a JsonPath,
//        section: &Section,
//    ) -> ValidationError<'a> {
//        Self {
//            instance: Cow::Borrowed(instance),
//            path: Cow::Borrowed(json_path),
//            section: section.clone(),
//            error: ValidationErrorType::SchemaValidationFailed("Schema validation failed"),
//        }
//    }
//
//    pub(crate) fn value_expected(
//        instance: &'a Value,
//        json_path: &'a JsonPath,
//        section: &Section,
//    ) -> ValidationError<'a> {
//        Self {
//            instance: Cow::Borrowed(instance),
//            path: Cow::Borrowed(json_path),
//            section: section.clone(),
//            error: ValidationErrorType::ValueExpected("Value expected"),
//        }
//    }
//
//    pub(crate) fn circular_reference(
//        instance: &'a Value,
//        json_path: &'a JsonPath,
//        section: &Section,
//    ) -> ValidationError<'a> {
//        Self {
//            instance: Cow::Borrowed(instance),
//            path: Cow::Borrowed(json_path),
//            section: section.clone(),
//            error: ValidationErrorType::CircularReference("Circular reference"),
//        }
//    }
//
//    pub(crate) fn invalid_ref(
//        instance: &'a Value,
//        json_path: &'a JsonPath,
//        section: &Section,
//    ) -> ValidationError<'a> {
//        Self {
//            instance: Cow::Borrowed(instance),
//            path: Cow::Borrowed(json_path),
//            section: section.clone(),
//            error: ValidationErrorType::InvalidRef("Invalid ref"),
//        }
//    }
//}
//
//impl Display for ValidationError<'_> {
//    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
//        write!(f, "{} : {}", self.error, self.instance)
//    }
//}
//
//impl std::error::Error for ValidationError<'_> {}

#[derive(Debug)]
pub enum ValidationErrorType {
    UnsupportedSpecVersion(String, Section),
    SchemaValidationFailed(String, Section),
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

impl Display for ValidationErrorType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationErrorType::SchemaValidationFailed(msg, section) => {
                write!(f, "Schema validation failed {} in {}", msg, section)
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
            ValidationErrorType::UnsupportedSpecVersion(msg, section) => {
                write!(f, "Unsupported spec version {} in {}", msg, section)
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
                ValidationErrorType::UnsupportedSpecVersion(_, _),
                ValidationErrorType::UnsupportedSpecVersion(_, _),
            ) => true,
            (_, _) => false,
        }
    }
}

impl std::error::Error for ValidationErrorType {}
