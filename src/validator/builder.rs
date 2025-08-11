use crate::traverser::OpenApiTraverser;
use crate::types::version::OpenApiVersion;
use crate::validator::OpenApiPayloadValidator;
use crate::OPENAPI_FIELD;
use jsonschema::{Draft, Resource, Validator as JsonValidator};
use serde_json::Value;
use std::fmt::{Display, Formatter};
use std::str::FromStr;

#[derive(Debug)]
pub enum ValidatorBuilderError {
    InvalidOption(String),
    InvalidVersion(String),
    InvalidSpecification(String),
    LoadFailure(String),
}

impl ValidatorBuilderError {
    pub fn invalid_option(msg: impl Into<String>) -> Self {
        Self::InvalidOption(msg.into())
    }

    pub fn invalid_version(msg: impl Into<String>) -> Self {
        Self::InvalidVersion(msg.into())
    }

    pub fn invalid_specification(msg: impl Into<String>) -> Self {
        Self::InvalidSpecification(msg.into())
    }

    pub fn load_failure(msg: impl Into<String>) -> Self {
        Self::LoadFailure(msg.into())
    }
}

impl Display for ValidatorBuilderError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidatorBuilderError::InvalidOption(msg) => {
                write!(f, "Invalid Option: {}", msg)
            }
            ValidatorBuilderError::InvalidVersion(msg) => {
                write!(f, "Invalid Version: {}", msg)
            }
            ValidatorBuilderError::InvalidSpecification(msg) => {
                write!(f, "Invalid Specification: {}", msg)
            }
            ValidatorBuilderError::LoadFailure(msg) => {
                write!(f, "Load Failure: {}", msg)
            }
        }
    }
}

impl std::error::Error for ValidatorBuilderError {}

enum SpecificationLoader {
    None,
    File(String),
    //    Raw(String),
    //    External(String),
}

pub struct OpenApiPayloadValidatorBuilder {
    specification_loader: SpecificationLoader,
    version: Option<OpenApiVersion>,
    root_id: Value,
}

impl OpenApiPayloadValidatorBuilder {
    pub fn new() -> Self {
        Self {
            specification_loader: SpecificationLoader::None,
            version: None,
            root_id: Value::String(String::from("@@root")),
        }
    }

    pub fn version(mut self, version: impl AsRef<str>) -> Self {
        let version = match OpenApiVersion::from_str(version.as_ref()) {
            Ok(version) => Some(version),
            Err(invalid) => None,
        };
        self.version = version;
        self
    }

    pub fn root_id(mut self, root_id: impl Into<String>) -> Self {
        let root_id = Value::String(root_id.into());
        self.root_id = root_id;
        self
    }

    pub fn load_from_file(mut self, path: impl Into<String>) -> Self {
        self.specification_loader = SpecificationLoader::File(path.into());
        self
    }

    fn resolve_draft(spec: &Value) -> Result<Draft, ValidatorBuilderError> {
        let version = match OpenApiTraverser::get_as_str(&spec, OPENAPI_FIELD) {
            Ok(version) => version,
            Err(e) => return Err(ValidatorBuilderError::invalid_specification(e.to_string())),
        };
        let version = match OpenApiVersion::from_str(version) {
            Ok(version) => version,
            Err(e) => return Err(ValidatorBuilderError::invalid_version(e.to_string())),
        };
        Ok(version.get_draft())
    }

    pub fn build(self) -> Result<OpenApiPayloadValidator, ValidatorBuilderError> {
        let mut spec = match self.specification_loader {
            SpecificationLoader::None => {
                return Err(ValidatorBuilderError::invalid_option(
                    "No specification loader provided.",
                ));
            }
            SpecificationLoader::File(path) => Self::load_file_spec(path)?,
        };

        spec["$id"] = self.root_id;
        let draft = if self.version.is_none() {
            Self::resolve_draft(&spec)?
        } else {
            self.version.unwrap().get_draft()
        };

        // Create this resource once and re-use it for multiple validation calls.
        let resource = match Resource::from_contents(spec.clone()) {
            Ok(res) => res,
            Err(e) => return Err(ValidatorBuilderError::invalid_specification(e.to_string())),
        };

        // Assign draft and provide resource
        let options = JsonValidator::options()
            .with_draft(draft)
            .with_resource("@@inner", resource);

        // Create the traverser with owned value
        let traverser = match OpenApiTraverser::new(spec) {
            Ok(traverser) => traverser,
            Err(e) => return Err(ValidatorBuilderError::invalid_specification(e.to_string())),
        };

        Ok(OpenApiPayloadValidator { traverser, options })
    }

    fn load_file_spec(path: String) -> Result<Value, ValidatorBuilderError> {
        let content = match std::fs::read_to_string(&path) {
            Ok(content) => content,
            Err(e) => return Err(ValidatorBuilderError::load_failure(e.to_string())),
        };
        let specification = match serde_json::from_str(&content) {
            Ok(specification) => specification,
            Err(e) => return Err(ValidatorBuilderError::invalid_specification(e.to_string())),
        };
        Ok(specification)
    }
}
