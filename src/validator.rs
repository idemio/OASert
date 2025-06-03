use crate::traverser::OpenApiTraverser;
use crate::types::{OpenApiTypes, OpenApiVersion, Operation, ParameterLocation, RequestBodyData, RequestParamData};
use crate::{
    CONTENT_FIELD, ENCODED_BACKSLASH, ENCODED_TILDE, IN_FIELD, NAME_FIELD, OPENAPI_FIELD,
    PARAMETERS_FIELD, PATH_SEPARATOR, REF_FIELD, REQUEST_BODY_FIELD, REQUIRED_FIELD, SCHEMA_FIELD,
    SECURITY_FIELD, TILDE,
};
use jsonschema::{Resource, ValidationOptions, Validator as JsonValidator};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::fmt::{Display, Formatter};
use std::str::FromStr;
use unicase::UniCase;

pub struct OpenApiPayloadValidator {
    traverser: OpenApiTraverser,
    options: ValidationOptions,
}

impl OpenApiPayloadValidator {
    pub fn new(mut value: Value) -> Result<Self, ValidationError> {
        // Assign ID for schema validation in the future.
        value["$id"] = json!("@@root");

        // Find the version defined in the spec and get the corresponding draft for validation.
        let version = match value.get(OPENAPI_FIELD).and_then(|v| v.as_str()) {
            None => return Err(ValidationError::FieldMissing),
            Some(v) => v,
        };
        let version = OpenApiVersion::from_str(version)?;
        let draft = version.get_draft();

        // Create this resource once and re-use it for multiple validation calls.
        let resource = match Resource::from_contents(value.clone()) {
            Ok(res) => res,
            Err(_) => {
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
        scopes: &Vec<String>,
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
            }
            ValidationError::InvalidType => {
                todo!()
            }
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
    pub(crate) fn new() -> Self {
        JsonPath(Vec::new())
    }

    pub(crate) fn add(&mut self, segment: &str) -> &mut Self {
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

pub(crate) trait Validator {
    fn validate(
        &self,
        traverser: &OpenApiTraverser,
        operation: &Operation,
        validation_options: &ValidationOptions,
    ) -> Result<(), ValidationError>;

    /// Validates a JSON instance against a schema referenced by a JSON path.
    ///
    /// # Arguments
    ///
    /// * `options` - Contains configuration options for building and executing the validator
    /// * `json_path` - The path used to locate the schema for validation
    /// * `instance` - The JSON value to be validated against the schema
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If validation succeeds
    /// * `Err(ValidationError::SchemaValidationFailed)` - If either schema building fails, or validation fails
    fn complex_validation(
        options: &ValidationOptions,
        json_path: &JsonPath,
        instance: &Value,
    ) -> Result<(), ValidationError> {
        let full_pointer_path = format!("@@root#/{}", json_path.format_path());
        let schema = json!({
            REF_FIELD: full_pointer_path
        });

        let validator = match options.build(&schema) {
            Ok(val) => val,
            Err(e) => {
                return Err(ValidationError::SchemaValidationFailed);
            }
        };

        match validator.validate(instance) {
            Ok(_) => Ok(()),
            Err(e) => Err(ValidationError::SchemaValidationFailed),
        }
    }

    /// Validates an instance against a JSON schema.
    ///
    /// # Arguments
    ///
    /// * `schema` - A JSON schema represented as a serde_json Value
    /// * `instance` - The data to validate against the schema, represented as a serde_json Value
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If the instance successfully validates against the schema
    /// * `Err(ValidationError::SchemaValidationFailed)` - If validation fails for any reason
    fn simple_validation(schema: &Value, instance: &Value) -> Result<(), ValidationError> {
        if let Some(string) = instance.as_str() {
            let instance = OpenApiTypes::convert_string_to_schema_type(schema, string)?;
            if let Err(e) = jsonschema::validate(schema, &instance) {
                println!("Error: {e}");
                return Err(ValidationError::SchemaValidationFailed);
            }
        } else {
            if let Err(e) = jsonschema::validate(schema, instance) {
                println!("Error: {e}");
                return Err(ValidationError::SchemaValidationFailed);
            }
        }
        Ok(())
    }
}

pub(crate) struct RequestParameterValidator<'a> {
    request_instance: &'a HashMap<UniCase<String>, String>,
    parameter_location: ParameterLocation,
}

impl<'a> RequestParameterValidator<'a> {
    pub(crate) fn new<'b>(
        request_instance: &'b HashMap<UniCase<String>, String>,
        parameter_location: ParameterLocation,
    ) -> Self
    where
        'b: 'a,
    {
        Self {
            request_instance,
            parameter_location,
        }
    }
}

impl<'a> Validator for RequestParameterValidator<'a> {
    /// Validates request parameters against an OpenAPI operation definition.
    ///
    /// # Arguments
    ///
    /// * `self` - The RequestParameterValidator instance containing the request parameters and parameter location
    /// * `traverser` - An OpenApiTraverser that allows navigation through the OpenAPI specification
    /// * `operation` - The Operation object containing the OpenAPI operation definition to validate against
    /// * `_validation_options` - Validation options (unused in this implementation)
    ///
    /// # Returns
    /// * `Result<(), ValidationError>` - Ok(()) if validation succeeds, or Err with a ValidationError if validation fails
    fn validate(
        &self,
        traverser: &OpenApiTraverser,
        operation: &Operation,
        _validation_options: &ValidationOptions,
    ) -> Result<(), ValidationError> {
        let parameter_location = &self.parameter_location;
        let operation_definition = &operation.data;
        let parameter_definitions =
            match traverser.get_optional_spec_node(operation_definition, PARAMETERS_FIELD) {
                Ok(res) => Ok(res),
                Err(e) if e.kind() == ValidationErrorKind::MismatchingSchema => Ok(None),
                Err(e) => Err(e),
            }?;
        match parameter_definitions {
            Some(parameter_definitions) => {
                let parameter_definitions =
                    OpenApiTraverser::require_array(parameter_definitions.value())?;
                for parameter_definition in parameter_definitions {
                    // Only look at parameters that match the current section.
                    let location =
                        traverser.get_required_spec_node(parameter_definition, IN_FIELD)?;
                    let location = OpenApiTraverser::require_str(location.value())?;
                    if location == parameter_location.to_string() {
                        let parameter_name =
                            traverser.get_required_spec_node(parameter_definition, NAME_FIELD)?;
                        let parameter_name = OpenApiTraverser::require_str(parameter_name.value())?;
                        let is_parameter_required = traverser
                            .get_optional_spec_node(parameter_definition, REQUIRED_FIELD)?;
                        let is_parameter_required: bool = match is_parameter_required {
                            None => false,
                            Some(val) => {
                                OpenApiTraverser::require_bool(val.value()).unwrap_or(false)
                            }
                        };
                        let parameter_schema =
                            traverser.get_required_spec_node(parameter_definition, SCHEMA_FIELD)?;
                        let parameter_schema = parameter_schema.value();

                        if let Some(request_parameter_value) = self
                            .request_instance
                            .get(&UniCase::<String>::from(parameter_name))
                        {

                            Self::simple_validation(
                                parameter_schema,
                                &json!(request_parameter_value),
                            )?
                        } else if is_parameter_required {
                            return Err(ValidationError::RequiredParameterMissing);
                        }
                    }
                }
                Ok(())
            }
            None => Ok(()),
        }
    }
}

pub(crate) struct RequestBodyValidator<'a> {
    request_instance: Option<&'a Value>,
    content_type: Option<String>,
}

impl<'a> RequestBodyValidator<'a> {
    pub(crate) fn new<'b>(request_instance: Option<&'b Value>, content_type: Option<String>) -> Self
    where
        'b: 'a,
    {
        Self {
            request_instance,
            content_type,
        }
    }

    /// Validates that all required fields specified in a schema are present in the request body.
    ///
    /// # Arguments
    ///
    /// * `body_schema` - A JSON schema that may contain a "required" field listing mandatory properties
    /// * `request_body` - An optional JSON value representing the request body to validate
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If all required fields are present in the request body
    /// * `Err(ValidationError::ValueExpected)` - If required fields are specified, but the request body is missing
    /// * `Err(ValidationError::RequiredPropertyMissing)` - If any required field is missing from the request body
    /// * `Err(ValidationError::UnexpectedType)` - If a value in the required array is not a string
    /// * `Err(ValidationError::FieldMissing)` - If the required field doesn't exist in the schema
    fn check_required_body(
        traverser: &OpenApiTraverser,
        body_schema: &Value,
        request_body: Option<&Value>,
    ) -> Result<(), ValidationError> {
        if let Some(required_fields) =
            traverser.get_optional_spec_node(body_schema, REQUIRED_FIELD)?
        {
            let required_fields = OpenApiTraverser::require_array(required_fields.value())?;
            // if the body provided is empty and required fields are present, then it's an invalid body.
            if !required_fields.is_empty() && request_body.is_none() {
                return Err(ValidationError::ValueExpected);
            }

            if let Some(body) = request_body {
                for required in required_fields {
                    let required_field = OpenApiTraverser::require_str(required)?;

                    // if the current required field is not present in the body, then it's a failure.
                    if body.get(required_field).is_none() {
                        return Err(ValidationError::RequiredPropertyMissing);
                    }
                }
            }
        }
        Ok(())
    }
}

impl<'a> Validator for RequestBodyValidator<'a> {
    /// Validates the request body of an OpenAPI operation against the specification.
    ///
    /// This function checks if a request body exists when required, and if it conforms to the
    /// expected schema defined in the OpenAPI specification for the given operation and content type.
    ///
    /// # Arguments
    ///
    /// * `&self` - Reference to the RequestBodyValidator instance which contains the request body
    ///   instance and content type
    /// * `traverser` - Reference to an OpenApiTraverser used to navigate the OpenAPI specification
    /// * `operation` - Reference to the Operation object containing the operation definition and path
    /// * `validation_options` - Reference to ValidationOptions used during schema validation
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If the request body is valid or not required
    /// * `Err(ValidationError::DefinitionExpected)` - If a body instance exists but no request body
    ///   is defined in the specification
    /// * `Err(ValidationError::ValueExpected)` - If the request body is required but not provided
    /// * `Err(ValidationError::RequiredParameterMissing)` - If the content type is missing but
    ///   request body is required
    /// * `Err(ValidationError::SchemaValidationFailed)` - If the request body fails schema validation
    fn validate(
        &self,
        traverser: &OpenApiTraverser,
        operation: &Operation,
        validation_options: &ValidationOptions,
    ) -> Result<(), ValidationError> {
        let (operation_definition, mut operation_definition_path) =
            (&operation.data, operation.path.clone());
        let body_instance = self.request_instance;

        let request_body_definition =
            match traverser.get_optional_spec_node(&operation_definition, REQUEST_BODY_FIELD)? {
                None if body_instance.is_some() => {
                    return Err(ValidationError::DefinitionExpected);
                }
                None => return Ok(()),
                Some(val) => val,
            };
        
        let is_request_body_required =
            traverser.get_optional_spec_node(request_body_definition.value(), REQUIRED_FIELD)?;
        let is_request_body_required: bool = match is_request_body_required {
            None => true,
            Some(val) => val.value().as_bool().unwrap_or(true),
        };
        if let Some(content_type) = &self.content_type {
            let content_definition =
                traverser.get_required_spec_node(request_body_definition.value(), CONTENT_FIELD)?;

            let media_type_definition =
                traverser.get_required_spec_node(content_definition.value(), &content_type)?;

            let request_media_type_definition =
                traverser.get_required_spec_node(media_type_definition.value(), SCHEMA_FIELD)?;

            Self::check_required_body(
                traverser,
                request_media_type_definition.value(),
                body_instance,
            )?;
            if let Some(body_instance) = body_instance {
                operation_definition_path
                    .add(REQUEST_BODY_FIELD)
                    .add(CONTENT_FIELD)
                    .add(&content_type)
                    .add(SCHEMA_FIELD);
                Self::complex_validation(
                    &validation_options,
                    &operation_definition_path,
                    body_instance,
                )?

            // if the body does not exist, make sure 'required' is set to false.
            } else if is_request_body_required {
                return Err(ValidationError::ValueExpected);
            }
        } else if is_request_body_required {
            return Err(ValidationError::RequiredParameterMissing);
        }

        Ok(())
    }
}

pub(crate) struct RequestScopeValidator<'a> {
    request_instance: &'a Vec<String>,
}

impl<'a> RequestScopeValidator<'a> {
    pub(crate) fn new<'b>(request_instance: &'b Vec<String>) -> Self
    where
        'b: 'a,
    {
        Self { request_instance }
    }
}

impl<'a> Validator for RequestScopeValidator<'a> {
    /// Validates whether a request has at least one of the required scopes specified in the operation's security definitions.
    ///
    /// # Arguments
    /// * `&self` - Reference to the `RequestScopeValidator` containing the request's scopes
    /// * `traverser` - Reference to an `OpenApiTraverser` used to navigate the OpenAPI specification
    /// * `operation` - Reference to the `Operation` being validated
    /// * `_validation_options` - Reference to `ValidationOptions` (unused in this implementation)
    ///
    /// # Returns
    /// * `Ok(())` - If the request has at least one of the required scopes
    /// * `Err(ValidationError::ValueExpected)` - If the request doesn't have any of the required scopes
    fn validate(
        &self,
        traverser: &OpenApiTraverser,
        operation: &Operation,
        _validation_options: &ValidationOptions,
    ) -> Result<(), ValidationError> {
        let operation = &operation.data;
        let security_definitions = traverser.get_optional_spec_node(operation, SECURITY_FIELD)?;

        let request_scopes: HashSet<&str> =
            self.request_instance.iter().map(|s| s.as_str()).collect();

        if let Some(security_definitions) = security_definitions {
            // get the array of maps
            let security_definitions =
                OpenApiTraverser::require_array(security_definitions.value())?;

            for security_definition in security_definitions {
                // convert to map
                let security_definition = OpenApiTraverser::require_object(security_definition)?;

                for (_, scope_list) in security_definition {
                    // convert to list
                    let scope_list = OpenApiTraverser::require_array(scope_list)?;

                    // check to see if the scope is found in our request scopes
                    for scope in scope_list {
                        let scope = OpenApiTraverser::require_str(scope)?;
                        if request_scopes.contains(scope) {
                            return Ok(());
                        }
                    }
                }
            }
        }

        Err(ValidationError::ValueExpected)
    }
}

#[cfg(test)]
mod test {
    use crate::types::{Operation, RequestBodyData, RequestParamData};
    use crate::validator::{JsonPath, OpenApiPayloadValidator, ValidationError};
    use serde_json::{Value, json};
    use std::collections::HashMap;
    use std::fs;
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

    #[test]
    fn test_wild_openapi_spec_request_body_validation() {
        // Load the wild-openapi-spec.json file
        let spec_json = fs::read_to_string("./test/wild-openapi-spec.json").unwrap();
        let spec =
            serde_json::from_str(&spec_json).expect("Failed to parse wild-openapi-spec.json");

        // Create validator
        let validator = OpenApiPayloadValidator::new(spec).unwrap();

        // Create headers with JSON content type
        let mut headers = HashMap::new();
        headers.insert(
            UniCase::new("Content-Type".to_string()),
            "application/json".to_string(),
        );
        let headers_instance = TestParamStruct { data: headers };

        // Get the pet operation from the spec

        //"/paths/~1pet/post"
        let operation = validator.traverser.get_operation("/pet", "post").unwrap();

        // Test 1: Valid BasePet + Cat JSON body
        let valid_cat_body = TestBodyStruct {
            data: json!({
                "name": "Mittens",
                "age": 3,
                "hunts": true,
                "breed": "Siamese"
            }),
        };

        let result =
            validator.validate_request_body(&operation, Some(&valid_cat_body), &headers_instance);
        match result {
            Ok(_) => assert!(true),
            Err(e) => assert!(false, "error: {}", e.to_string()),
        }

        // Test 3: Invalid cat with wrong breed
        let invalid_cat_body = TestBodyStruct {
            data: json!({
                "name": "Whiskers",
                "age": 2,
                "hunts": true,
                "breed": "Unknown"
            }),
        };

        let result =
            validator.validate_request_body(&operation, Some(&invalid_cat_body), &headers_instance);
        assert!(
            result.is_err(),
            "Cat with invalid breed should fail validation"
        );

        // Test 6: Test with XML content type (should validate against Pet schema)
        let mut xml_headers = HashMap::new();
        xml_headers.insert(
            UniCase::new("Content-Type".to_string()),
            "application/xml".to_string(),
        );
        let xml_headers_instance = TestParamStruct { data: xml_headers };

        let valid_pet_xml_equivalent = TestBodyStruct {
            data: json!({  // We use JSON for simplicity, but specify XML content type
                "name": "Doggo",
                "photoUrls": ["http://example.com/photo.jpg"],
                "id": 123,
                "category": {
                    "id": 1,
                    "name": "Dogs"
                },
                "tags": [
                    {"id": 1, "name": "friendly"}
                ],
                "status": "available"
            }),
        };

        let result = validator.validate_request_body(
            &operation,
            Some(&valid_pet_xml_equivalent),
            &xml_headers_instance,
        );
        assert!(
            result.is_ok(),
            "Valid pet with XML content type should pass validation: {:?}",
            result
        );

        // Test 7: Invalid - missing required photoUrls in Pet schema with XML content type
        let invalid_pet_xml_equivalent = TestBodyStruct {
            data: json!({
                "name": "Doggo",
                // Missing required "photoUrls" field
                "id": 123
            }),
        };

        let result = validator.validate_request_body(
            &operation,
            Some(&invalid_pet_xml_equivalent),
            &xml_headers_instance,
        );
        assert!(
            result.is_err(),
            "Pet without required photoUrls field should fail validation with XML content type"
        );
    }

    #[test]
    fn test_validate_request_query_parameters_no_parameters() {
        // Setup
        let spec = json!({
            "openapi": "3.0.0",
            "paths": {
                "/test": {
                    "get": {
                        "parameters": []
                    }
                }
            }
        });

        let validator = OpenApiPayloadValidator::new(spec).unwrap();
        let operation = create_operation(json!({"parameters": []}));
        let query_params = TestParamStruct { data: HashMap::new() };

        // Execute
        let result = validator.validate_request_query_parameters(&operation, &query_params);

        // Verify
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_request_query_parameters_required_parameter_present() {
        // Setup
        let spec = json!({
            "openapi": "3.0.0",
            "paths": {
                "/test": {
                    "get": {
                        "parameters": [
                            {
                                "name": "id",
                                "in": "query",
                                "required": true,
                                "schema": {
                                    "type": "string"
                                }
                            }
                        ]
                    }
                }
            }
        });

        let validator = OpenApiPayloadValidator::new(spec).unwrap();
        let operation = create_operation(json!({
            "parameters": [
                {
                    "name": "id",
                    "in": "query",
                    "required": true,
                    "schema": {
                        "type": "string"
                    }
                }
            ]
        }));

        let mut params = HashMap::new();
        params.insert(UniCase::new("id".to_string()), "123".to_string());
        let query_params = TestParamStruct { data: params };

        // Execute
        let result = validator.validate_request_query_parameters(&operation, &query_params);

        // Verify
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_request_query_parameters_required_parameter_missing() {
        // Setup
        let spec = json!({
            "openapi": "3.0.0",
            "paths": {
                "/test": {
                    "get": {
                        "parameters": [
                            {
                                "name": "id",
                                "in": "query",
                                "required": true,
                                "schema": {
                                    "type": "string"
                                }
                            }
                        ]
                    }
                }
            }
        });

        let validator = OpenApiPayloadValidator::new(spec).unwrap();
        let operation = create_operation(json!({
            "parameters": [
                {
                    "name": "id",
                    "in": "query",
                    "required": true,
                    "schema": {
                        "type": "string"
                    }
                }
            ]
        }));

        let query_params = TestParamStruct { data: HashMap::new() };

        // Execute
        let result = validator.validate_request_query_parameters(&operation, &query_params);

        // Verify
        assert!(result.is_err());
        match result {
            Err(ValidationError::RequiredParameterMissing) => (),
            _ => panic!("Expected RequiredParameterMissing error"),
        }
    }

    #[test]
    fn test_validate_request_query_parameters_optional_parameter_missing() {
        // Setup
        let spec = json!({
            "openapi": "3.0.0",
            "paths": {
                "/test": {
                    "get": {
                        "parameters": [
                            {
                                "name": "filter",
                                "in": "query",
                                "required": false,
                                "schema": {
                                    "type": "string"
                                }
                            }
                        ]
                    }
                }
            }
        });

        let validator = OpenApiPayloadValidator::new(spec).unwrap();
        let operation = create_operation(json!({
            "parameters": [
                {
                    "name": "filter",
                    "in": "query",
                    "required": false,
                    "schema": {
                        "type": "string"
                    }
                }
            ]
        }));

        let query_params = TestParamStruct { data: HashMap::new() };

        // Execute
        let result = validator.validate_request_query_parameters(&operation, &query_params);

        // Verify
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_request_query_parameters_parameter_wrong_type() {
        // Setup
        let spec = json!({
            "openapi": "3.0.0",
            "paths": {
                "/test": {
                    "get": {
                        "parameters": [
                            {
                                "name": "age",
                                "in": "query",
                                "required": true,
                                "schema": {
                                    "type": "integer"
                                }
                            }
                        ]
                    }
                }
            }
        });

        let validator = OpenApiPayloadValidator::new(spec).unwrap();
        let operation = create_operation(json!({
            "parameters": [
                {
                    "name": "age",
                    "in": "query",
                    "required": true,
                    "schema": {
                        "type": "integer"
                    }
                }
            ]
        }));

        let mut params = HashMap::new();
        params.insert(UniCase::new("age".to_string()), "not-a-number".to_string());
        let query_params = TestParamStruct { data: params };

        // Execute
        let result = validator.validate_request_query_parameters(&operation, &query_params);

        // Verify
        assert!(result.is_err());
        println!("err: {:?}", result);
        match result {
            Err(ValidationError::InvalidType) => (),
            _ => panic!("Expected InvalidType error"),
        }
    }

    #[test]
    fn test_validate_request_query_parameters_multiple_parameters() {
        // Setup
        let spec = json!({
            "openapi": "3.0.0",
            "paths": {
                "/test": {
                    "get": {
                        "parameters": [
                            {
                                "name": "page",
                                "in": "query",
                                "required": true,
                                "schema": {
                                    "type": "integer"
                                }
                            },
                            {
                                "name": "limit",
                                "in": "query",
                                "required": true,
                                "schema": {
                                    "type": "integer"
                                }
                            },
                            {
                                "name": "sort",
                                "in": "query",
                                "required": false,
                                "schema": {
                                    "type": "string"
                                }
                            }
                        ]
                    }
                }
            }
        });

        let validator = OpenApiPayloadValidator::new(spec).unwrap();
        let operation = create_operation(json!({
            "parameters": [
                {
                    "name": "page",
                    "in": "query",
                    "required": true,
                    "schema": {
                        "type": "integer"
                    }
                },
                {
                    "name": "limit",
                    "in": "query",
                    "required": true,
                    "schema": {
                        "type": "integer"
                    }
                },
                {
                    "name": "sort",
                    "in": "query",
                    "required": false,
                    "schema": {
                        "type": "string"
                    }
                }
            ]
        }));

        let mut params = HashMap::new();
        params.insert(UniCase::new("page".to_string()), "1".to_string());
        params.insert(UniCase::new("limit".to_string()), "10".to_string());
        params.insert(UniCase::new("sort".to_string()), "asc".to_string());
        let query_params = TestParamStruct { data: params };

        // Execute
        let result = validator.validate_request_query_parameters(&operation, &query_params);
        println!("Error: {:?}", result);
        // Verify
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_request_query_parameters_non_query_parameters() {
        // Setup
        let spec = json!({
            "openapi": "3.0.0",
            "paths": {
                "/test": {
                    "get": {
                        "parameters": [
                            {
                                "name": "id",
                                "in": "path",
                                "required": true,
                                "schema": {
                                    "type": "string"
                                }
                            },
                            {
                                "name": "filter",
                                "in": "query",
                                "required": true,
                                "schema": {
                                    "type": "string"
                                }
                            }
                        ]
                    }
                }
            }
        });

        let validator = OpenApiPayloadValidator::new(spec).unwrap();
        let operation = create_operation(json!({
            "parameters": [
                {
                    "name": "id",
                    "in": "path",
                    "required": true,
                    "schema": {
                        "type": "string"
                    }
                },
                {
                    "name": "filter",
                    "in": "query",
                    "required": true,
                    "schema": {
                        "type": "string"
                    }
                }
            ]
        }));

        let mut params = HashMap::new();
        params.insert(UniCase::new("filter".to_string()), "active".to_string());
        let query_params = TestParamStruct { data: params };

        // Execute
        let result = validator.validate_request_query_parameters(&operation, &query_params);

        // Verify
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_request_query_parameters_invalid_schema_structure() {
        // Setup
        let spec = json!({
            "openapi": "3.0.0",
            "paths": {
                "/test": {
                    "get": {
                        "parameters": [
                            {
                                "name": "filter",
                                "in": "query",
                                "required": true
                                // Missing schema
                            }
                        ]
                    }
                }
            }
        });

        let validator = OpenApiPayloadValidator::new(spec).unwrap();
        let operation = create_operation(json!({
            "parameters": [
                {
                    "name": "filter",
                    "in": "query",
                    "required": true
                    // Missing schema
                }
            ]
        }));

        let mut params = HashMap::new();
        params.insert(UniCase::new("filter".to_string()), "active".to_string());
        let query_params = TestParamStruct { data: params };

        // Execute
        let result = validator.validate_request_query_parameters(&operation, &query_params);

        // Verify
        assert!(result.is_err());
    }

}
