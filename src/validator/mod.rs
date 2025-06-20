mod request_body;
mod request_parameter;
mod scope;

use crate::error::{OperationSection, Section, SpecificationSection, ValidationErrorType};
use crate::traverser::OpenApiTraverser;
use crate::types::json_path::JsonPath;
use crate::types::version::OpenApiVersion;
use crate::types::{HttpLike, Operation, ParameterLocation};
use crate::validator::request_body::RequestBodyValidator;
use crate::validator::request_parameter::RequestParameterValidator;
use crate::validator::scope::RequestScopeValidator;
use crate::{OPENAPI_FIELD, PATHS_FIELD, REF_FIELD};
use http::HeaderMap;
use jsonschema::{Resource, ValidationOptions, Validator as JsonValidator};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;

pub struct OpenApiPayloadValidator {
    traverser: OpenApiTraverser,
    options: ValidationOptions,
}

impl OpenApiPayloadValidator {
    pub fn new(mut value: Value) -> Result<Self, ValidationErrorType> {
        // Assign ID for schema validation in the future.
        value["$id"] = json!("@@root");

        // Find the version defined in the spec and get the corresponding draft for validation.
        let version = match value.get(OPENAPI_FIELD).and_then(|v| v.as_str()) {
            None => {
                return Err(ValidationErrorType::FieldExpected(
                    OPENAPI_FIELD.to_string(),
                    Section::Specification(SpecificationSection::Other),
                ));
            }
            Some(v) => v,
        };
        let version = OpenApiVersion::from_str(version)?;
        let draft = version.get_draft();

        // Create this resource once and re-use it for multiple validation calls.
        let resource = match Resource::from_contents(value.clone()) {
            Ok(res) => res,
            Err(e) => {
                return Err(ValidationErrorType::SchemaValidationFailed(
                    e.to_string(),
                    Section::Specification(SpecificationSection::Other),
                ));
            }
        };

        // Assign draft and provide resource
        let options = JsonValidator::options()
            .with_draft(draft)
            .with_resource("@@inner", resource);

        // Create the traverser with owned value
        let traverser = match OpenApiTraverser::new(value) {
            Ok(traverser) => traverser,
            Err(_) => {
                return Err(ValidationErrorType::FieldExpected(
                    PATHS_FIELD.to_string(),
                    Section::Specification(SpecificationSection::Paths(OperationSection::Other)),
                ));
            }
        };

        Ok(Self { traverser, options })
    }

    pub fn traverser(&self) -> &OpenApiTraverser {
        &self.traverser
    }

    /// Extracts the content type from HTTP headers.
    ///
    /// This function parses the "content-type" header from a HeaderMap and returns the
    /// primary content type value (e.g., "application/json") without any parameters.
    ///
    /// # Arguments
    ///
    /// * `headers_instance` - A reference to a HeaderMap containing HTTP headers
    ///
    /// # Returns
    ///
    /// * `Some(String)` - The extracted content type if found and valid
    /// * `None` - If no valid content type was found or if parsing failed
    fn extract_content_type(headers_instance: &HeaderMap) -> Option<&str> {
        if let Some(content_type_header) = headers_instance.get("content-type") {
            if let Ok(content_type_header) = content_type_header.to_str() {
                if let Some(split_content_type) = content_type_header
                    .split(";")
                    .find(|content_type_segment| content_type_segment.contains("/"))
                {
                    return Some(split_content_type.trim());
                }
            }
        }
        None
    }

    /// # find_operation
    ///
    /// Retrieves an OpenAPI Operation object that matches the specified path and HTTP method.
    ///
    /// This function acts as a wrapper around the internal traverser's `get_operation` method.
    /// It searches through the OpenAPI specification document to find an operation that matches
    /// the provided request path and method.
    ///
    /// ## Arguments
    ///
    /// * `path` - A string slice representing the request path to match against paths defined
    ///   in the OpenAPI specification
    /// * `method` - A string slice representing the HTTP method (GET, POST, etc.) to match
    ///   against methods defined in the OpenAPI specification
    ///
    /// ## Returns
    ///
    /// * `Ok(Arc<Operation>)` - Pointer to the Operation object if a matching path and method combination is found in the specification.
    /// * `Err(ValidationErrorType)` - An error indicating why the operation couldn't be found,
    ///   typically `ValidationErrorType::FieldExpected` when no matching path+method is found
    ///
    /// ## Example
    ///
    /// ```rust
    /// use oasert::validator::OpenApiPayloadValidator;
    ///
    /// // Mini-spec for testing
    /// let schema = serde_json::json!({
    ///     "openapi": "3.1.0",
    ///     "paths": {
    ///         "/pets": {
    ///             "get": {
    ///                 "responses": {
    ///                     "200": {
    ///                         "description": "OK"
    ///                     }
    ///                 }
    ///             }
    ///         }
    ///     }
    /// });
    ///
    /// let validator = OpenApiPayloadValidator::new(schema).unwrap();
    /// match validator.find_operation("/pets", "get") {
    ///     Ok(operation) => {
    ///         // Use the operation for validation or other purposes
    ///         println!("Found operation: {:?}", operation);
    ///     },
    ///     Err(err) => {
    ///         println!("Operation not found: {:?}", err);
    ///     }
    /// }
    /// ```
    pub fn find_operation(
        &self,
        path: &str,
        method: &str,
    ) -> Result<Arc<Operation>, ValidationErrorType> {
        match self
            .traverser
            .get_operation_from_path_and_method(path, method)
        {
            Ok(op) => Ok(op),
            Err(_) => todo!(),
        }
    }

    /// # validate_request_body
    ///
    /// Validates an HTTP request body against an OpenAPI operation specification.
    ///
    /// # Arguments
    ///
    /// * `operation` - The OpenAPI operation specification to validate against
    /// * `request` - The HTTP request containing the body and headers to validate
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If the request body is valid according to the OpenAPI specification
    /// * `Err(ValidationErrorType)` - If validation fails, with specific error details:
    ///   - `SectionExpected` - If the request has a body but the operation doesn't define a request body
    ///   - `FieldExpected` - If a required Content-Type header is missing
    ///   - `SchemaValidationFailed` - If the body doesn't match the schema
    ///   - Various other error types for specific validation failures
    ///
    /// # Example
    ///
    /// ```rust
    /// use http::Request;
    /// use oasert::validator::OpenApiPayloadValidator;
    /// use serde_json::json;
    ///
    /// // Mini-spec for testing
    /// let schema = json!({
    ///     "openapi": "3.1.0",
    ///     "paths": {
    ///         "/my-path": {
    ///             "post": {
    ///                 "requestBody": {
    ///                     "content": {
    ///                         "application/json": {
    ///                             "schema": {
    ///                                 "type": "object",
    ///                                 "required": ["name"],
    ///                                 "properties": {
    ///                                     "name": {
    ///                                         "type": "string"
    ///                                     }
    ///                                 }
    ///                             }
    ///                         }
    ///                     }
    ///                 }
    ///             }
    ///         }
    ///     }
    /// });
    ///
    /// let validator = OpenApiPayloadValidator::new(schema).unwrap();
    /// let operation = validator.find_operation("/my-path", "POST").unwrap();
    /// let request = Request::builder()
    ///     .header("content-type", "application/json")
    ///     .body(json!({ "name": "example" }))
    ///     .unwrap();
    ///
    /// match validator.validate_request_body(&operation, &request) {
    ///     Ok(()) => println!("Request body is valid"),
    ///     Err(err) => println!("Validation error: {:?}", err),
    /// }
    /// ```
    pub fn validate_request_body<T>(
        &self,
        operation: &Operation,
        request: &impl HttpLike<T>,
    ) -> Result<(), ValidationErrorType>
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

    /// # validate_request
    ///
    /// Validates an HTTP request against an OpenAPI specification.
    ///
    /// This function validates different aspects of an HTTP request, including:
    /// - Matching the request path and method against a defined operation in the OpenAPI spec
    /// - Validating the request body against the schema for the specified content type
    /// - Validating request headers against parameter requirements
    /// - Validating query parameters against parameter requirements
    /// - Validating that the request has the required scopes (if applicable)
    ///
    /// # Arguments
    ///
    /// * `request` - An implementation of the `HttpLike` trait that provides access to request components
    ///               (method, path, headers, body, query parameters)
    /// * `scopes` - An optional vector of authorization scopes that the request has
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If the request is valid according to the OpenAPI specification
    /// * `Err(ValidationErrorType)` - If any validation fails, with details about the failure
    pub fn validate_request<T>(
        &self,
        request: &impl HttpLike<T>,
        scopes: Option<&Vec<String>>,
    ) -> Result<(), ValidationErrorType>
    where
        T: serde::ser::Serialize,
    {
        let operation = self.find_operation(request.path(), request.method().as_str())?;
        self.validate_request_body(&operation, request)?;
        self.validate_request_header_params(&operation, request.headers())?;

        if let Some(query_params) = request.query() {
            self.validate_request_query_parameters(&operation, query_params)?;
        }

        if let Some(scopes) = scopes {
            self.validate_request_scopes(&operation, scopes)?;
        }

        Ok(())
    }

    /// # validate_request_header_params
    ///
    /// Validates HTTP request headers against OpenAPI operation specification parameters.
    /// The function converts the HTTP headers into a map of string key-value pairs, then creates
    /// a RequestParameterValidator to validate these headers against the operation's header parameters.
    ///
    /// ## Arguments
    ///
    /// * `operation` - A reference to an Operation that contains the OpenAPI operation definition
    ///   with parameter specifications to validate against
    /// * `headers` - A reference to a HeaderMap containing the HTTP request headers to validate
    ///
    /// ## Returns
    ///
    /// * `Ok(())` - If all required header parameters are present and valid according to their schemas
    /// * `Err(ValidationErrorType)` - If validation fails, with the specific error type indicating the reason:
    ///   - `FieldExpected` - If a required header parameter is missing
    ///   - `SchemaValidationFailed` - If a header value doesn't match its schema
    ///   - `UnexpectedType` - If a header value has an incorrect type
    ///   - Other error types depending on specific validation failures
    ///
    /// ## Example
    ///
    /// ```rust
    /// use http::HeaderMap;
    /// use oasert::validator::OpenApiPayloadValidator;
    ///
    /// // Mini-spec for testing
    /// let schema = serde_json::json!({
    ///     "openapi": "3.1.0",
    ///     "paths": {
    ///         "/my-path": {
    ///             "get": {
    ///                 "parameters": [
    ///                     {
    ///                         "name": "Content-Type",
    ///                         "in": "header",
    ///                         "required": true,
    ///                     }
    ///                 ]
    ///             }
    ///         }
    ///     }
    /// });
    ///
    /// let validator = OpenApiPayloadValidator::new(schema).unwrap();
    /// let operation = validator.find_operation("/my-path", "GET").unwrap();
    /// let mut headers = HeaderMap::new();
    /// headers.insert("Content-Type", "application/json".parse().unwrap());
    ///
    /// match validator.validate_request_header_params(&operation, &headers) {
    ///     Ok(()) => println!("Headers validated successfully"),
    ///     Err(err) => println!("Header validation failed: {:?}", err),
    /// }
    /// ```
    pub fn validate_request_header_params(
        &self,
        operation: &Operation,
        headers: &HeaderMap,
    ) -> Result<(), ValidationErrorType> {
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

    /// # validate_request_query_parameters
    ///
    /// Validates query parameters against an OpenAPI operation definition.
    ///
    /// Parses the query string into a HashMap of key-value pairs and validates them against
    /// the parameters defined in the OpenAPI operation specification. The function uses
    /// the RequestParameterValidator to perform the actual validation.
    ///
    /// ## Arguments
    ///
    /// * `operation` - A reference to an Operation object containing the OpenAPI operation definition
    /// * `query_params` - A string containing the raw URL query parameters in the format "key1=value1&key2=value2"
    ///
    /// ## Returns
    ///
    /// * `Ok(())` - If all query parameters are valid according to the operation definition
    /// * `Err(ValidationErrorType)` - If validation fails, returns one of several possible error types:
    ///   - `FieldExpected` - When a required parameter is missing
    ///   - `SchemaValidationFailed` - When a parameter value doesn't match the schema
    ///   - `UnexpectedType` - When a parameter value type doesn't match the expected type
    ///   - Various other validation errors depending on the specific validation failure
    ///
    /// ## Example
    ///
    /// ```
    ///
    /// use oasert::validator::OpenApiPayloadValidator;
    ///
    /// // Mini-spec for testing
    /// let schema = serde_json::json!({
    ///     "openapi": "3.1.0",
    ///     "paths": {
    ///         "/my-path": {
    ///             "get": {
    ///                 "parameters": [
    ///                     {
    ///                         "name": "limit",
    ///                         "in": "query",
    ///                         "required": true,
    ///                     }
    ///                 ]
    ///             }
    ///         }
    ///     }
    /// });
    /// let validator = OpenApiPayloadValidator::new(schema).unwrap();
    /// let operation = validator.find_operation("/my-path", "GET").unwrap();
    /// let query_string = "limit=10&status=available";
    ///
    /// match validator.validate_request_query_parameters(&operation, query_string) {
    ///     Ok(()) => println!("Query parameters are valid!"),
    ///     Err(e) => println!("Validation error: {:?}", e),
    /// }
    /// ```
    pub fn validate_request_query_parameters(
        &self,
        operation: &Operation,
        query_params: &str,
    ) -> Result<(), ValidationErrorType> {
        let query_params: HashMap<String, String> = query_params
            .split("&")
            .filter_map(|pair| {
                let mut parts = pair.split('=');
                match (parts.next(), parts.next()) {
                    (Some(key), Some(value)) => {
                        let key = percent_encoding::percent_decode_str(key)
                            .decode_utf8_lossy()
                            .to_string();
                        let value = percent_encoding::percent_decode_str(value)
                            .decode_utf8_lossy()
                            .to_string();
                        Some((key, value))
                    }
                    (Some(key), None) => {
                        let key = percent_encoding::percent_decode_str(key)
                            .decode_utf8_lossy()
                            .to_string();
                        Some((key, "".to_string()))
                    }
                    _ => {
                        log::warn!("Invalid query parameter: {}", pair);
                        None
                    }
                }
            })
            .collect();
        let validator = RequestParameterValidator::new(&query_params, ParameterLocation::Query);
        validator.validate(&self.traverser, operation, &self.options)
    }

    /// # validate_request_scopes
    ///
    /// Validates that the provided request scopes satisfy the security requirements defined in the OpenAPI specification.
    ///
    /// This function checks whether the given scopes are sufficient to access the specified operation
    /// according to the security definitions in the OpenAPI document. It creates a `RequestScopeValidator`
    /// and delegates the validation logic to it.
    ///
    /// # Arguments
    ///
    /// * `operation` - A reference to an `Operation` struct representing the API operation being validated
    /// * `scopes` - A reference to a vector of strings containing the authorization scopes provided in the request
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If the provided scopes satisfy the security requirements for the operation
    /// * `Err(ValidationErrorType)` - If validation fails, with the specific error type describing the reason
    ///
    /// # Example
    ///
    /// ```rust
    /// use oasert::types::Operation;
    /// use oasert::validator::OpenApiPayloadValidator;
    ///
    /// // Mini-spec for testing
    /// let schema = serde_json::json!({
    /// "openapi": "3.1.0",
    /// "paths": {
    ///     "/my-path": {
    ///         "get": {
    ///             "security": [
    ///                 {
    ///                     "oauth2": [
    ///                         "read:items",
    ///                         "write:items"
    ///                     ]
    ///                 }
    ///             ]
    ///         }
    ///     }
    /// }
    /// });
    ///
    /// let validator = OpenApiPayloadValidator::new(schema).unwrap();
    /// let operation = validator.find_operation("/my-path", "GET").unwrap();
    ///
    /// let scopes = vec!["read:items".to_string(), "write:items".to_string()];
    ///
    /// match validator.validate_request_scopes(&operation, &scopes) {
    ///     Ok(()) => println!("Request scopes are valid"),
    ///     Err(e) => println!("Validation failed: {:?}", e),
    /// }
    /// ```
    pub fn validate_request_scopes(
        &self,
        operation: &Operation,
        scopes: &Vec<String>,
    ) -> Result<(), ValidationErrorType> {
        let validator = RequestScopeValidator::new(scopes);
        validator.validate(&self.traverser, operation, &self.options)
    }
}

pub(crate) trait Validator {
    /// # validate
    ///
    /// Validates an OpenAPI operation against a set of validation rules.
    ///
    /// # Arguments
    ///
    /// * `traverser` - Reference to an OpenApiTraverser that provides access to the full OpenAPI specification
    ///                 and previously resolved references and operations.
    /// * `operation` - Reference to the Operation being validated, containing the operation data and JSON path.
    /// * `validation_options` - Reference to ValidationOptions that configure the validation behavior.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If the validation passes without errors.
    /// * `Err(ValidationErrorType)` - If validation fails, returns one of several error types:
    ///   - UnsupportedSpecVersion - When the OpenAPI version is not supported
    ///   - SchemaValidationFailed - When schema validation fails
    ///   - ValueExpected - When a required value is missing
    ///   - SectionExpected - When a required section is missing
    ///   - FieldExpected - When a required field is missing
    ///   - UnexpectedType - When a value's type doesn't match the expected type
    ///   - UnableToParse - When data cannot be parsed correctly
    ///   - CircularReference - When a circular reference is detected
    ///   - InvalidRef - When a reference is invalid
    fn validate(
        &self,
        traverser: &OpenApiTraverser,
        operation: &Operation,
        validation_options: &ValidationOptions,
    ) -> Result<(), ValidationErrorType>;

    fn section(&self) -> &Section;

    /// Validates a JSON instance against a schema referenced by a JSON path.
    ///
    /// # Arguments
    /// * `options` - The validation options used to configure the validator
    /// * `json_path` - A path reference to the schema to validate against
    /// * `instance` - The JSON value to validate
    /// * `section` - The section context for error reporting
    ///
    /// # Returns
    /// * `Ok(())` - If validation succeeds
    /// * `Err(ValidationErrorType)` - If validation fails, containing the specific error type
    fn complex_validation_by_path<'a>(
        options: &ValidationOptions,
        json_path: &JsonPath,
        instance: &Value,
        section: Section,
    ) -> Result<(), ValidationErrorType> {
        let full_pointer_path = format!("@@root#/{}", json_path.format_path());
        let schema = json!({
            REF_FIELD: full_pointer_path
        });
        let validator = Self::build_validator(options, &schema)?;
        Self::do_validate(&validator, instance, section)
    }

    /// Validates a JSON instance against a JSON schema.
    ///
    /// This function takes a JSON schema and an instance, builds a validator based on the provided options,
    /// and performs validation of the instance against the schema.
    ///
    /// # Arguments
    ///
    /// * `options` - Configuration options for the validation process
    /// * `schema` - The JSON schema to validate against
    /// * `instance` - The JSON instance to be validated
    /// * `section` - Indicates which part of the document is being validated (Specification or Payload)
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If validation succeeds
    /// * `Err(ValidationErrorType)` - If validation fails, containing details about the validation error
    fn complex_validation_by_schema(
        options: &ValidationOptions,
        schema: &Value,
        instance: &Value,
        section: Section,
    ) -> Result<(), ValidationErrorType> {
        let validator = Self::build_validator(options, schema)?;
        Self::do_validate(&validator, instance, section)
    }

    /// Validates a JSON instance against a JSON Schema validator.
    ///
    /// # Arguments
    /// * `validator` - A reference to a JSON Schema validator that will perform the validation.
    /// * `instance` - A reference to a JSON Value to be validated against the schema.
    /// * `section` - The section of the document where the validation is taking place, used in error reporting.
    ///
    /// # Returns
    /// * `Ok(())` - If the validation passes.
    /// * `Err(ValidationErrorType::SchemaValidationFailed)` - If validation fails, containing the error message and section where the failure occurred.
    fn do_validate(
        validator: &jsonschema::Validator,
        instance: &Value,
        section: Section,
    ) -> Result<(), ValidationErrorType> {
        match validator.validate(instance) {
            Ok(_) => Ok(()),
            Err(e) => Err(ValidationErrorType::SchemaValidationFailed(
                e.to_string(),
                section,
            )),
        }
    }

    /// Builds a JSON schema validator using the provided validation options and schema.
    ///
    /// # Arguments
    ///
    /// * `validation_options` - The validation options that configure how the schema validation should be performed
    /// * `schema` - The JSON schema used for validation, represented as a serde_json Value
    ///
    /// # Returns
    ///
    /// * `Ok(JsonValidator)` - A successfully built JSON validator
    /// * `Err(ValidationErrorType::SchemaValidationFailed)` - If the schema validation fails during validator construction
    fn build_validator<'a>(
        validation_options: &ValidationOptions,
        schema: &Value,
    ) -> Result<JsonValidator, ValidationErrorType> {
        let validator = match validation_options.build(&schema) {
            Ok(val) => val,
            Err(e) => {
                return Err(ValidationErrorType::SchemaValidationFailed(
                    e.to_string(),
                    Section::Specification(SpecificationSection::Other),
                ));
            }
        };
        Ok(validator)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use http::{HeaderMap, HeaderValue, Method, Request, Uri};
    use serde_json::json;

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
                                    "type": "string",
                                    "minLength": 3,
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
        let mut headers = HeaderMap::new();
        headers.insert("required_header", HeaderValue::from_static("value"));
        headers.insert("content-type", HeaderValue::from_static("application/json"));
        let body = json!({
            "name": "Test User",
            "age": 30
        });
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
        let scopes = vec!["read".to_string(), "write".to_string()];
        let result = validator.validate_request(&request, Some(&scopes));
        assert!(result.is_ok());
    }

    #[test]
    fn test_invalid_request_body() {
        let validator = create_test_validator();
        let mut headers = HeaderMap::new();
        headers.insert("required_header", HeaderValue::from_static("value"));
        headers.insert("content-type", HeaderValue::from_static("application/json"));
        let body = json!({
            "age": 30
        });
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
        let result = validator.validate_request(&request, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_missing_required_header() {
        let validator = create_test_validator();
        let body = json!({
            "name": "Test User",
            "age": 30
        });
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
        let result = validator.validate_request(&request, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_query_parameter_validation() {
        let validator = create_test_validator();
        let body = json!({
            "name": "Test User",
            "age": 30
        });

        let uri = Uri::builder()
            .scheme("https")
            .authority("example.com")
            // min length is set to 3, so this query parameter is invalid
            .path_and_query("/test?optional_query=aa")
            .build()
            .unwrap();
        let request = Request::builder()
            .method(Method::GET)
            .uri(uri)
            .header("required_header", "value")
            .header("content-type", "application/json")
            .body(body)
            .unwrap();
        let result = validator.validate_request(&request, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_missing_scopes() {
        let validator = create_test_validator();
        let body = json!({
            "name": "Test User",
            "age": 30
        });
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
        let scopes = vec!["read".to_string()]; // Missing 'write' scope
        let result = validator.validate_request(&request, Some(&scopes));
        assert!(result.is_err());
    }

    #[test]
    fn test_no_query_parameters() {
        let validator = create_test_validator();
        let body = json!({
            "name": "Test User",
            "age": 30
        });
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
        let scopes = vec!["read".to_string(), "write".to_string()];
        let result = validator.validate_request(&request, Some(&scopes));
        assert!(result.is_ok());
    }

    #[test]
    fn test_no_scopes_provided() {
        let validator = create_test_validator();
        let body = json!({
            "name": "Test User",
            "age": 30
        });
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
        let result = validator.validate_request(&request, None);
        assert!(result.is_ok());
    }
}
