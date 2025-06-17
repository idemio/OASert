mod request_body;
mod request_parameter;
mod scope;

use crate::traverser::OpenApiTraverser;
use crate::{OPENAPI_FIELD, REF_FIELD};
use jsonschema::{Resource, ValidationOptions, Validator as JsonValidator};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::str::FromStr;

use crate::error::{Section, SpecificationSection, ValidationErrorType};
use crate::types::json_path::JsonPath;
use crate::types::{HttpLike, OpenApiVersion, Operation, ParameterLocation};
use crate::validator::request_body::RequestBodyValidator;
use crate::validator::request_parameter::RequestParameterValidator;
use crate::validator::scope::RequestScopeValidator;
use http::HeaderMap;
use http::Request;

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
            None => todo!("OpenAPI version not found."),
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

    pub fn validate_request<T>(
        &self,
        request: &impl HttpLike<T>,
        scopes: Option<&Vec<String>>,
    ) -> Result<(), ValidationErrorType>
    where
        T: serde::ser::Serialize,
    {
        let operation = self
            .traverser
            .get_operation(request.path(), request.method().as_str())?;

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

        if let Some(query_params) = request.query() {
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

    /// Validates query parameters against an OpenAPI operation definition.
    pub fn validate_request_query_parameters(
        &self,
        operation: &Operation,
        query_params: &str,
    ) -> Result<(), ValidationErrorType> {
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
        let validator = RequestParameterValidator::new(&query_params, ParameterLocation::Query);
        validator.validate(&self.traverser, operation, &self.options)
    }

    /// Validates if the provided scopes meet the security requirements of an operation.
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
    fn validate(
        &self,
        traverser: &OpenApiTraverser,
        operation: &Operation,
        validation_options: &ValidationOptions,
    ) -> Result<(), ValidationErrorType>;

    fn section(&self) -> &Section;

    /// Validates a JSON instance against a schema referenced by a JSON path.
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

    fn complex_validation_by_schema(
        options: &ValidationOptions,
        schema: &Value,
        instance: &Value,
        section: Section,
    ) -> Result<(), ValidationErrorType> {
        let validator = Self::build_validator(options, schema)?;
        Self::do_validate(&validator, instance, section)
    }

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

        // Create headers with required header(s)
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

        // Create request without required header(s)
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

        // If query validation fails based on schema
        let result = validator.validate_request(&request, None);
        assert!(result.is_err());
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
        assert!(result.is_err());
    }

    #[test]
    fn test_no_query_parameters() {
        let validator = create_test_validator();

        // Create a valid body
        let body = json!({
            "name": "Test User",
            "age": 30
        });

        // Create a request without query parameters
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
        assert!(result.is_ok());
    }
}
