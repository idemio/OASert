pub mod traverser;
pub mod types;
mod validator;

use crate::traverser::OpenApiTraverser;
use crate::types::{
    OpenApiVersion, Operation, ParameterLocation, RequestBodyData, RequestParamData,
};
use crate::validator::{RequestBodyValidator, RequestParameterValidator, RequestScopeValidator, Validator};
use jsonschema::{Resource, ValidationOptions, Validator as JsonValidator};
use serde_json::{Value, json};
use std::cmp::PartialEq;
use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::str::FromStr;
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

pub struct OpenApiPayloadValidator {
    traverser: OpenApiTraverser,
    options: ValidationOptions,
}

impl OpenApiPayloadValidator {
    pub fn new(mut value: Value) -> Result<Self, ValidationError> {
        // Assign ID for schema validation in the future.
        value["$id"] = json!("@@root");

        // Find the version defined in the spec and get the corresponding draft for validation.
        let version = traverser::get_as_str(&value, OPENAPI_FIELD)?;
        let version = OpenApiVersion::from_str(version)?;
        let draft = version.get_draft();

        // Create this resource once and re-use it for multiple validation calls.
        let resource = match Resource::from_contents(value.clone()) {
            Ok(res) => res,
            Err(e) => {
                return Err(ValidationError::SchemaValidationFailed);
            }
        };

        // Assign draft and provide resource
        let options = JsonValidator::options()
            .with_draft(draft)
            .with_resource("@@inner", resource);

        Ok(Self {
            traverser: OpenApiTraverser::new(value),
            options,
        })
    }

    /// Extracts a valid content type from HTTP headers.
    ///
    /// # Arguments
    /// * `headers_instance` - A reference to a HashMap of HTTP headers where keys are
    ///   case-insensitive header names and values are header values.
    ///
    /// # Returns
    /// * `Some(String)` - The content type string if found and valid.
    /// * `None` - If no valid content type is found in the headers.
    fn extract_content_type(headers_instance: &HashMap<UniCase<String>, String>) -> Option<String> {
        if let Some(content_type_header) = headers_instance.get(&UniCase::from("content-type")) {
            if let Some(split_content_type) =
                content_type_header.split(";").find(|content_type_segment| {
                    content_type_segment.contains("/")
                        && (content_type_segment.starts_with("application")
                            || content_type_segment.starts_with("text")
                            || content_type_segment.starts_with("xml")
                            || content_type_segment.starts_with("audio")
                            || content_type_segment.starts_with("example")
                            || content_type_segment.starts_with("font")
                            || content_type_segment.starts_with("image")
                            || content_type_segment.starts_with("model")
                            || content_type_segment.starts_with("video")
                            || content_type_segment.starts_with("multipart")
                            || content_type_segment.starts_with("message"))
                })
            {
                return Some(split_content_type.to_string());
            }
        }
        None
    }

    /// Validates a request body against an OpenAPI operation specification.
    ///
    /// This function extracts the content type from headers, creates a RequestBodyValidator
    /// with the request body (if provided), and validates the body against the OpenAPI
    /// specification for the given operation.
    ///
    /// # Arguments
    ///
    /// * `operation` - Reference to an Operation object containing the operation definition and path
    /// * `body_instance` - Optional reference to an object implementing RequestBodyData that provides
    ///   the request body content
    /// * `headers_instance` - Reference to an object implementing RequestParamData that provides
    ///   the request headers
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If validation succeeds (body matches the specification)
    /// * `Err(ValidationError)` - If validation fails for any reason (missing required body,
    ///   schema validation failure, etc.)
    pub fn validate_request_body(
        &self,
        operation: &Operation,
        body_instance: Option<&impl RequestBodyData>,
        headers_instance: &impl RequestParamData,
    ) -> Result<(), ValidationError> {
        let headers_instance = headers_instance.get();
        let content_type = Self::extract_content_type(&headers_instance);
        let validator = match body_instance {
            None => RequestBodyValidator::new(None, content_type),
            Some(val) => RequestBodyValidator::new(Some(val.get()), content_type),
        };
        validator.validate(&self.traverser, operation, &self.options)
    }

    /// Validates HTTP request headers against the parameters defined in an OpenAPI operation.
    ///
    /// # Arguments
    /// * `operation` - The OpenAPI operation definition to validate against, containing parameter specifications
    /// * `headers` - An object implementing the RequestParamData trait that provides access to the request headers
    ///
    /// # Returns
    /// * `Ok(())` - If all header parameters are valid according to the OpenAPI specification
    /// * `Err(ValidationError)` - If validation fails, containing the specific validation error that occurred
    pub fn validate_request_header_params(
        &self,
        operation: &Operation,
        headers: &impl RequestParamData,
    ) -> Result<(), ValidationError> {
        let headers = headers.get();
        let validator = RequestParameterValidator::new(headers, ParameterLocation::Header);
        validator.validate(&self.traverser, operation, &self.options)
    }

    /// Validates query parameters against an OpenAPI operation definition.
    ///
    /// # Arguments
    ///
    /// * `operation` - The Operation object containing the OpenAPI operation definition to validate against
    /// * `query_params` - An object implementing RequestParamData, providing access to the query parameters to validate
    ///
    /// # Returns
    ///
    /// * `Result<(), ValidationError>` - Ok(()) if validation succeeds, or Err with a ValidationError if validation fails
    pub fn validate_request_query_parameters(
        &self,
        operation: &Operation,
        query_params: &impl RequestParamData,
    ) -> Result<(), ValidationError> {
        let query_params = query_params.get();
        let validator = RequestParameterValidator::new(query_params, ParameterLocation::Query);
        validator.validate(&self.traverser, operation, &self.options)
    }

    /// Validates if the provided scopes meet the security requirements of an operation.
    ///
    /// # Arguments
    /// * `operation` - Reference to the Operation being validated, containing the security definitions
    /// * `scopes` - Vector of strings representing the scopes provided in the request
    ///
    /// # Returns
    /// * `Ok(())` - If the request has at least one of the required scopes defined in the operation
    /// * `Err(ValidationError::ValueExpected)` - If the request doesn't have any of the required scopes
    pub fn validate_request_scopes(
        &self,
        operation: &Operation,
        scopes: &Vec<String>
    ) -> Result<(), ValidationError> {
        let validator = RequestScopeValidator::new(scopes);
        validator.validate(&self.traverser, operation, &self.options)
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
    RequiredPropertyMissing,
    RequiredParameterMissing,
    UnsupportedSpecVersion,
    SchemaValidationFailed,
    ValueExpected,
    DefinitionExpected,
    UnexpectedType,
    MissingOperation,
    CircularReference,
    FieldMissing,
    InvalidRef,
    InvalidType,
}

impl Display for ValidationError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationError::SchemaValidationFailed => {
                todo!()
            }
            ValidationError::DefinitionExpected => {
                todo!()
            }
            ValidationError::ValueExpected => {
                todo!()
            }
            ValidationError::RequiredPropertyMissing => {
                todo!()
            }
            ValidationError::RequiredParameterMissing => {
                todo!()
            }
            ValidationError::UnsupportedSpecVersion => {
                todo!()
            }
            ValidationError::UnexpectedType => {
                todo!()
            }
            ValidationError::MissingOperation => {
                todo!()
            }
            ValidationError::FieldMissing => {
                todo!()
            }
            ValidationError::CircularReference => {
                todo!()
            }
            ValidationError::InvalidRef => {
                todo!()
            },
            ValidationError::InvalidType => {
                todo!()
            },
        }
    }
}

impl ValidationError {
    pub fn kind(&self) -> ValidationErrorKind {
        match self {
            ValidationError::ValueExpected
            | ValidationError::SchemaValidationFailed
            | ValidationError::RequiredParameterMissing
            | ValidationError::MissingOperation
            | ValidationError::RequiredPropertyMissing => ValidationErrorKind::InvalidPayload,

            ValidationError::FieldMissing => ValidationErrorKind::MismatchingSchema,

            ValidationError::InvalidRef
            | ValidationError::UnsupportedSpecVersion
            | ValidationError::InvalidType
            | ValidationError::DefinitionExpected
            | ValidationError::UnexpectedType
            | ValidationError::CircularReference => ValidationErrorKind::InvalidSpec,
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

    fn add(&mut self, segment: &str) -> &mut Self {
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
    use crate::types::{Operation, RequestBodyData, RequestParamData};
    use crate::{JsonPath, OpenApiPayloadValidator, ValidationError};
    use serde_json::{Value, json};
    use std::collections::HashMap;
    use unicase::UniCase;

    struct TestParamStruct {
        data: HashMap<UniCase<String>, String>,
    }

    impl RequestParamData for TestParamStruct {
        fn get(&self) -> &HashMap<UniCase<String>, String> {
            &self.data
        }
    }

    struct TestBodyStruct {
        data: Value,
    }

    impl RequestBodyData for TestBodyStruct {
        fn get(&self) -> &Value {
            &self.data
        }
    }

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
        let headers = TestParamStruct {
            data: HashMap::new(),
        };

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
        let headers_struct = TestParamStruct { data: headers };

        let result = validator.validate_request_header_params(&operation, &headers_struct);
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
        let headers_struct = TestParamStruct { data: headers };

        let result = validator.validate_request_header_params(&operation, &headers_struct);
        assert!(result.is_err());
        if let Err(ValidationError::RequiredParameterMissing) = result {
            assert!(true, "Expected error")
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
        let headers_struct = TestParamStruct { data: headers };

        let result = validator.validate_request_header_params(&operation, &headers_struct);
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
        let headers_struct = TestParamStruct { data: headers };

        let result = validator.validate_request_header_params(&operation, &headers_struct);
        assert!(result.is_err());
        if let Err(ValidationError::FieldMissing) = result {
            assert!(true, "Expected error")
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
        let headers_struct = TestParamStruct { data: headers };

        let result = validator.validate_request_header_params(&operation, &headers_struct);
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
        // Scenario: No 'body' and no Content-Type header
        let validator = OpenApiPayloadValidator::new(json!({
            "openapi": "3.1.0",
        }))
        .unwrap();
        let operation = create_operation(json!({}));
        let headers: HashMap<UniCase<String>, String> = HashMap::new();
        let headers_struct = TestParamStruct { data: headers };
        let result =
            validator.validate_request_body(&operation, None::<&TestBodyStruct>, &headers_struct);
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
        let headers_struct = TestParamStruct { data: headers };
        let body = json!({});
        let body_struct = TestBodyStruct { data: body };
        let result =
            validator.validate_request_body(&operation, Some(&body_struct), &headers_struct);
        assert!(result.is_err());
        if let Err(ValidationError::DefinitionExpected) = result {
            assert!(true, "Expected error")
        } else {
            panic!("Expected ValidationError::RequiredParameterMissing");
        }
    }

    #[test]
    fn test_validate_request_body_no_body_with_content_type() {
        // Scenario: No 'body', but Content-Type 'header' is present
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
        let headers_struct = TestParamStruct { data: headers };
        let result =
            validator.validate_request_body(&operation, None::<&TestBodyStruct>, &headers_struct);
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
        let headers_struct = TestParamStruct { data: headers };

        let body = json!({});
        let body_struct = TestBodyStruct { data: body };
        let result =
            validator.validate_request_body(&operation, Some(&body_struct), &headers_struct);
        assert!(result.is_err());
        if let Err(ValidationError::DefinitionExpected) = result {
            assert!(true, "Expected error")
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
        let headers_struct = TestParamStruct { data: headers };
        let body = json!({});
        let body_struct = TestBodyStruct { data: body };
        let result =
            validator.validate_request_body(&operation, Some(&body_struct), &headers_struct);
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
        let headers_struct = TestParamStruct { data: headers };
        let body = json!({});
        let body_struct = TestBodyStruct { data: body };
        let result =
            validator.validate_request_body(&operation, Some(&body_struct), &headers_struct);
        assert!(result.is_err());
        if let Err(ValidationError::RequiredPropertyMissing) = result {
            assert!(true, "Expected error")
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
        let headers_struct = TestParamStruct { data: headers };
        let body = json!({});
        let body_struct = TestBodyStruct { data: body };
        let result =
            validator.validate_request_body(&operation, Some(&body_struct), &headers_struct);
        assert!(result.is_err());
        if let Err(ValidationError::FieldMissing) = result {
            assert!(true, "Expected error")
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
        let headers_struct = TestParamStruct { data: headers };
        let body = json!({});
        let body_struct = TestBodyStruct { data: body };
        let result =
            validator.validate_request_body(&operation, Some(&body_struct), &headers_struct);
        assert!(result.is_err());
        if let Err(ValidationError::FieldMissing) = result {
            assert!(true, "Expected error")
        } else {
            panic!("Expected ValidationError::FieldMissing");
        }
    }
}
