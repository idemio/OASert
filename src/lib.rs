pub mod traverser;
pub mod types;

use crate::traverser::{OpenApiTraverser, SearchResult};
use crate::types::Operation;
use jsonschema::{Draft, Resource, ValidationOptions, Validator};
use serde_json::{Value, json};
use std::cmp::PartialEq;
use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::str::FromStr;
use std::sync::Arc;
use unicase::UniCase;

const CONTENT_FIELD: &'static str = "content";
const SCHEMA_FIELD: &'static str = "schema";
const REQUEST_BODY_FIELD: &'static str = "requestBody";
const PATHS_FIELD: &'static str = "paths";
const PARAMETERS_FIELD: &'static str = "parameters";
const REF_FIELD: &'static str = "$ref";
const SECURITY_FIELD: &'static str = "security";
const PATH_SEPARATOR: &'static str = "/";
const TILDE: &'static str = "~";
const ENCODED_BACKSLASH: &'static str = "~1";
const ENCODED_TILDE: &'static str = "~0";
const NAME_FIELD: &'static str = "name";
const OPENAPI_FIELD: &'static str = "openapi";
const REQUIRED_FIELD: &'static str = "required";
const IN_FIELD: &'static str = "in";

pub enum OpenApiVersion {
    V30x,
    V31x,
}

impl FromStr for OpenApiVersion {
    type Err = ValidationError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.starts_with("3.1") {
            Ok(OpenApiVersion::V31x)
        } else if s.starts_with("3.0") {
            Ok(OpenApiVersion::V30x)
        } else {
            Err(ValidationError::UnsupportedSpecVersion(s.to_string()))
        }
    }
}

impl OpenApiVersion {
    fn get_draft(&self) -> Draft {
        match self {
            OpenApiVersion::V30x => Draft::Draft4,
            OpenApiVersion::V31x => Draft::Draft202012,
        }
    }
}

fn serde_get_type(value: &Value) -> &'static str {
    match value {
        Value::Object(_) => "object",
        Value::Array(_) => "array",
        Value::String(_) => "string",
        Value::Number(_) => "number",
        Value::Bool(_) => "boolean",
        Value::Null => "null",
    }
}

pub enum ParameterLocation {
    Header,
    Query,
    Cookie,
    Path,
}

impl Display for ParameterLocation {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let str = String::from(match self {
            ParameterLocation::Header => "header",
            ParameterLocation::Query => "query",
            ParameterLocation::Cookie => "cookie",
            ParameterLocation::Path => "path",
        });
        write!(f, "{}", str)
    }
}

pub struct OpenApiPayloadValidator {
    traverser: OpenApiTraverser,
    options: ValidationOptions,
}

impl OpenApiPayloadValidator {
    pub fn new(mut value: Value) -> Result<Self, ValidationError> {
        // Assign ID for schema validation in the future.
        value["$id"] = json!("@@root");

        // Find the version defined in the spec and get the corresponding draft for validation.
        let draft = match Self::get_version_from_spec(&value) {
            Ok(version) => version.get_draft(),
            Err(e) => return Err(e),
        };

        // Create this resource once and re-use it for multiple validation calls.
        let resource = match Resource::from_contents(value.clone()) {
            Ok(res) => res,
            Err(e) => {
                return Err(ValidationError::SchemaValidationFailed(e.to_string()));
            }
        };

        // Assign draft and provide resource
        let options = Validator::options()
            .with_draft(draft)
            .with_resource("@@inner", resource);

        Ok(Self {
            traverser: OpenApiTraverser::new(value),
            options: options,
        })
    }

    /// Extracts the OpenAPI version from a given JSON specification object.
    ///
    /// # Arguments
    ///
    /// * `specification` - A JSON value (of type `serde_json::Value`) that represents
    ///   the OpenAPI specification. The function looks for a field named `"openapi"`.
    ///
    /// # Returns
    ///
    /// * `Ok(OpenApiVersion)` - Returns the version parsed as `OpenApiVersion` if the
    ///   "openapi" field exists and contains a valid version string (`3.1.x` or `3.0.x`).
    ///
    /// * `Err(ValidationError::FieldMissing)` - Returned when the "openapi" field is
    ///   missing from the specification.
    ///
    /// * `Err(ValidationError::UnsupportedSpecVersion)` - Returned when the "openapi"
    ///   field is present but contains a string not matching `3.1.x` or `3.0.x`.
    fn get_version_from_spec(specification: &Value) -> Result<OpenApiVersion, ValidationError> {
        // Find the openapi field and grab the version. It should follow either 3.1.x or 3.0.x.
        if let Some(version) = specification
            .get(OPENAPI_FIELD)
            .and_then(|node| node.as_str())
        {
            return match OpenApiVersion::from_str(version) {
                Ok(version) => Ok(version),
                Err(e) => Err(e),
            };
        }

        Err(ValidationError::FieldMissing(
            OPENAPI_FIELD.to_string(),
            specification.clone(),
        ))
    }

    /// Validates that the required fields specified in the `body_schema` are present in the `request_body`.
    ///
    /// # Arguments
    ///
    /// * `body_schema` - A reference to a JSON value representing the schema definition for the body,
    ///   which may include a "required" array specifying mandatory fields.
    /// * `request_body` - An optional reference to a JSON value representing the request body provided by the client.
    ///   Can be `None` if no body is included in the request.
    ///
    /// # Return Values
    ///
    /// * `Ok(())` - If the `request_body` contains all required fields or no required fields are specified in `body_schema`.
    /// * `Err(ValidationError)` - If:
    ///   - `request_body` is `None` but `body_schema` specifies required fields.
    ///   - A required field specified in `body_schema` is missing in the provided `request_body`.
    ///     The error will specify which field is missing and the current content of the body (or "null").
    fn check_required_body(
        &self,
        body_schema: &Value,
        request_body: Option<&Value>,
    ) -> Result<(), ValidationError> {
        if let Ok(required_fields) = OpenApiTraverser::get_as_array(body_schema, REQUIRED_FIELD) {
            if !required_fields.is_empty() && request_body.is_none() {
                return Err(ValidationError::ValueExpected("request body".to_string()));
            }

            for required in required_fields {
                let required_field = required.as_str().unwrap();

                if request_body.is_some_and(|body| body.get(required_field).is_none()) {
                    return Err(ValidationError::RequiredPropertyMissing(
                        required_field.to_string(),
                        request_body.unwrap_or(&json!("null")).clone(),
                    ));
                }
            }
        }
        Ok(())
    }

    /// Validates a given JSON instance against a provided JSON schema.
    ///
    /// # Arguments
    ///
    /// - `schema`: A reference to a `serde_json::Value` representing the JSON schema to validate against.
    /// - `instance`: A reference to a `serde_json::Value` representing the JSON instance to be validated.
    ///
    /// # Returns
    ///
    /// - `Ok(())`: If the instance is valid according to the schema.
    /// - `Err(ValidationError::SchemaValidationFailed)`: If the instance does not comply with the schema.
    ///   The error contains a string message with details about the validation failure.
    fn simple_validation(schema: &Value, instance: &Value) -> Result<(), ValidationError> {
        if let Err(e) = jsonschema::validate(schema, instance) {
            return Err(ValidationError::SchemaValidationFailed(e.to_string()));
        }
        Ok(())
    }

    /// Validates a JSON instance against a schema generated from the given `JsonPath`.
    ///
    /// # Arguments
    ///
    /// * `json_path` - A reference to a `JsonPath` object, which represents the path used for constructing the schema reference.
    /// * `instance` - A reference to a `Value` (from `serde_json`) that represents the JSON instance to be validated.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If the JSON instance is successfully validated against the generated schema.
    /// * `Err(ValidationError::SchemaValidationFailed(String))` - If schema building or validation fails, with details in the error message.
    fn complex_validation(
        &self,
        json_path: &JsonPath,
        instance: &Value,
    ) -> Result<(), ValidationError> {
        let full_pointer_path = format!("@@root#/{}", json_path.format_path());
        let schema = json!({
            REF_FIELD: full_pointer_path
        });

        let validator = match self.options.build(&schema) {
            Ok(val) => val,
            Err(e) => {
                return Err(ValidationError::SchemaValidationFailed(e.to_string()));
            }
        };

        match validator.validate(instance) {
            Ok(_) => Ok(()),
            Err(e) => Err(ValidationError::SchemaValidationFailed(e.to_string())),
        }
    }

    /// Checks if all required parameters defined in the `param_schemas` are present
    /// in the provided `request_params` set.
    ///
    /// # Arguments
    /// - `param_schemas`: A vector of JSON schema objects, each describing a parameter's metadata,
    ///   including its name, location, and whether it is required.
    /// - `request_params`: A map of parameter names (case-insensitive) to their respective values,
    ///   representing the actual parameters received in the request.
    ///
    /// # Returns
    /// - `Ok(())`: If all the required parameters described in `param_schemas` are present in `request_params`.
    /// - `Err(ValidationError::RequiredParameterMissing)`: If a required parameter is missing from `request_params`.
    fn check_required_params(
        &self,
        param_schemas: &Vec<Value>,
        request_params: &HashMap<UniCase<String>, String>,
    ) -> Result<(), ValidationError> {
        for param in param_schemas {
            let param_name = OpenApiTraverser::get_as_str(param, NAME_FIELD)?;
            let section = OpenApiTraverser::get_as_str(param, IN_FIELD)?;

            // If required is not defined, default is false.
            let param_required = param
                .get(REQUIRED_FIELD)
                .and_then(|required| required.as_bool())
                .unwrap_or(false);

            if !request_params.contains_key(&UniCase::<String>::from(param_name)) && param_required
            {
                return Err(ValidationError::RequiredParameterMissing(
                    param_name.to_string(),
                    section.to_string(),
                ));
            }
        }

        Ok(())
    }

    /// Validates that the parameters provided in a request comply with the expected schemas.
    ///
    /// - The function iterates through a list of parameter schemas, extracting details such as
    ///   the parameter name, its location (e.g., header, query), and whether it is required.
    /// - For each parameter, it checks if the parameter exists in the `request_params` map, validates
    ///   its value against the associated schema, and throws a validation error if any required
    ///   parameter is missing or invalid according to its schema.
    ///
    /// # Arguments
    ///
    /// - `param_schemas`: A reference to a vector of JSON values representing the expected parameter schemas.
    ///   Each schema defines properties like the parameter's name, location, type, and whether it is required.
    /// - `request_params`: A reference to a case-insensitive `HashMap` containing the parameters provided
    ///   in the request. Keys represent parameter names, and values represent their associated string values.
    /// - `section`: An instance of `ParameterLocation` specifying the location of the parameters (e.g., header,
    ///   query, cookie, or path).
    ///
    /// # Returns
    ///
    /// - `Ok(())`: If all parameters are valid and comply with the schemas.
    /// - `Err(ValidationError)`: If a required parameter is missing, a parameter schema is incomplete,
    ///   or a parameter value violates its schema. Specific errors include:
    ///     - `ValidationError::FieldMissing`: If a field in the schema (e.g., `schema`) is missing.
    ///     - `ValidationError::RequiredParameterMissing`: If a required parameter is missing from the request.
    ///     - `ValidationError::SchemaValidationFailed`: If a parameter value fails schema validation.
    fn validate_params(
        &self,
        param_schemas: &Vec<Value>,
        request_params: &HashMap<UniCase<String>, String>,
        section: ParameterLocation,
    ) -> Result<(), ValidationError> {
        for param in param_schemas {
            if param.get(IN_FIELD).is_some_and(|param| {
                param
                    .as_str()
                    .is_some_and(|param| param == section.to_string())
            }) {
                let is_required = param
                    .get(REQUIRED_FIELD)
                    .and_then(|req| req.as_bool())
                    .unwrap_or(false);

                let name = OpenApiTraverser::get_as_str(param, NAME_FIELD)?;

                let schema = match param.get(SCHEMA_FIELD) {
                    Some(x) => x,
                    None => {
                        return Err(ValidationError::FieldMissing(
                            String::from(SCHEMA_FIELD),
                            param.clone(),
                        ));
                    }
                };

                if let Some(request_param_value) =
                    request_params.get(&UniCase::<String>::from(name))
                {
                    Self::simple_validation(schema, &json!(request_param_value))?
                } else if is_required {
                    return Err(ValidationError::RequiredParameterMissing(
                        name.to_string(),
                        section.to_string(),
                    ));
                }
            }
        }

        Ok(())
    }

    fn validate_query_params(
        &self,
        param_schemas: &Vec<Value>,
        query_params: &HashMap<UniCase<String>, String>,
    ) -> Result<(), ValidationError> {
        self.check_required_params(param_schemas, query_params)?;
        self.validate_params(param_schemas, query_params, ParameterLocation::Query)
    }

    fn extract_content_type(headers: &HashMap<UniCase<String>, String>) -> Option<String> {
        if let Some(content_type_header) = headers.get(&UniCase::from("content-type")) {
            if let Some(split_content_type) = content_type_header.split(";").find(|segment| {
                segment.contains("/")
                    && (segment.starts_with("application")
                        || segment.starts_with("text")
                        || segment.starts_with("xml")
                        || segment.starts_with("audio")
                        || segment.starts_with("example")
                        || segment.starts_with("font")
                        || segment.starts_with("image")
                        || segment.starts_with("model")
                        || segment.starts_with("video")
                        || segment.starts_with("multipart")
                        || segment.starts_with("message"))
            }) {
                return Some(split_content_type.to_string());
            }
        }

        None
    }

    pub fn validate_request_body(
        &self,
        operation: &Operation,
        body: Option<&Value>,
        headers: &HashMap<UniCase<String>, String>,
    ) -> Result<(), ValidationError> {
        let (operation, mut path) = (&operation.data, operation.path.clone());
        if let Some(content_type) = Self::extract_content_type(headers) {
            let request_body_schema = match self
                .traverser
                .get_optional_spec_node(&operation, REQUEST_BODY_FIELD)?
            {
                None if body.is_some() => {
                    return Err(ValidationError::DefinitionExpected(
                        "request body".to_string(),
                    ));
                }
                None => return Ok(()),
                Some(val) => val,
            };

            let content_schema = self
                .traverser
                .get_required_spec_node(request_body_schema.value(), CONTENT_FIELD)?;

            let media_type = self
                .traverser
                .get_required_spec_node(content_schema.value(), &content_type)?;

            let request_media_type_schema = self
                .traverser
                .get_required_spec_node(media_type.value(), SCHEMA_FIELD)?;

            self.check_required_body(request_media_type_schema.value(), body)?;

            if let Some(body) = body {
                path.add_segment(REQUEST_BODY_FIELD)
                    .add_segment(CONTENT_FIELD)
                    .add_segment(&content_type)
                    .add_segment(SCHEMA_FIELD);
                self.complex_validation(&path, body)?
            }
        } else if body.is_some() {
            return Err(ValidationError::RequiredParameterMissing(
                "content-type".to_string(),
                "header".to_string(),
            ));
        }
        Ok(())
    }

    fn get_parameters<'a>(
        &'a self,
        operation: &'a Operation,
    ) -> Result<Option<SearchResult<'a>>, ValidationError> {
        let operation = &operation.data;
        match self
            .traverser
            .get_optional_spec_node(operation, PARAMETERS_FIELD)
        {
            Ok(res) => Ok(res),
            Err(e) if e.kind() == ValidationErrorKind::MismatchingSchema => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn validate_request_header_params(
        &self,
        operation: &Operation,
        headers: &HashMap<UniCase<String>, String>,
    ) -> Result<(), ValidationError> {
        let parameters = self.get_parameters(operation)?;
        if let Err(e) = match parameters {
            // if we have header params in our request and the spec contains params, do the validation.
            Some(request_params) => {
                let request_param_array = OpenApiTraverser::require_array(request_params.value())?;
                self.check_required_params(request_param_array, headers)?;
                self.validate_params(request_param_array, headers, ParameterLocation::Header)
            }
            None => Ok(()),
        } {
            return Err(e);
        }
        Ok(())
    }

    pub fn validate_request_query_parameters(
        &self,
        operation: &Operation,
        query_params: &HashMap<UniCase<String>, String>,
    ) -> Result<(), ValidationError> {
        let parameters = self.get_parameters(operation)?;
        if let Err(e) = match parameters {
            Some(request_params) => {
                let request_param_array = OpenApiTraverser::require_array(request_params.value())?;
                self.validate_query_params(request_param_array, query_params)
            }
            None => Ok(()),
        } {
            return Err(e);
        }
        Ok(())
    }
}

#[derive(Ord, PartialOrd, Eq, PartialEq)]
pub enum ValidationErrorKind {
    InvalidPayload,
    InvalidSpec,
    MismatchingSchema,
}

#[derive(Debug)]
pub enum ValidationError {
    RequiredPropertyMissing(String, Value),
    RequiredParameterMissing(String, String),
    UnsupportedSpecVersion(String),
    SchemaValidationFailed(String),
    ValueExpected(String),
    DefinitionExpected(String),
    UnexpectedType(String, &'static str, Value),
    MissingOperation(String, String),
    CircularReference(usize, String),
    FieldMissing(String, Value),
    InvalidRef(String),
    InvalidType(String),
}

impl Display for ValidationError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationError::SchemaValidationFailed(_) => {
                todo!()
            }
            ValidationError::DefinitionExpected(_) => {
                todo!()
            }
            ValidationError::ValueExpected(_) => {
                todo!()
            }
            ValidationError::RequiredPropertyMissing(_, _) => {
                todo!()
            }
            ValidationError::RequiredParameterMissing(param_name, param_type) => {
                write!(
                    f,
                    "RequiredParameterMissing: Missing the {param_type} parameter {param_name}."
                )
            }
            ValidationError::UnsupportedSpecVersion(version) => {
                write!(
                    f,
                    "UnsupportedSpecVersion: Version '{version}' is not supported/valid."
                )
            }
            ValidationError::UnexpectedType(field, expected, node) => {
                let actual_type = serde_get_type(node);
                write!(
                    f,
                    "UnexpectedType: Expected '{field}' to be '{expected}' but '{actual_type}' was found"
                )
            }
            ValidationError::MissingOperation(path, method) => {
                write!(
                    f,
                    "MissingOperation: Could not find operation for path '{path}' and '{method}'"
                )
            }
            ValidationError::FieldMissing(msg, node) => {
                write!(
                    f,
                    "RequiredFieldMissing: Object {} is missing required field {}",
                    node, msg
                )
            }
            ValidationError::CircularReference(refs, ref_string) => {
                write!(
                    f,
                    "CircularReference: {ref_string} references {ref_string} at a depth of {refs}"
                )
            }
            ValidationError::InvalidRef(msg) => write!(f, "InvalidRef: {}", msg),
            ValidationError::InvalidType(msg) => write!(f, "InvalidType: {}", msg),
        }
    }
}

impl ValidationError {
    pub fn kind(&self) -> ValidationErrorKind {
        match self {
            ValidationError::ValueExpected(_)
            | ValidationError::SchemaValidationFailed(_)
            | ValidationError::RequiredParameterMissing(_, _)
            | ValidationError::MissingOperation(_, _)
            | ValidationError::RequiredPropertyMissing(_, _) => ValidationErrorKind::InvalidPayload,

            ValidationError::FieldMissing(_, _) => ValidationErrorKind::MismatchingSchema,

            ValidationError::InvalidRef(_)
            | ValidationError::UnsupportedSpecVersion(_)
            | ValidationError::InvalidType(_)
            | ValidationError::DefinitionExpected(_)
            | ValidationError::UnexpectedType(_, _, _)
            | ValidationError::CircularReference(_, _) => ValidationErrorKind::InvalidSpec,
        }
    }
}

impl std::error::Error for ValidationError {}

#[derive(Debug, Clone)]
pub struct JsonPath(pub Vec<String>);

impl JsonPath {
    fn new() -> Self {
        JsonPath(Vec::new())
    }

    fn add_segment(&mut self, segment: &str) -> &mut Self {
        if segment.contains(TILDE) || segment.contains(PATH_SEPARATOR) {
            let segment = segment
                .replace(TILDE, ENCODED_TILDE)
                .replace(PATH_SEPARATOR, ENCODED_BACKSLASH);
            self.0.push(segment);
        } else {
            self.0.push(segment.to_owned());
        }

        self
    }

    fn format_path(&self) -> String {
        self.0.join(PATH_SEPARATOR)
    }
}

#[cfg(test)]
mod test {
    use crate::types::Operation;
    use crate::{JsonPath, OpenApiPayloadValidator, ValidationError};
    use serde_json::{Value, json};
    use std::collections::HashMap;
    use std::fs;
    use std::sync::Arc;
    use unicase::UniCase;

    #[test]
    fn test_validate_request_header_params_no_parameters() {
        // Test case: No headers and the operation schema has no parameters
        let validator = OpenApiPayloadValidator::new(json!({
            "openapi": "3.1.0"
        }))
        .unwrap();
        let operation = Operation {
            data: json!({}),
            path: JsonPath::new(),
        };
        let headers: HashMap<UniCase<String>, String> = HashMap::new();

        let result = validator.validate_request_header_params(&operation, &headers);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_request_header_params_required_parameter_present() {
        // Test case: Required parameter present in headers
        let validator = OpenApiPayloadValidator::new(json!({
            "openapi": "3.1.0"
        }))
        .unwrap();
        let operation = Operation {
            data: json!({
                "parameters": [
                    {
                        "name": "Authorization",
                        "in": "header",
                        "required": true,
                        "schema": {
                            "type": "string"
                        }
                    }
                ]
            }),
            path: JsonPath::new(),
        };
        let mut headers = HashMap::new();
        headers.insert(
            UniCase::from("Authorization".to_string()),
            "Bearer token".to_string(),
        );

        let result = validator.validate_request_header_params(&operation, &headers);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_request_header_params_required_parameter_missing() {
        // Test case: Required parameter missing in headers
        let validator = OpenApiPayloadValidator::new(json!({
            "openapi": "3.1.0"
        }))
        .unwrap();
        let operation = Operation {
            data: json!({
                "parameters": [
                    {
                        "name": "Authorization",
                        "in": "header",
                        "required": true,
                        "schema": {
                            "type": "string"
                        }
                    }
                ]
            }),
            path: JsonPath::new(),
        };
        let headers: HashMap<UniCase<String>, String> = HashMap::new();

        let result = validator.validate_request_header_params(&operation, &headers);
        assert!(result.is_err());
        if let Err(ValidationError::RequiredParameterMissing(param_name, place)) = result {
            assert_eq!(param_name, "Authorization");
            assert_eq!(place, "header");
        } else {
            panic!("Expected ValidationError::RequiredParameterMissing");
        }
    }

    #[test]
    fn test_validate_request_header_params_optional_parameter_missing() {
        // Test case: Optional parameter missing in headers
        let validator = OpenApiPayloadValidator::new(json!({
            "openapi": "3.1.0"
        }))
        .unwrap();
        let operation = Operation {
            data: json!({
                "parameters": [
                    {
                        "name": "X-Optional-Header",
                        "in": "header",
                        "required": false,
                        "schema": {
                            "type": "string"
                        }
                    }
                ]
            }),
            path: JsonPath::new(),
        };
        let headers: HashMap<UniCase<String>, String> = HashMap::new();

        let result = validator.validate_request_header_params(&operation, &headers);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_request_header_params_invalid_schema_structure() {
        // Test case: Invalid schema (e.g., missing 'name' or 'in' fields in a parameter)
        let validator = OpenApiPayloadValidator::new(json!({
            "openapi": "3.1.0"
        }))
        .unwrap();

        let operation = Operation {
            data: json!({
                "parameters": [
                    {
                        "required": true,
                        "in": "header",
                        "schema": {
                            "type": "string"
                        }
                    }
                ]
            }),
            path: JsonPath::new(),
        };
        let headers: HashMap<UniCase<String>, String> = HashMap::new();

        let result = validator.validate_request_header_params(&operation, &headers);
        assert!(result.is_err());
        if let Err(ValidationError::FieldMissing(field, _)) = result {
            assert_eq!(field, "name");
        } else {
            panic!("Expected ValidationError::FieldMissing");
        }
    }

    #[test]
    fn test_validate_request_header_params_multiple_parameters() {
        // Test case: Multiple parameters with some missing and some present
        let validator = OpenApiPayloadValidator::new(json!({
            "openapi": "3.1.0"
        }))
        .unwrap();
        let operation = Operation {
            data: json!({
                "parameters": [
                    {
                        "name": "Authorization",
                        "in": "header",
                        "required": true,
                        "schema": {
                            "type": "string"
                        }
                    },
                    {
                        "name": "X-Optional-Header",
                        "in": "header",
                        "required": false,
                        "schema": {
                            "type": "string"
                        }
                    },
                    {
                        "name": "Content-Type",
                        "in": "header",
                        "required": true,
                        "schema": {
                            "type": "string"
                        }
                    }
                ]
            }),
            path: JsonPath::new(),
        };
        let mut headers = HashMap::new();
        headers.insert(
            UniCase::from("Authorization".to_string()),
            "Bearer token".to_string(),
        );
        headers.insert(
            UniCase::from("Content-Type".to_string()),
            "application/json".to_string(),
        );

        let result = validator.validate_request_header_params(&operation, &headers);
        assert!(result.is_ok());
    }

    fn create_operation(data: Value) -> Operation {
        Operation {
            data,
            path: JsonPath::new(),
        }
    }

    #[test]
    fn test_validate_request_body_no_body_no_content_type() {
        // Scenario: No body and no Content-Type header
        let validator = OpenApiPayloadValidator::new(json!({
            "openapi": "3.1.0",
        }))
        .unwrap();
        let operation = create_operation(json!({}));
        let headers: HashMap<UniCase<String>, String> = HashMap::new();
        let result = validator.validate_request_body(&operation, None, &headers);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_request_body_with_body_missing_content_type() {
        // Scenario: Body exists but Content-Type header is missing
        let validator = OpenApiPayloadValidator::new(json!({
            "openapi": "3.1.0",
        }))
        .unwrap();
        let operation = create_operation(json!({}));
        let headers: HashMap<UniCase<String>, String> = HashMap::new();
        let body = json!({});
        let result = validator.validate_request_body(&operation, Some(&body), &headers);
        assert!(result.is_err());
        if let Err(ValidationError::RequiredParameterMissing(param, source)) = result {
            assert_eq!(param, "content-type");
            assert_eq!(source, "header");
        } else {
            panic!("Expected ValidationError::RequiredParameterMissing");
        }
    }

    #[test]
    fn test_validate_request_body_no_body_with_content_type() {
        // Scenario: No body, but Content-Type header is present
        let validator = OpenApiPayloadValidator::new(json!({
            "openapi": "3.1.0",
        }))
        .unwrap();
        let operation = create_operation(json!({}));
        let mut headers = HashMap::new();
        headers.insert(
            UniCase::new("Content-Type".to_string()),
            "application/json".to_string(),
        );
        let result = validator.validate_request_body(&operation, None, &headers);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_request_body_no_request_body_schema_in_spec() {
        // Scenario: Body exists, Content-Type exists but no requestBody field in OpenAPI spec
        let validator = OpenApiPayloadValidator::new(json!({
            "openapi": "3.1.0",
        }))
        .unwrap();
        let operation = create_operation(json!({}));
        let mut headers = HashMap::new();
        headers.insert(
            UniCase::new("Content-Type".to_string()),
            "application/json".to_string(),
        );
        let body = json!({});
        let result = validator.validate_request_body(&operation, Some(&body), &headers);
        assert!(result.is_err());
        if let Err(ValidationError::DefinitionExpected(field)) = result {
            assert_eq!(field, "request body");
        } else {
            panic!("Expected ValidationError::DefinitionExpected");
        }
    }

    #[test]
    fn test_validate_request_body_body_matches_schema() {
        // Scenario: Body exists, Content-Type exists, and body matches schema
        let operation_json = json!({
            "openapi": "3.1.0",
            "requestBody": {
                "content": {
                    "application/json": {
                        "schema": {
                            "type": "object",
                        }
                    }
                }
            }
        });
        let validator = OpenApiPayloadValidator::new(operation_json.clone()).unwrap();
        let operation = create_operation(operation_json);
        let mut headers = HashMap::new();
        headers.insert(
            UniCase::new("Content-Type".to_string()),
            "application/json".to_string(),
        );
        let body = json!({});
        let result = validator.validate_request_body(&operation, Some(&body), &headers);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_request_body_body_does_not_match_schema() {
        // Scenario: Body exists, Content-Type exists, and body does not match schema
        let operation_json = json!({
            "openapi": "3.1.0",
            "requestBody": {
                "content": {
                    "application/json": {
                        "schema": {
                            "type": "object",
                            "required": ["id"]
                        }
                    }
                }
            }
        });
        let validator = OpenApiPayloadValidator::new(operation_json.clone()).unwrap();
        let operation = create_operation(operation_json);

        let mut headers = HashMap::new();
        headers.insert(
            UniCase::new("Content-Type".to_string()),
            "application/json".to_string(),
        );
        let body = json!({});
        let result = validator.validate_request_body(&operation, Some(&body), &headers);
        assert!(result.is_err());
        if let Err(ValidationError::RequiredPropertyMissing(_, _)) = result {
            // Expected error
        } else {
            panic!("Expected ValidationError::RequiredPropertyMissing");
        }
    }

    #[test]
    fn test_validate_request_body_missing_content_schema() {
        // Scenario: Content field is missing from the OpenAPI specification
        let operation_json = json!({
            "openapi": "3.1.0",
            "requestBody": {}
        });
        let validator = OpenApiPayloadValidator::new(operation_json.clone()).unwrap();
        let operation = create_operation(operation_json);
        let mut headers = HashMap::new();
        headers.insert(
            UniCase::new("Content-Type".to_string()),
            "application/json".to_string(),
        );
        let body = json!({});
        let result = validator.validate_request_body(&operation, Some(&body), &headers);
        assert!(result.is_err());
        if let Err(ValidationError::FieldMissing(field, _)) = result {
            assert_eq!(field, "content");
        } else {
            panic!("Expected ValidationError::FieldMissing");
        }
    }

    #[test]
    fn test_validate_request_body_missing_schema_field() {
        // Scenario: Schema field is missing in the Content-Type definition
        let operation_json = json!({
            "openapi": "3.1.0",
            "requestBody": {
                "content": {
                    "application/json": {}
                }
            }
        });
        let validator = OpenApiPayloadValidator::new(operation_json.clone()).unwrap();
        let operation = create_operation(operation_json);
        let mut headers = HashMap::new();
        headers.insert(
            UniCase::new("Content-Type".to_string()),
            "application/json".to_string(),
        );
        let body = json!({});
        let result = validator.validate_request_body(&operation, Some(&body), &headers);
        assert!(result.is_err());
        if let Err(ValidationError::FieldMissing(field, _)) = result {
            assert_eq!(field, "schema");
        } else {
            panic!("Expected ValidationError::FieldMissing");
        }
    }
}
