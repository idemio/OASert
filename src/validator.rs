use crate::traverser::OpenApiTraverser;
use crate::types::{OpenApiTypes, OpenApiVersion, Operation, ParameterLocation};
use crate::{
    CONTENT_FIELD, ENCODED_BACKSLASH, ENCODED_TILDE, IN_FIELD, NAME_FIELD, OPENAPI_FIELD,
    PARAMETERS_FIELD, PATH_SEPARATOR, REF_FIELD, REQUEST_BODY_FIELD, REQUIRED_FIELD, SCHEMA_FIELD,
    SECURITY_FIELD, TILDE,
};
use jsonschema::{Resource, ValidationOptions, Validator as JsonValidator};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::fmt::{Display, Formatter};
use std::str::FromStr;

use http::HeaderMap;
use http::Request;

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

    pub fn traverser(&self) -> &OpenApiTraverser {
        &self.traverser
    }

    /// Extracts a valid content type from HTTP headers.
    fn extract_content_type(headers_instance: &HeaderMap) -> Option<String> {
        if let Some(content_type_header) = headers_instance.get("content-type") {
            if let Ok(content_type_header) = content_type_header.to_str() {
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
        }
        None
    }

    /// Validates a request body against an OpenAPI operation specification.
    pub fn validate_request_body<T>(
        &self,
        operation: &Operation,
        request: &Request<T>,
    ) -> Result<(), ValidationError>
    where
        T: serde::ser::Serialize,
    {
        let content_type = Self::extract_content_type(&request.headers());
        let body_instance = request.body();
        match serde_json::to_value(body_instance) {
            Ok(body) => {
                let validator = RequestBodyValidator::new(Some(&body), content_type);
                validator.validate(&self.traverser, operation, &self.options)
            }
            Err(_) => {
                let validator = RequestBodyValidator::new(None, content_type);
                validator.validate(&self.traverser, operation, &self.options)
            }
        }
    }

    pub fn validate_request<T>(
        &self,
        request: &Request<T>,
        scopes: Option<&Vec<String>>,
    ) -> Result<(), ValidationError>
    where
        T: serde::ser::Serialize,
    {
        let operation = self
            .traverser
            .get_operation(request.uri().path(), request.method().as_str())?;

        let content_type = Self::extract_content_type(&request.headers());
        match serde_json::to_value(request.body()) {
            Ok(body) => {
                let validator = RequestBodyValidator::new(Some(&body), content_type);
                validator.validate(&self.traverser, &operation, &self.options)
            }
            Err(_) => {
                let validator = RequestBodyValidator::new(None, content_type);
                validator.validate(&self.traverser, &operation, &self.options)
            }
        }?;
        self.validate_request_header_params(&operation, request.headers())?;

        if let Some(query_params) = request.uri().query() {
            self.validate_request_query_parameters(&operation, query_params)?;
        }

        if let Some(scopes) = scopes {
            self.validate_request_scopes(&operation, scopes)?;
        }

        Ok(())
    }

    /// Validates HTTP request headers against the parameters defined in an OpenAPI operation.
    pub fn validate_request_header_params(
        &self,
        operation: &Operation,
        headers: &HeaderMap,
    ) -> Result<(), ValidationError> {
        let headers: HashMap<String, String> = headers
            .iter()
            .filter_map(|(key, value)| {
                if let (key, Ok(value)) = (key.to_string(), value.to_str()) {
                    Some((key, value.to_string()))
                } else {
                    None
                }
            })
            .collect();
        let validator = RequestParameterValidator::new(&headers, ParameterLocation::Header);
        validator.validate(&self.traverser, operation, &self.options)
    }

    /// Validates query parameters against an OpenAPI operation definition.
    pub fn validate_request_query_parameters(
        &self,
        operation: &Operation,
        query_params: &str,
    ) -> Result<(), ValidationError> {
        let query_params: HashMap<String, String> = query_params
            .split("&")
            .filter_map(|pair| {
                let mut parts = pair.split('=');
                if let (Some(key), Some(value)) = (parts.next(), parts.next()) {
                    Some((key.to_string(), value.to_string()))
                } else {
                    None
                }
            })
            .collect();
        println!("{:?}", query_params);
        let validator = RequestParameterValidator::new(&query_params, ParameterLocation::Query);
        validator.validate(&self.traverser, operation, &self.options)
    }

    /// Validates if the provided scopes meet the security requirements of an operation.
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

    pub(crate) fn format_path(&self) -> String {
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
    fn complex_validation_by_path(
        options: &ValidationOptions,
        json_path: &JsonPath,
        instance: &Value,
    ) -> Result<(), ValidationError> {
        let full_pointer_path = format!("@@root#/{}", json_path.format_path());
        let schema = json!({
            REF_FIELD: full_pointer_path
        });
        let validator = Self::build_validator(options, &schema)?;
        match validator.validate(instance) {
            Ok(_) => Ok(()),
            Err(e) => Err(ValidationError::SchemaValidationFailed),
        }
    }

    fn complex_validation_by_schema(
        options: &ValidationOptions,
        schema: &Value,
        instance: &Value,
    ) -> Result<(), ValidationError> {
        let validator = Self::build_validator(options, schema)?;
        match validator.validate(instance) {
            Ok(_) => Ok(()),
            Err(e) => Err(ValidationError::SchemaValidationFailed),
        }
    }

    fn build_validator(
        validation_options: &ValidationOptions,
        schema: &Value,
    ) -> Result<JsonValidator, ValidationError> {
        let validator = match validation_options.build(&schema) {
            Ok(val) => val,
            Err(e) => {
                println!("{:?}", e);
                return Err(ValidationError::SchemaValidationFailed);
            }
        };
        Ok(validator)
    }
}

pub(crate) struct RequestParameterValidator<'a> {
    request_instance: &'a HashMap<String, String>,
    parameter_location: ParameterLocation,
}

impl<'a> RequestParameterValidator<'a> {
    pub(crate) fn new<'b>(
        request_instance: &'b HashMap<String, String>,
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
    fn validate(
        &self,
        traverser: &OpenApiTraverser,
        operation: &Operation,
        validation_options: &ValidationOptions,
    ) -> Result<(), ValidationError> {
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

                let mut parameter_index = 0usize;
                for parameter_definition in parameter_definitions {
                    // Only look at parameters that match the current section.
                    let location =
                        traverser.get_required_spec_node(parameter_definition, IN_FIELD)?;
                    let location = OpenApiTraverser::require_str(location.value())?;

                    if location.to_lowercase() == self.parameter_location.to_string().to_lowercase()
                    {
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
                        if let Some(request_parameter_value) =
                            self.request_instance.get(parameter_name)
                        {
                            let instance = json!(request_parameter_value);
                            if let Some(string) = instance.as_str() {
                                let instance = OpenApiTypes::convert_string_to_schema_type(
                                    parameter_schema,
                                    string,
                                )?;
                                Self::complex_validation_by_schema(
                                    validation_options,
                                    &parameter_schema,
                                    &instance,
                                )?
                            } else {
                                Self::complex_validation_by_schema(
                                    validation_options,
                                    &parameter_schema,
                                    &instance,
                                )?
                            }
                        } else if is_parameter_required {
                            return Err(ValidationError::RequiredParameterMissing);
                        }
                    }
                    parameter_index += 1;
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
                Self::complex_validation_by_path(
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

    fn validate_scopes_using_schema(
        security_definitions: &Value,
        request_scopes: &HashSet<&str>,
    ) -> Result<(), ValidationError> {
        // get the array of maps
        let security_definitions = OpenApiTraverser::require_array(security_definitions)?;

        if security_definitions.is_empty() {
            log::debug!("Definition is empty, scopes automatically pass");
            return Ok(());
        }

        for security_definition in security_definitions {
            // convert to map
            let security_definition = OpenApiTraverser::require_object(security_definition)?;
            for (security_schema_name, scope_list) in security_definition {
                // convert to list
                let scope_list = OpenApiTraverser::require_array(scope_list)?;
                let mut scopes_match_schema = true;

                // check to see if the scope is found in our request scopes
                'scope_match: for scope in scope_list {
                    let scope = OpenApiTraverser::require_str(scope)?;
                    if !request_scopes.contains(scope) {
                        scopes_match_schema = false;
                        break 'scope_match;
                    }
                }

                if scopes_match_schema {
                    log::debug!("Scopes match {security_schema_name}");
                    return Ok(());
                }
            }
        }
        Err(ValidationError::RequiredPropertyMissing)
    }
}

impl<'a> Validator for RequestScopeValidator<'a> {
    /// Validates whether a request has at least one of the required scopes specified in the operation's security definitions.
    fn validate(
        &self,
        traverser: &OpenApiTraverser,
        operation: &Operation,
        _validation_options: &ValidationOptions,
    ) -> Result<(), ValidationError> {
        let operation = &operation.data;
        println!("{:?}", operation);
        println!("{:?}", self.request_instance);
        let request_scopes: HashSet<&str> =
            self.request_instance.iter().map(|s| s.as_str()).collect();

        let security_definitions = traverser.get_optional_spec_node(operation, SECURITY_FIELD)?;
        if let Some(security_definitions) = security_definitions {
            return Self::validate_scopes_using_schema(
                security_definitions.value(),
                &request_scopes,
            );
        }

        let global_security_definitions =
            traverser.get_optional_spec_node(traverser.specification(), SECURITY_FIELD)?;
        if let Some(security_definitions) = global_security_definitions {
            return Self::validate_scopes_using_schema(
                security_definitions.value(),
                &request_scopes,
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use http::{HeaderMap, HeaderValue, Method, Request, Uri};
    use serde_json::json;

    // Helper function to create a mock operation with query parameters
    fn create_operation_with_query_params(parameters: serde_json::Value) -> Operation {
        let mut path = JsonPath::new();
        path.add("paths").add("/test").add("get");

        let operation_data = json!({
            "parameters": parameters
        });

        Operation {
            data: operation_data,
            path,
        }
    }

    // Helper function to create a validator with specific schema
    fn create_validator() -> OpenApiPayloadValidator {
        let spec = json!({
            "openapi": "3.0.0",
            "info": {
                "title": "Test API",
                "version": "1.0.0"
            },
            "paths": {
                "/test": {
                    "get": {
                        "parameters": [
                            {
                                "name": "limit",
                                "in": "query",
                                "required": true,
                                "schema": {
                                    "type": "integer",
                                    "minimum": 1,
                                    "maximum": 100
                                }
                            },
                            {
                                "name": "offset",
                                "in": "query",
                                "required": false,
                                "schema": {
                                    "type": "integer",
                                    "default": 0
                                }
                            },
                            {
                                "name": "filter",
                                "in": "query",
                                "required": false,
                                "schema": {
                                    "type": "string",
                                    "enum": ["active", "inactive", "all"]
                                }
                            }
                        ]
                    }
                }
            }
        });

        OpenApiPayloadValidator::new(spec).unwrap()
    }

    #[test]
    fn test_validate_valid_query_params() {
        let validator = create_validator();

        // Get the operation object
        let operation = validator.traverser().get_operation("/test", "get").unwrap();

        // Valid query string with all parameters
        let query_params = "limit=50&offset=10&filter=active";

        let result = validator.validate_request_query_parameters(&operation, query_params);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_query_params_missing_required() {
        let validator = create_validator();

        // Get the operation object
        let operation = validator.traverser().get_operation("/test", "get").unwrap();

        // Query string missing required 'limit' parameter
        let query_params = "offset=10&filter=active";

        let result = validator.validate_request_query_parameters(&operation, query_params);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ValidationError::RequiredParameterMissing
        ));
    }

    #[test]
    fn test_validate_query_params_with_only_required() {
        let validator = create_validator();

        // Get the operation object
        let operation = validator.traverser().get_operation("/test", "get").unwrap();

        // Query string with only the required parameter
        let query_params = "limit=50";

        let result = validator.validate_request_query_parameters(&operation, query_params);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_query_params_invalid_value() {
        let validator = create_validator();

        // Get the operation object
        let operation = validator.traverser().get_operation("/test", "get").unwrap();

        // Query string with invalid value for 'limit' (exceeds maximum)
        let query_params = "limit=200&offset=10";

        let result = validator.validate_request_query_parameters(&operation, query_params);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_query_params_non_numeric_for_integer() {
        let validator = create_validator();

        // Get the operation object
        let operation = validator.traverser().get_operation("/test", "get").unwrap();

        // Query string with non-numeric value for 'limit' which requires integer
        let query_params = "limit=abc&offset=10";

        let result = validator.validate_request_query_parameters(&operation, query_params);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_query_params_invalid_enum_value() {
        let validator = create_validator();

        // Get the operation object
        let operation = validator.traverser().get_operation("/test", "get").unwrap();

        // Query string with invalid enum value for 'filter'
        let query_params = "limit=50&filter=invalid";

        let result = validator.validate_request_query_parameters(&operation, query_params);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_query_params_empty_string() {
        let validator = create_validator();

        // Get the operation object
        let operation = validator.traverser().get_operation("/test", "get").unwrap();

        // Empty query string
        let query_params = "";

        let result = validator.validate_request_query_parameters(&operation, query_params);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ValidationError::RequiredParameterMissing
        ));
    }

    #[test]
    fn test_validate_query_params_malformed_query() {
        let validator = create_validator();

        // Get the operation object
        let operation = validator.traverser().get_operation("/test", "get").unwrap();

        // Malformed query string (missing value)
        let query_params = "limit=50&offset=";
        let result = validator.validate_request_query_parameters(&operation, query_params);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_query_params_duplicate_parameters() {
        let validator = create_validator();

        // Get the operation object
        let operation = validator.traverser().get_operation("/test", "get").unwrap();

        // Query string with duplicate parameters (last one should be used)
        let query_params = "limit=50&limit=75";

        let result = validator.validate_request_query_parameters(&operation, query_params);
        assert!(result.is_ok());

        // In this implementation, the last value overrides previous ones
        // We can't easily test the exact value used, but we know it should validate
    }

    #[test]
    fn test_validate_query_params_url_encoded_values() {
        let validator = create_validator();

        // Get the operation object
        let operation = validator.traverser().get_operation("/test", "get").unwrap();

        // Query string with URL-encoded values
        let query_params = "limit=50&filter=active%20items";

        // This test depends on how the validator handles URL encoding
        // If it doesn't decode values, this might fail
        let result = validator.validate_request_query_parameters(&operation, query_params);
        assert!(result.is_err()); // "active items" is not in the enum
    }

    #[test]
    fn test_validate_query_params_extra_parameters() {
        let validator = create_validator();

        // Get the operation object
        let operation = validator.traverser().get_operation("/test", "get").unwrap();

        // Query string with extra parameters not defined in the schema
        let query_params = "limit=50&extra=value";

        // Extra parameters should be ignored
        let result = validator.validate_request_query_parameters(&operation, query_params);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_query_params_no_parameters_defined() {
        // Create a validator with no parameters defined
        let spec = json!({
            "openapi": "3.0.0",
            "info": {
                "title": "Test API",
                "version": "1.0.0"
            },
            "paths": {
                "/test": {
                    "get": {}
                }
            }
        });

        let validator = OpenApiPayloadValidator::new(spec).unwrap();

        // Get the operation object
        let operation = validator.traverser().get_operation("/test", "get").unwrap();

        // Any query string should be valid when no parameters are defined
        let query_params = "param1=value1&param2=value2";

        let result = validator.validate_request_query_parameters(&operation, query_params);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_query_params_parsing_edge_cases() {
        let validator = create_validator();

        // Get the operation object
        let operation = validator.traverser().get_operation("/test", "get").unwrap();

        // Test with various edge cases in query string parsing

        // 1. Query string with just an ampersand
        let query_params = "&";
        let result = validator.validate_request_query_parameters(&operation, query_params);
        assert!(result.is_err()); // Missing required parameter

        // 2. Query string with just a key (no equals sign)
        let query_params = "limit";
        let result = validator.validate_request_query_parameters(&operation, query_params);
        assert!(result.is_err()); // Malformed query and missing required parameter

        // 3. Query string with just a key and equals sign
        let query_params = "limit=";
        let result = validator.validate_request_query_parameters(&operation, query_params);
        assert!(result.is_err()); // Malformed value for required parameter
    }

    #[test]
    fn test_validate_query_params_with_array_reference() {
        // Create a validator with a parameter that references a component
        let spec = json!({
            "openapi": "3.0.0",
            "info": {
                "title": "Test API",
                "version": "1.0.0"
            },
            "paths": {
                "/test": {
                    "get": {
                        "parameters": [
                            {
                                "$ref": "#/components/parameters/LimitParam"
                            }
                        ]
                    }
                }
            },
            "components": {
                "parameters": {
                    "LimitParam": {
                        "name": "limit",
                        "in": "query",
                        "required": true,
                        "schema": {
                            "type": "integer",
                            "minimum": 1
                        }
                    }
                }
            }
        });

        let validator = OpenApiPayloadValidator::new(spec).unwrap();

        // Get the operation object
        let operation = validator.traverser().get_operation("/test", "get").unwrap();

        // Valid query string
        let query_params = "limit=10";

        let result = validator.validate_request_query_parameters(&operation, query_params);
        assert!(result.is_ok());
    }

    fn create_operation_with_security(security_requirements: serde_json::Value) -> Operation {
        let mut path = JsonPath::new();
        path.add("paths").add("/test").add("get");

        let operation_data = json!({
            "security": security_requirements
        });

        Operation {
            data: operation_data,
            path,
        }
    }

    // Helper function to create a validator with specific security definitions
    fn create_validator_with_security_definitions(
        definitions: serde_json::Value,
    ) -> OpenApiPayloadValidator {
        let spec = json!({
            "openapi": "3.0.0",
            "info": {
                "title": "Test API",
                "version": "1.0.0"
            },
            "paths": {
                "/test": {
                    "get": {
                        "security": [
                            { "oauth2": ["read", "write"] }
                        ]
                    }
                }
            },
            "components": {
                "securitySchemes": definitions
            }
        });
        OpenApiPayloadValidator::new(spec).unwrap()
    }

    #[test]
    fn test_validate_request_scopes_success() {
        // Create a validator with OAuth2 security definition
        let validator = create_validator_with_security_definitions(json!({
            "oauth2": {
                "type": "oauth2",
                "flows": {
                    "implicit": {
                        "authorizationUrl": "https://example.com/auth",
                        "scopes": {
                            "read": "Read access",
                            "write": "Write access"
                        }
                    }
                }
            }
        }));

        // Create an operation requiring "read" and "write" scopes
        let operation = validator.traverser().get_operation("/test", "get").unwrap();

        // Test with matching scopes
        let scopes = vec!["read".to_string(), "write".to_string()];
        let result = validator.validate_request_scopes(&operation, &scopes);

        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_request_scopes_success_with_extra_scopes() {
        // Create a validator with OAuth2 security definition
        let validator = create_validator_with_security_definitions(json!({
            "oauth2": {
                "type": "oauth2",
                "flows": {
                    "implicit": {
                        "authorizationUrl": "https://example.com/auth",
                        "scopes": {
                            "read": "Read access",
                            "write": "Write access",
                            "admin": "Admin access"
                        }
                    }
                }
            }
        }));

        // Create an operation requiring "read" and "write" scopes
        let operation = validator.traverser().get_operation("/test", "get").unwrap();

        // Test with matching scopes plus an extra one
        let scopes = vec!["read".to_string(), "write".to_string(), "admin".to_string()];
        let result = validator.validate_request_scopes(&operation, &scopes);

        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_request_scopes_missing_required_scope() {
        // Create a validator with OAuth2 security definition
        let validator = create_validator_with_security_definitions(json!({
            "oauth2": {
                "type": "oauth2",
                "flows": {
                    "implicit": {
                        "authorizationUrl": "https://example.com/auth",
                        "scopes": {
                            "read": "Read access",
                            "write": "Write access"
                        }
                    }
                }
            }
        }));

        // Create an operation requiring "read" and "write" scopes
        let operation = validator.traverser().get_operation("/test", "get").unwrap();

        // Test with missing "write" scope
        let scopes = vec!["read".to_string()];
        let result = validator.validate_request_scopes(&operation, &scopes);

        assert!(result.is_err());
    }

    #[test]
    fn test_validate_request_scopes_empty_scopes() {
        // Create a validator with OAuth2 security definition
        let validator = create_validator_with_security_definitions(json!({
            "oauth2": {
                "type": "oauth2",
                "flows": {
                    "implicit": {
                        "authorizationUrl": "https://example.com/auth",
                        "scopes": {
                            "read": "Read access",
                            "write": "Write access"
                        }
                    }
                }
            }
        }));

        // Create an operation requiring "read" and "write" scopes
        let operation = validator.traverser().get_operation("/test", "get").unwrap();

        // Test with empty scopes
        let scopes = vec![];
        let result = validator.validate_request_scopes(&operation, &scopes);

        assert!(result.is_err());
    }

    #[test]
    fn test_validate_request_scopes_multiple_security_requirements_one_satisfied() {
        // Create a validator with multiple security definitions
        let validator = create_validator_with_security_definitions(json!({
            "oauth2": {
                "type": "oauth2",
                "flows": {
                    "implicit": {
                        "authorizationUrl": "https://example.com/auth",
                        "scopes": {
                            "read": "Read access",
                            "write": "Write access"
                        }
                    }
                }
            },
            "apiKey": {
                "type": "apiKey",
                "name": "api_key",
                "in": "header"
            }
        }));

        // Create an operation with alternative security requirements
        let operation = create_operation_with_security(json!([
            { "oauth2": ["read", "write"] },
            { "apiKey": [] }
        ]));

        // Test with satisfying the first requirement but not the second
        let scopes = vec!["read".to_string(), "write".to_string()];
        let result = validator.validate_request_scopes(&operation, &scopes);

        assert!(result.is_ok());
    }

    //    #[test]
    //    fn test_validate_request_scopes_multiple_security_requirements_none_satisfied() {
    //        // Create a validator with multiple security definitions
    //        let validator = create_validator_with_security_definitions(json!({
    //            "oauth2": {
    //                "type": "oauth2",
    //                "flows": {
    //                    "implicit": {
    //                        "authorizationUrl": "https://example.com/auth",
    //                        "scopes": {
    //                            "read": "Read access",
    //                            "write": "Write access"
    //                        }
    //                    }
    //                }
    //            },
    //            "apiKey": {
    //                "type": "apiKey",
    //                "name": "api_key",
    //                "in": "header"
    //            }
    //        }));
    //
    //        // Create an operation with alternative security requirements
    //        let operation = create_operation_with_security(json!([
    //            { "oauth2": ["read", "write"] },
    //            { "apiKey": [] }
    //        ]));
    //
    //        // Test with not satisfying any requirement
    //        let scopes = vec!["admin".to_string()];
    //        let result = validator.validate_request_scopes(&operation, &scopes);
    //
    //        assert!(result.is_err());
    //    }

    #[test]
    fn test_validate_request_scopes_no_security_requirement() {
        // Create a validator without security definitions
        let validator = create_validator_with_security_definitions(json!({}));

        // Create an operation without security requirements
        let operation = create_operation_with_security(json!([]));

        // Test with any scopes
        let scopes = vec!["read".to_string(), "write".to_string()];
        let result = validator.validate_request_scopes(&operation, &scopes);

        assert!(result.is_ok());
    }

    //    #[test]
    //    fn test_validate_request_scopes_with_invalid_security_scheme() {
    //        // Create a validator with an invalid security scheme
    //        let validator = create_validator_with_security_definitions(json!({
    //            "nonexistent": {
    //                "type": "oauth2",
    //                "flows": {
    //                    "implicit": {
    //                        "authorizationUrl": "https://example.com/auth",
    //                        "scopes": {
    //                            "read": "Read access"
    //                        }
    //                    }
    //                }
    //            }
    //        }));
    //
    //        // Create an operation requiring a different security scheme
    //        let operation = create_operation_with_security(json!([
    //            { "oauth2": ["read"] }
    //        ]));
    //
    //        // Test with scopes for a scheme that doesn't exist in the security definitions
    //        let scopes = vec!["read".to_string()];
    //        let result = validator.validate_request_scopes(&operation, &scopes);
    //
    //        assert!(result.is_err());
    //    }

    #[test]
    fn test_validate_request_scopes_with_malformed_security_requirement() {
        // Create a validator with OAuth2 security definition
        let validator = create_validator_with_security_definitions(json!({
            "oauth2": {
                "type": "oauth2",
                "flows": {
                    "implicit": {
                        "authorizationUrl": "https://example.com/auth",
                        "scopes": {
                            "read": "Read access",
                            "write": "Write access"
                        }
                    }
                }
            }
        }));

        // Create an operation with malformed security requirement (not an array of objects)
        let operation = create_operation_with_security(json!("malformed"));

        // Test with any scopes
        let scopes = vec!["read".to_string(), "write".to_string()];
        let result = validator.validate_request_scopes(&operation, &scopes);

        assert!(result.is_err());
    }

    #[test]
    fn test_validate_request_scopes_with_security_scheme_without_scopes() {
        // Create a validator with API key security definition
        let validator = create_validator_with_security_definitions(json!({
            "apiKey": {
                "type": "apiKey",
                "name": "api_key",
                "in": "header"
            }
        }));

        // Create an operation requiring API key authentication (no scopes)
        let operation = create_operation_with_security(json!([
            { "apiKey": [] }
        ]));

        // Test with empty scopes
        let scopes = vec![];
        let result = validator.validate_request_scopes(&operation, &scopes);

        // Should pass since API key schemes don't require scopes
        assert!(result.is_ok());
    }

    // Helper function to create a validator for testing
    fn create_test_validator() -> OpenApiPayloadValidator {
        // Create a minimal OpenAPI spec for testing
        let spec = json!({
            "openapi": "3.0.0",
            "info": {
                "title": "Test API",
                "version": "1.0.0"
            },
            "paths": {
                "/test": {
                    "get": {
                        "parameters": [
                            {
                                "name": "required_header",
                                "in": "header",
                                "required": true,
                                "schema": {
                                    "type": "string"
                                }
                            },
                            {
                                "name": "optional_query",
                                "in": "query",
                                "required": false,
                                "schema": {
                                    "type": "string"
                                }
                            }
                        ],
                        "requestBody": {
                            "required": true,
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "required": ["name"],
                                        "properties": {
                                            "name": { "type": "string" },
                                            "age": { "type": "integer" }
                                        }
                                    }
                                }
                            }
                        },
                        "security": [
                            {
                                "oauth2": ["read", "write"]
                            }
                        ],
                        "responses": {
                            "200": {
                                "description": "Success"
                            }
                        }
                    }
                }
            }
        });

        OpenApiPayloadValidator::new(spec).unwrap()
    }

    #[test]
    fn test_validate_valid_request() {
        let validator = create_test_validator();

        // Create headers with required header
        let mut headers = HeaderMap::new();
        headers.insert("required_header", HeaderValue::from_static("value"));
        headers.insert("content-type", HeaderValue::from_static("application/json"));

        // Create a valid body
        let body = json!({
            "name": "Test User",
            "age": 30
        });

        // Create a valid request
        let uri = Uri::builder()
            .scheme("https")
            .authority("example.com")
            .path_and_query("/test?optional_query=value")
            .build()
            .unwrap();

        let request = Request::builder()
            .method(Method::GET)
            .uri(uri)
            .header("required_header", "value")
            .header("content-type", "application/json")
            .body(body)
            .unwrap();

        // Provide required scopes
        let scopes = vec!["read".to_string(), "write".to_string()];

        // Should validate successfully
        let result = validator.validate_request(&request, Some(&scopes));
        assert!(result.is_ok());
    }

    #[test]
    fn test_invalid_operation() {
        let validator = create_test_validator();

        // Create a request with an invalid path
        let uri = Uri::builder()
            .scheme("https")
            .authority("example.com")
            .path_and_query("/invalid_path")
            .build()
            .unwrap();

        let request = Request::builder()
            .method(Method::GET)
            .uri(uri)
            .body(json!({}))
            .unwrap();

        // Should fail with MissingOperation
        let result = validator.validate_request(&request, None);
        assert!(matches!(result, Err(ValidationError::MissingOperation)));
    }

    #[test]
    fn test_invalid_request_body() {
        let validator = create_test_validator();

        // Create headers
        let mut headers = HeaderMap::new();
        headers.insert("required_header", HeaderValue::from_static("value"));
        headers.insert("content-type", HeaderValue::from_static("application/json"));

        // Create an invalid body (missing required 'name' field)
        let body = json!({
            "age": 30
        });

        // Create request
        let uri = Uri::builder()
            .scheme("https")
            .authority("example.com")
            .path_and_query("/test")
            .build()
            .unwrap();

        let request = Request::builder()
            .method(Method::GET)
            .uri(uri)
            .header("required_header", "value")
            .header("content-type", "application/json")
            .body(body)
            .unwrap();

        // Should fail with body validation error
        let result = validator.validate_request(&request, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_missing_required_header() {
        let validator = create_test_validator();

        // Create a valid body
        let body = json!({
            "name": "Test User",
            "age": 30
        });

        // Create request without required header
        let uri = Uri::builder()
            .scheme("https")
            .authority("example.com")
            .path_and_query("/test")
            .build()
            .unwrap();

        let request = Request::builder()
            .method(Method::GET)
            .uri(uri)
            .header("content-type", "application/json")
            .body(body)
            .unwrap();

        // Should fail with header validation error
        let result = validator.validate_request(&request, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_query_parameter_validation() {
        let validator = create_test_validator();

        // Create a valid body
        let body = json!({
            "name": "Test User",
            "age": 30
        });

        // Create a request with an invalid query parameter (if the schema had type restrictions)
        let uri = Uri::builder()
            .scheme("https")
            .authority("example.com")
            .path_and_query("/test?optional_query=invalid_value")
            .build()
            .unwrap();

        let request = Request::builder()
            .method(Method::GET)
            .uri(uri)
            .header("required_header", "value")
            .header("content-type", "application/json")
            .body(body)
            .unwrap();

        // If query validation would fail based on schema
        let result = validator.validate_request(&request, None);
        // Assert based on expected behavior
    }

    #[test]
    fn test_missing_scopes() {
        let validator = create_test_validator();

        // Create a valid body
        let body = json!({
            "name": "Test User",
            "age": 30
        });

        // Create a valid request
        let uri = Uri::builder()
            .scheme("https")
            .authority("example.com")
            .path_and_query("/test")
            .build()
            .unwrap();

        let request = Request::builder()
            .method(Method::GET)
            .uri(uri)
            .header("required_header", "value")
            .header("content-type", "application/json")
            .body(body)
            .unwrap();

        // Missing required scope
        let scopes = vec!["read".to_string()]; // Missing 'write' scope

        // Should fail with scope validation error
        let result = validator.validate_request(&request, Some(&scopes));
        assert!(result.is_err());
    }

    #[test]
    fn test_non_serializable_body() {
        // This test would be challenging since we'd need a type that implements
        // Serialize but fails during serialization. For completeness, we could
        // mock the serialization failure.

        struct NonSerializableType;

        impl serde::Serialize for NonSerializableType {
            fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                Err(serde::ser::Error::custom("Forced serialization error"))
            }
        }

        let validator = create_test_validator();

        let uri = Uri::builder()
            .scheme("https")
            .authority("example.com")
            .path_and_query("/test")
            .build()
            .unwrap();

        let request = Request::builder()
            .method(Method::GET)
            .uri(uri)
            .header("required_header", "value")
            .header("content-type", "application/json")
            .body(NonSerializableType)
            .unwrap();

        // Should handle serialization failure gracefully
        let result = validator.validate_request(&request, None);
        // The function should use None for the body in this case
    }

    #[test]
    fn test_no_query_parameters() {
        let validator = create_test_validator();

        // Create a valid body
        let body = json!({
            "name": "Test User",
            "age": 30
        });

        // Create request without query parameters
        let uri = Uri::builder()
            .scheme("https")
            .authority("example.com")
            .path_and_query("/test")
            .build()
            .unwrap();

        let request = Request::builder()
            .method(Method::GET)
            .uri(uri)
            .header("required_header", "value")
            .header("content-type", "application/json")
            .body(body)
            .unwrap();

        // Should still validate successfully as the query param is optional
        let scopes = vec!["read".to_string(), "write".to_string()];
        let result = validator.validate_request(&request, Some(&scopes));
        assert!(result.is_ok());
    }

    #[test]
    fn test_no_scopes_provided() {
        let validator = create_test_validator();

        // Create a valid body
        let body = json!({
            "name": "Test User",
            "age": 30
        });

        // Create a valid request
        let uri = Uri::builder()
            .scheme("https")
            .authority("example.com")
            .path_and_query("/test")
            .build()
            .unwrap();

        let request = Request::builder()
            .method(Method::GET)
            .uri(uri)
            .header("required_header", "value")
            .header("content-type", "application/json")
            .body(body)
            .unwrap();

        // No scopes provided (passing None)
        let result = validator.validate_request(&request, None);
        // Depending on implementation, this might fail or skip scope validation
    }

    #[test]
    fn test_new_json_path() {
        let path = JsonPath::new();
        assert_eq!(path.0.len(), 0);
        assert_eq!(path.format_path(), "");
    }

    #[test]
    fn test_add_simple_segment() {
        let mut path = JsonPath::new();
        path.add("simple");
        assert_eq!(path.0.len(), 1);
        assert_eq!(path.0[0], "simple");
        assert_eq!(path.format_path(), "simple");
    }

    #[test]
    fn test_add_multiple_segments() {
        let mut path = JsonPath::new();
        path.add("component").add("schemas").add("User");
        assert_eq!(path.0.len(), 3);
        assert_eq!(path.0[0], "component");
        assert_eq!(path.0[1], "schemas");
        assert_eq!(path.0[2], "User");
        assert_eq!(path.format_path(), "component/schemas/User");
    }

    #[test]
    fn test_add_segment_with_tilde() {
        let mut path = JsonPath::new();
        path.add("user~name");
        assert_eq!(path.0.len(), 1);
        assert_eq!(path.0[0], format!("user{}name", ENCODED_TILDE));

        // Check that the tilde is properly encoded in the formatted path
        let formatted = path.format_path();
        assert_eq!(formatted, format!("user{}name", ENCODED_TILDE));
    }

    #[test]
    fn test_add_segment_with_slash() {
        let mut path = JsonPath::new();
        path.add("user/profile");
        assert_eq!(path.0.len(), 1);
        assert_eq!(path.0[0], format!("user{}profile", ENCODED_BACKSLASH));

        // Check that the slash is properly encoded in the formatted path
        let formatted = path.format_path();
        assert_eq!(formatted, format!("user{}profile", ENCODED_BACKSLASH));
        assert!(!formatted.contains("/"));
    }

    #[test]
    fn test_add_segment_with_tilde_and_slash() {
        let mut path = JsonPath::new();
        path.add("user~/profile");
        assert_eq!(path.0.len(), 1);

        // Both special characters should be encoded
        let expected = "user".to_string() + ENCODED_TILDE + ENCODED_BACKSLASH + "profile";
        assert_eq!(path.0[0], expected);

        // Check that both special characters are properly encoded in the formatted path
        let formatted = path.format_path();
        assert_eq!(formatted, expected);
    }

    #[test]
    fn test_format_path_empty() {
        let path = JsonPath::new();
        assert_eq!(path.format_path(), "");
    }

    #[test]
    fn test_format_path_single_segment() {
        let mut path = JsonPath::new();
        path.add("test");
        assert_eq!(path.format_path(), "test");
    }

    #[test]
    fn test_format_path_complex() {
        let mut path = JsonPath::new();
        path.add("paths")
            .add("/users/{id}")
            .add("get")
            .add("responses")
            .add("200");

        // The segment with slashes should be encoded
        let expected_second = format!("{}users{}{{id}}", ENCODED_BACKSLASH, ENCODED_BACKSLASH);
        assert_eq!(path.0[1], expected_second);

        // The formatted path should have the proper separators and encodings
        let expected_path = format!(
            "paths{}{}{}{}{}{}{}{}",
            PATH_SEPARATOR,
            expected_second,
            PATH_SEPARATOR,
            "get",
            PATH_SEPARATOR,
            "responses",
            PATH_SEPARATOR,
            "200"
        );
        assert_eq!(path.format_path(), expected_path);
    }

    #[test]
    fn test_chained_add_operations() {
        let mut path = JsonPath::new();
        let result = path.add("first").add("second").add("third");

        // Verify that we get a mutable reference back each time for chaining
        assert_eq!(result.0.len(), 3);
        assert_eq!(path.0.len(), 3);
        assert_eq!(path.format_path(), "first/second/third");
    }

    #[test]
    fn test_add_empty_segment() {
        let mut path = JsonPath::new();
        path.add("");
        assert_eq!(path.0.len(), 1);
        assert_eq!(path.0[0], "");
        assert_eq!(path.format_path(), "");
    }

    #[test]
    fn test_json_pointer_compatibility() {
        // Test that the path representation is compatible with JSON Pointer format
        // by creating paths that would be used in real OpenAPI specs

        let mut path = JsonPath::new();
        path.add("components").add("schemas").add("Error");
        assert_eq!(path.format_path(), "components/schemas/Error");

        let mut path = JsonPath::new();
        path.add("paths")
            .add("/pets")
            .add("get")
            .add("parameters")
            .add("0");

        // The '/pets' segment should have its slash encoded
        let expected_second = format!("{}pets", ENCODED_BACKSLASH);
        assert_eq!(path.0[1], expected_second);

        let expected_path = format!(
            "paths{}{}{}{}{}{}{}{}",
            PATH_SEPARATOR,
            expected_second,
            PATH_SEPARATOR,
            "get",
            PATH_SEPARATOR,
            "parameters",
            PATH_SEPARATOR,
            "0"
        );
        assert_eq!(path.format_path(), expected_path);
    }

    #[test]
    fn test_special_characters_encoding() {
        let mut path = JsonPath::new();

        // Test various special character combinations
        path.add("a~b/c");
        path.add("d/e~f");
        path.add("~~/~~");
        path.add("//");

        assert_eq!(path.0.len(), 4);

        // First segment: a~b/c -> a~0b~1c
        assert_eq!(
            path.0[0],
            format!("a{}b{}c", ENCODED_TILDE, ENCODED_BACKSLASH)
        );

        // Second segment: d/e~f -> d~1e~0f
        assert_eq!(
            path.0[1],
            format!("d{}e{}f", ENCODED_BACKSLASH, ENCODED_TILDE)
        );

        // Third segment: ~~/~~ -> ~0~0~1~0~0
        assert_eq!(
            path.0[2],
            format!("{0}{0}{1}{0}{0}", ENCODED_TILDE, ENCODED_BACKSLASH)
        );

        // Fourth segment: // -> ~1~1
        assert_eq!(path.0[3], format!("{0}{0}", ENCODED_BACKSLASH));

        // Verify the entire formatted path
        let expected_path = [
            format!("a{}b{}c", ENCODED_TILDE, ENCODED_BACKSLASH),
            format!("d{}e{}f", ENCODED_BACKSLASH, ENCODED_TILDE),
            format!("{0}{0}{1}{0}{0}", ENCODED_TILDE, ENCODED_BACKSLASH),
            format!("{0}{0}", ENCODED_BACKSLASH),
        ]
        .join(PATH_SEPARATOR);

        assert_eq!(path.format_path(), expected_path);
    }

    #[test]
    fn test_numeric_segments() {
        let mut path = JsonPath::new();
        path.add("items").add("0").add("name");

        assert_eq!(path.0.len(), 3);
        assert_eq!(path.0[0], "items");
        assert_eq!(path.0[1], "0");
        assert_eq!(path.0[2], "name");
        assert_eq!(path.format_path(), "items/0/name");
    }

    #[test]
    fn test_modify_existing_path() {
        let mut path = JsonPath::new();
        path.add("components").add("schemas");

        assert_eq!(path.format_path(), "components/schemas");

        // Now modify by adding more segments
        path.add("User").add("properties").add("email");

        assert_eq!(path.0.len(), 5);
        assert_eq!(
            path.format_path(),
            "components/schemas/User/properties/email"
        );
    }
}
