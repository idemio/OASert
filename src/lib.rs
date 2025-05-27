use dashmap::{DashMap, Entry};
use jsonschema::{Draft, Resource, ValidationOptions, Validator};
use serde_json::{Value, json};
use std::cmp::PartialEq;
use std::collections::{HashMap, HashSet};
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

pub struct OpenApiPayloadValidator {
    traverser: OpenApiTraverser,
    validator_options: ValidationOptions,
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
            validator_options: options,
        })
    }

    /// Extracts the OpenAPI version from a specification document.
    ///
    /// # Arguments
    /// * `specification` - A reference to a JSON Value containing the OpenAPI specification document
    ///
    /// # Returns
    /// * `Ok(OpenApiVersion)` - The parsed OpenAPI version (either V30x or V31x)
    /// * `Err(ValidationError)` - If the "openapi" field is missing or contains an unsupported version
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

    /// Validates a request body against a schema definition.
    ///
    /// This function performs a two-step validation process:
    /// 1. Checks that all required fields specified in the schema are present in the request body
    /// 2. If a request body is provided, performs complex schema validation on its content
    ///
    /// # Arguments
    /// * `request_body_path` - A JsonPath reference pointing to the schema definition in the specification
    /// * `request_schema` - The JSON schema to validate against, which may contain required field definitions
    /// * `request_body` - An optional JSON value representing the request body to validate
    ///
    /// # Returns
    /// * `Ok(())` - If validation succeeds
    /// * `Err(ValidationError::ValueExpected)` - If required fields are defined but, request body is missing
    /// * `Err(ValidationError::RequiredPropertyMissing)` - If a specific required field is missing
    /// * `Err(ValidationError::SchemaValidationFailed)` - If complex schema validation fails
    fn validate_body(
        &self,
        request_body_path: &JsonPath,
        request_schema: &Value,
        request_body: Option<&Value>,
    ) -> Result<(), ValidationError> {
        if let Err(e) = self.check_required_body(request_schema, request_body) {
            return Err(e);
        }

        if let Some(body) = request_body {
            return self.complex_validation(request_body_path, body);
        }

        Ok(())
    }

    /// Validates that required fields in a JSON schema are present in the request body.
    ///
    /// # Arguments
    ///
    /// * `body_schema` - A JSON schema that may contain a "required" field listing required properties
    /// * `request_body` - An optional JSON value representing the request body to validate
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If all required fields are present in the request body
    /// * `Err(ValidationError::ValueExpected)` - If required fields are defined but the request body is missing
    /// * `Err(ValidationError::RequiredPropertyMissing)` - If a specific required field is missing from the request body
    fn check_required_body(
        &self,
        body_schema: &Value,
        request_body: Option<&Value>,
    ) -> Result<(), ValidationError> {
        if let Some(required_fields) = body_schema
            .get(REQUIRED_FIELD)
            .and_then(|required| required.as_array())
        {
            if !required_fields.is_empty() && request_body.is_none() {
                return Err(ValidationError::ValueExpected("request body".to_string()));
            }

            for required in required_fields {
                let required_field = required.as_str().unwrap();

                if request_body.is_some_and(|body| body.get(required_field).is_none()) {
                    return Err(ValidationError::RequiredPropertyMissing(
                        required_field.to_string(),
                        request_body.unwrap().clone(),
                    ));
                }
            }
        }
        Ok(())
    }

    /// Validates an instance value against a JSON schema.
    ///
    /// This function checks if the provided instance conforms to the specified JSON schema
    /// by using the jsonschema crate's validation functionality.
    ///
    /// # Arguments
    ///
    /// * `schema` - A reference to a Value representing a JSON schema to validate against
    /// * `instance` - A reference to a Value representing the JSON instance to validate
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If the instance successfully validates against the schema
    /// * `Err(ValidationError::SchemaValidationFailed)` - If validation fails, containing the error message
    fn simple_validation(schema: &Value, instance: &Value) -> Result<(), ValidationError> {
        if let Err(e) = jsonschema::validate(schema, instance) {
            return Err(ValidationError::SchemaValidationFailed(e.to_string()));
        }
        Ok(())
    }

    /// Validates a JSON instance against a schema referenced by a JSON path.
    ///
    /// # Arguments
    /// * `json_path` - A reference to a JsonPath that points to a schema definition in the specification
    /// * `instance` - A reference to the JSON Value to validate against the referenced schema
    ///
    /// # Returns
    /// * `Ok(())` - If validation succeeds
    /// * `Err(ValidationError::SchemaValidationFailed)` - If schema building fails, or validation fails
    fn complex_validation(
        &self,
        json_path: &JsonPath,
        instance: &Value,
    ) -> Result<(), ValidationError> {
        let full_pointer_path = format!("@@root#/{}", json_path.format_path());
        let schema = json!({
            REF_FIELD: full_pointer_path
        });

        let validator = match self.validator_options.build(&schema) {
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

    /// Validates that all required parameters are present in the request.
    ///
    /// # Arguments
    ///
    /// * `param_schemas` - A JSON Value containing an array of parameter schemas to validate against.
    /// * `request_params` - An optional HashMap containing the actual request parameters where keys are
    ///   case-insensitive parameter names and values are the parameter values.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If all required parameters are present in the request.
    /// * `Err(ValidationError::FieldMissing)` - If a parameter schema is missing a required field.
    /// * `Err(ValidationError::RequiredParameterMissing)` - If a required parameter is missing from the request.
    fn check_required_params(
        &self,
        param_schemas: &Value,
        request_params: Option<&HashMap<UniCase<String>, String>>,
    ) -> Result<(), ValidationError> {
        if let (Some(request_params), Some(param_schemas)) =
            (request_params, param_schemas.as_array())
        {
            for param in param_schemas {
                let param_name = match param.get(NAME_FIELD).and_then(|name| name.as_str()) {
                    Some(x) => x,
                    None => {
                        return Err(ValidationError::FieldMissing(
                            NAME_FIELD.to_string(),
                            param.clone(),
                        ));
                    }
                };
                let section = match param.get("in").and_then(|name| name.as_str()) {
                    Some(x) => x,
                    None => {
                        return Err(ValidationError::FieldMissing(
                            "in".to_string(),
                            param.clone(),
                        ));
                    }
                };
                let param_required = param
                    .get(REQUIRED_FIELD)
                    .and_then(|required| required.as_bool())
                    .unwrap_or(false);
                if !request_params.contains_key(&UniCase::<String>::from(param_name))
                    && param_required
                {
                    return Err(ValidationError::RequiredParameterMissing(
                        param_name.to_string(),
                        section.to_string(),
                    ));
                }
            }
        }

        Ok(())
    }

    /// Validates parameters based on their schemas and section.
    ///
    /// This function checks if the provided request parameters conform to their respective schemas
    /// in a specified section of an OpenAPI document. It validates each parameter by:
    /// 1. Checking if the parameter belongs to the specified section
    /// 2. Verifying if required parameters are present
    /// 3. Validating parameter values against their schemas
    ///
    /// # Arguments
    ///
    /// * `param_schemas` - JSON Value containing an array of parameter schemas to validate against
    /// * `request_params` - Optional HashMap of request parameters where keys are case-insensitive parameter names
    ///                      and values are the parameter values as strings
    /// * `section` - String specifying which section of parameters to validate (e.g., "path", "query", "header")
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If all parameters pass validation
    /// * `Err(ValidationError)` - If validation fails, with specific error details:
    ///   - `ValidationError::FieldMissing` - When a required schema field is missing
    ///   - `ValidationError::RequiredParameterMissing` - When a required parameter is not provided
    ///   - `ValidationError::SchemaValidationFailed` - When a parameter value fails schema validation
    fn validate_params(
        &self,
        param_schemas: &Value,
        request_params: Option<&HashMap<UniCase<String>, String>>,
        section: &str,
    ) -> Result<(), ValidationError> {
        if let Some(param_schemas) = param_schemas.as_array() {
            for param in param_schemas {
                if param
                    .get("in")
                    .is_some_and(|param| param.as_str().is_some_and(|param| param == section))
                {
                    let is_required = param
                        .get(REQUIRED_FIELD)
                        .and_then(|req| req.as_bool())
                        .unwrap_or(false);

                    let name = match param.get(NAME_FIELD).and_then(|name| name.as_str()) {
                        Some(x) => x,
                        None => {
                            return Err(ValidationError::FieldMissing(
                                String::from(NAME_FIELD),
                                param.clone(),
                            ));
                        }
                    };

                    let schema = match param.get(SCHEMA_FIELD) {
                        Some(x) => x,
                        None => {
                            return Err(ValidationError::FieldMissing(
                                String::from(SCHEMA_FIELD),
                                param.clone(),
                            ));
                        }
                    };

                    if let Some(request_param_value) = request_params.and_then(|request_params| {
                        request_params.get(&UniCase::<String>::from(name))
                    }) {
                        if let Err(e) = Self::simple_validation(schema, &json!(request_param_value))
                        {
                            return Err(e);
                        }
                    } else if is_required {
                        return Err(ValidationError::RequiredParameterMissing(
                            name.to_string(),
                            section.to_string(),
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    /// Validates query parameters against their schema definitions.
    ///
    /// This function performs validation of query parameters in two steps:
    /// 1. Checks that all required parameters are present in the request
    /// 2. Validates that parameter values conform to their respective schemas
    ///
    /// # Arguments
    ///
    /// * `param_schemas` - A JSON Value containing an array of parameter schemas to validate against
    /// * `query_params` - An optional HashMap containing the query parameters where keys are
    ///   case-insensitive parameter names and values are the parameter values as strings
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If all query parameters pass validation
    /// * `Err(ValidationError)` - If validation fails, with specific details about the validation error
    fn validate_query_params(
        &self,
        param_schemas: &Value,
        query_params: Option<&HashMap<UniCase<String>, String>>,
    ) -> Result<(), ValidationError> {
        if let Err(e) = self.check_required_params(param_schemas, query_params) {
            return Err(e);
        }
        self.validate_params(param_schemas, query_params, "query")
    }

    /// Validates HTTP header parameters against their schema definitions.
    ///
    /// This function performs header parameter validation in two steps:
    /// 1. Checks that all required header parameters are present
    /// 2. Validates each header parameter against its schema definition
    ///
    /// # Arguments
    ///
    /// * `param_schemas` - JSON Value containing an array of parameter schemas for validation
    /// * `headers` - Optional HashMap of request headers where keys are case-insensitive header names
    ///               and values are the header values as strings
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If all header parameters pass validation
    /// * `Err(ValidationError)` - If validation fails, with specific error information such as
    ///   - Missing required parameters
    ///   - Schema validation failures
    ///   - Missing required fields in parameter definitions
    fn validate_header_params(
        &self,
        param_schemas: &Value,
        headers: Option<&HashMap<UniCase<String>, String>>,
    ) -> Result<(), ValidationError> {
        if let Err(e) = self.check_required_params(param_schemas, headers) {
            return Err(e);
        }
        self.validate_params(param_schemas, headers, "header")
    }

    fn extract_content_type(headers: Option<&HashMap<UniCase<String>, String>>) -> Option<&str> {
        if let Some(headers) =
            headers.and_then(|headers| headers.get(&UniCase::from("content-type")))
        {
            return Some(headers);
        }

        None
    }

    /// # Validates a request body against an OpenAPI schema
    ///
    /// This function validates a request body against the schema defined in an OpenAPI specification.
    /// It extracts the content type from the request headers, locates the corresponding schema
    /// in the OpenAPI spec, and performs validation against that schema.
    ///
    /// ## Arguments
    ///
    /// * `operation_and_path` - A tuple containing:
    ///   * A reference to a JSON Value representing an OpenAPI operation object
    ///   * A JsonPath object used to track the path within the OpenAPI spec
    /// * `body` - The JSON Value representing the request body to validate
    /// * `headers` - A HashMap of request headers where keys are case-insensitive header names
    ///
    /// ## Returns
    ///
    /// * `Ok(())` - If the request body is valid, according to the schema
    /// * `Err(ValidationError::RequiredParameterMissing)` - If the content-type header is missing
    /// * `Err(ValidationError::DefinitionExpected)` - If the schema for the request body is missing
    /// * `Err(ValidationError)` - Other validation errors (missing required properties, schema validation failures)
    ///
    /// ## Implementation Details
    ///
    /// 1. Extracts the content type from the request headers
    /// 2. Builds a path to the schema for the given content type
    /// 3. Traverses the OpenAPI spec to find the request body schema:
    ///    - Gets the optional request body object
    ///    - Gets the required content object
    ///    - Gets the required media type object for the content type
    ///    - Gets the optional schema from media-type
    /// 4. Validates the request body against the schema:
    ///    - Checks that required properties are present
    ///    - Performs complex schema validation
    /// 5. Returns an error if any step of the validation fails
    pub fn validate_request_body(
        &self,
        operation_and_path: (&Value, JsonPath),
        body: &Value,
        headers: &HashMap<UniCase<String>, String>,
    ) -> Result<(), ValidationError> {
        let (operation, mut path) = operation_and_path;
        if let Some(content_type) = Self::extract_content_type(Some(headers)) {
            path.add_segment(REQUEST_BODY_FIELD)
                .add_segment(CONTENT_FIELD)
                .add_segment(content_type)
                .add_segment(SCHEMA_FIELD);

            let request_body_schema = match self
                .traverser
                .get_optional_spec_node(&operation, REQUEST_BODY_FIELD)?
            {
                None => return Ok(()),
                Some(val) => val,
            };

            let content_schema = match self
                .traverser
                .get_required_spec_node(request_body_schema.value(), CONTENT_FIELD)
            {
                Ok(val) => val,
                Err(e) => return Err(e),
            };

            let media_type = self
                .traverser
                .get_required_spec_node(content_schema.value(), content_type)?;
            let request_media_type_schema = self
                .traverser
                .get_optional_spec_node(media_type.value(), SCHEMA_FIELD)?;

            match request_media_type_schema {
                Some(request_body_schema) => {
                    if let Err(e) =
                        self.check_required_body(request_body_schema.value(), Some(body))
                    {
                        return Err(e);
                    }
                    self.complex_validation(&path, body)
                }

                None => Err(ValidationError::DefinitionExpected(
                    "request body".to_string(),
                )),
            }
        } else {
            Err(ValidationError::RequiredParameterMissing(
                "content-type".to_string(),
                "header".to_string(),
            ))
        }
    }

    /// # Validates an HTTP request against an OpenAPI specification
    ///
    /// Verifies that an incoming HTTP request conforms to the requirements
    /// defined in an OpenAPI specification by validating:
    /// - Request body against the schema definition
    /// - Request headers against parameter specifications
    /// - Query parameters against parameter specifications
    ///
    /// ## Arguments
    ///
    /// * `path` - The URL path of the request to validate
    /// * `method` - The HTTP method of the request (GET, POST, etc.)
    /// * `body` - Optional JSON body content of the request
    /// * `headers` - Optional map of request headers, case-insensitive
    /// * `query_params` - Optional map of query parameters, case-insensitive
    ///
    /// ## Returns
    ///
    /// * `Ok(())` - If the request is valid, according to the specification
    /// * `Err(ValidationError)` - If validation fails, with details about the specific error:
    ///   - `ValidationError::MissingOperation` - If the path/method combination is not found
    ///   - `ValidationError::DefinitionExpected` - If required schema definitions are missing
    ///   - `ValidationError::RequiredParameterMissing` - If a required parameter is not provided
    ///   - Other validation errors as defined in the `ValidationError` enum
    ///
    /// ## Example
    ///
    /// ```rust
    /// use std::collections::HashMap;
    /// use std::fs;
    /// use serde_json::Value;
    /// use unicase::UniCase;
    /// use crate::oasert::OpenApiPayloadValidator;
    ///
    /// let spec_string = fs::read_to_string("./test/openapi-v3.0.2.json").unwrap();
    /// let specification: Value = serde_json::from_str(&spec_string).unwrap();
    /// let validator = OpenApiPayloadValidator::new(specification).unwrap();
    /// let headers = Some(HashMap::from([
    ///     (UniCase::from("content-type".to_string()), "application/json".to_string())
    /// ]));
    /// let body = Some(serde_json::json!({"name": "Test User", "email": "test@example.com"}));
    ///
    /// match validator.validate_request("/users", "POST", body.as_ref(), headers.as_ref(), None) {
    ///     Ok(()) => println!("Request is valid!"),
    ///     Err(e) => println!("Validation error: {:?}", e),
    /// }
    /// ```
    ///
    /// ## Implementation Details
    ///
    /// The function follows these steps:
    /// 1. Retrieves the operation object from the OpenAPI spec using `path` and `method`
    /// 2. If Content-Type header is present:
    ///    - Builds a path to the schema for the specified content type
    ///    - Retrieves and validates the request body schema if it exists
    /// 3. Validates all header parameters against the spec:
    ///    - Checks that required headers are present
    ///    - Validates header values against their schemas
    /// 4. Validates all query parameters against the spec:
    ///    - Checks that required query parameters are present
    ///    - Validates query parameter values against their schemas
    ///
    /// The validation logic handles the case where elements are optional in both
    /// the request and the specification.
    pub fn validate_request(
        &self,
        path: &str,
        method: &str,
        body: Option<&Value>,
        headers: Option<&HashMap<UniCase<String>, String>>,
        query_params: Option<&HashMap<UniCase<String>, String>>,
    ) -> Result<(), ValidationError> {
        let (operation, mut path) = match self.traverser.get_operation(path, method) {
            Err(e) => return Err(e),
            Ok(val) => (val.0, val.1),
        };

        let result: Result<(), ValidationError> = {
            if let Some(content_type) = Self::extract_content_type(headers) {
                path.add_segment(REQUEST_BODY_FIELD)
                    .add_segment(CONTENT_FIELD)
                    .add_segment(content_type)
                    .add_segment(SCHEMA_FIELD);

                let request_body_schema = match self
                    .traverser
                    .get_optional_spec_node(&operation, REQUEST_BODY_FIELD)?
                {
                    None => return Ok(()),
                    Some(val) => val,
                };

                let content_schema = match self
                    .traverser
                    .get_required_spec_node(request_body_schema.value(), CONTENT_FIELD)
                {
                    Ok(val) => val,
                    Err(e) => return Err(e),
                };

                let media_type = self
                    .traverser
                    .get_required_spec_node(content_schema.value(), content_type)?;
                let request_media_type_schema = self
                    .traverser
                    .get_optional_spec_node(media_type.value(), SCHEMA_FIELD)?;

                return match (body.is_some(), request_media_type_schema) {
                    (true, Some(request_body_schema)) => {
                        self.validate_body(&path, &request_body_schema.value(), body)
                    }

                    (true, None) => Err(ValidationError::DefinitionExpected(
                        "request body".to_string(),
                    )),

                    (_, _) => Ok(()),
                };
            }
            Ok(())
        };

        if let Err(e) = result {
            return Err(e);
        }

        let spec_parameters = self.traverser.get_request_parameters(&operation)?;

        if let Err(e) = match (headers.is_some(), &spec_parameters) {
            // if we have header params in our request and the spec contains params, do the validation.
            (true, Some(request_params)) => {
                self.validate_header_params(&request_params.value(), headers)
            }

            // If no header params were provided and the spec contains params,
            // check to see if there are any required header params.
            (false, Some(request_params)) => {
                self.check_required_params(&request_params.value(), None)
            }

            // passthrough
            (_, _) => Ok(()),
        } {
            return Err(e);
        }

        if let Err(e) = match (query_params.is_some(), &spec_parameters) {
            // If we have query params in our request and the spec contains params, do the validation.
            (true, Some(request_params)) => {
                self.validate_query_params(&request_params.value(), query_params)
            }

            // If no query params were provided and the spec contains params,
            // check to see if there are any required query params.
            (false, Some(request_params)) => {
                self.check_required_params(&request_params.value(), None)
            }

            // If query params were provided, but the spec contains no param definitions, raise a validation error.
            (true, None) => Err(ValidationError::DefinitionExpected(
                "query parameters".to_string(),
            )),

            // passthrough
            (_, _) => Ok(()),
        } {
            return Err(e);
        }

        Ok(())
    }
}

type TraverseResult<'a> = Result<SearchResult<'a>, ValidationError>;

pub enum SearchResult<'a> {
    Arc(Arc<Value>),
    Ref(&'a Value),
}

impl<'a> SearchResult<'a> {
    fn value(&'a self) -> &'a Value {
        match self {
            SearchResult::Arc(arc_val) => arc_val,
            SearchResult::Ref(val) => val,
        }
    }
}

struct OpenApiTraverser {
    specification: Value,
    resolved_references: DashMap<String, Arc<Value>>,
}

impl OpenApiTraverser {
    fn new(specification: Value) -> Self {
        Self {
            specification,
            resolved_references: DashMap::new(),
        }
    }

    /// Looks up an OpenAPI operation based on a request path and method.
    ///
    /// # Arguments
    ///
    /// * `request_path` - The actual request path to match against OpenAPI specification paths
    /// * `request_method` - The HTTP method of the request (e.g., "get", "post", "put")
    ///
    /// # Returns
    ///
    /// * `Ok((Arc<Value>, JsonPath))` - A tuple containing:
    ///   - The operation definition as an Arc<Value>
    ///   - A JsonPath pointing to the operation in the specification
    /// * `Err(ValidationError)` - Returns a ValidationError if:
    ///   - The operation is not found for the given path and method
    ///   - The path methods in the specification are not in the expected object format
    fn get_operation(
        &self,
        request_path: &str,
        request_method: &str,
    ) -> Result<(Arc<Value>, JsonPath), ValidationError> {
        log::debug!("Looking for path '{request_path}' and method '{request_method}'");

        // Grab all paths from the spec
        if let Ok(spec_paths) = self.get_paths() {
            // For each path there are 1 to n methods.
            if let Some(spec_paths) = spec_paths.value().as_object() {
                for (spec_path, spec_path_methods) in spec_paths.iter() {
                    let operations = match spec_path_methods.as_object() {
                        Some(x) => x,
                        None => {
                            return Err(ValidationError::UnexpectedType(
                                spec_path.to_string(),
                                "object",
                                spec_path_methods.clone(),
                            ));
                        }
                    };

                    // Grab the operation matching our request method and test to see if the path matches our request path.
                    // If both method and path match, then we've found the operation associated with the request.
                    if let Some(operation) = operations.get(request_method) {
                        if Self::matches_spec_path(request_path, spec_path) {
                            log::debug!(
                                "OpenAPI path '{spec_path}' and method '{request_method}' match provided request path '{request_path}' and method '{request_method}'."
                            );
                            let mut json_path = JsonPath::new();
                            json_path
                                .add_segment(PATHS_FIELD)
                                .add_segment(spec_path)
                                .add_segment(&request_method.to_lowercase());
                            return Ok((Arc::new(operation.clone()), json_path));
                        }
                    }
                }
            }
        }
        Err(ValidationError::MissingOperation(
            request_path.to_string(),
            request_method.to_string(),
        ))
    }

    /// Determines if a given `path` matches an OpenAPI `specification` `path` pattern.
    ///
    /// This function checks if a request path matches a `specification` path, handling path parameters
    /// enclosed in curly braces (e.g., "/users/{id}").
    ///
    /// # Arguments
    ///
    /// * `path_to_match` - The actual request path to check against the `specification`.
    /// * `spec_path` - The `specification` path pattern that may contain path parameters in the format "{param_name}".
    ///
    /// # Returns
    ///
    /// * `true` if the path matches the `specification` pattern, accounting for path parameters.
    /// * `false` if the path does not match the pattern or has a different number of segments.
    fn matches_spec_path(path_to_match: &str, spec_path: &str) -> bool {
        // If the spec path we are checking contains no path parameters,
        // then we can simply compare path strings.
        if !(spec_path.contains("{") && spec_path.contains("}")) {
            spec_path == path_to_match

        // if the request path contains path parameters, we need to compare each segment
        // When we reach a segment that is a parameter, compare the value in the path to the value in the spec.
        } else {
            let target_segments = path_to_match.split(PATH_SEPARATOR).collect::<Vec<&str>>();
            let spec_segments = spec_path.split(PATH_SEPARATOR).collect::<Vec<&str>>();

            if spec_segments.len() != target_segments.len() {
                return false;
            }

            let (matching_segments, segment_count) =
                spec_segments.iter().zip(target_segments.iter()).fold(
                    (0, 0),
                    |(mut matches, mut count), (spec_segment, target_segment)| {
                        count += 1;
                        if let Some(_) = spec_segment.find("{").and_then(|start| {
                            spec_segment
                                .find("}")
                                .map(|end| &spec_segment[start + 1..end])
                        }) {
                            // assume the path param type matches
                            matches += 1;
                        } else if spec_segment == target_segment {
                            matches += 1;
                        }

                        (matches, count)
                    },
                );

            matching_segments == segment_count
        }
    }

    fn get_paths(&self) -> TraverseResult {
        self.get_required_spec_node(&self.specification, PATHS_FIELD)
    }

    /// Retrieves the `parameters` field from an operation object in an OpenAPI specification.
    ///
    /// This function attempts to extract the `parameters` field from the provided operation JSON value,
    /// treating a missing `parameters` field as a valid case (returns None) rather than an error.
    ///
    /// # Arguments
    /// * `operation` - A reference to a JSON Value representing an operation object from which
    ///   to extract the `parameters` field
    ///
    /// # Returns
    ///
    /// * `Ok(Some(SearchResult))` - If the `parameters` field exists, returns a wrapped reference to the field value, either as an owned `Arc<Value>` or a borrowed reference
    /// * `Ok(None)` - If the `parameters` field doesn't exist in the operation object
    /// * `Err(ValidationError)` - If an error occurs during validation not related to a missing `parameter` field
    fn get_request_parameters<'a>(
        &'a self,
        operation: &'a Value,
    ) -> Result<Option<SearchResult<'a>>, ValidationError> {
        match match self.get_optional_spec_node(operation, PARAMETERS_FIELD) {
            Ok(res) => res,
            Err(e) if e.kind() == ValidationErrorKind::MismatchingSchema => return Ok(None),
            Err(e) => return Err(e),
        } {
            None => Ok(None),
            Some(val) => Ok(Some(val)),
        }
    }

    /// Retrieves the security requirements specified for an operation in an OpenAPI specification.
    ///
    /// # Arguments
    /// * `operation` - A JSON value representing an operation object from which to extract the security field
    ///
    /// # Returns
    ///
    /// * `Ok(Some(SearchResult))` - If the security field exists in the operation, contains a reference to its value
    /// * `Ok(None)` - If the security field doesn't exist in the operation
    /// * `Err(ValidationError)` - If an error occurs during retrieval (other than the field being missing)
    fn get_request_security<'a>(
        &'a self,
        operation: &'a Value,
    ) -> Result<Option<SearchResult<'a>>, ValidationError> {
        match self.get_optional_spec_node(operation, SECURITY_FIELD)? {
            None => Ok(None),
            Some(val) => Ok(Some(val)),
        }
    }

    /// Retrieves an optional field from a JSON value in an OpenAPI specification.
    ///
    /// This function attempts to get a specified field from a JSON operation object,
    /// but unlike `get_required_spec_node`, it treats missing fields as valid
    /// (returns None) rather than errors.
    ///
    /// # Arguments
    /// * `operation` - The JSON value (typically an operation object) to search within
    /// * `field` - The name of the optional field to extract
    ///
    /// # Returns
    ///
    /// * `Ok(Some(SearchResult))` - If the field exists, returns a wrapped reference
    ///   to the field value, either as an owned `Arc<Value>` or a borrowed reference
    /// * `Ok(None)` - If the specified field doesn't exist in the value
    /// * `Err(ValidationError)` - For any error other than a missing field
    fn get_optional_spec_node<'a>(
        &'a self,
        operation: &'a Value,
        field: &str,
    ) -> Result<Option<SearchResult<'a>>, ValidationError> {
        match self.get_required_spec_node(operation, field) {
            Ok(security) => Ok(Some(security)),
            Err(e) if e.kind() == ValidationErrorKind::MismatchingSchema => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Attempts to retrieve a required field from a JSON value, following any references if present.
    ///
    /// # Arguments
    /// * `value` - The JSON value to search within
    /// * `field` - The name of the required field to extract
    ///
    /// # Returns
    /// * `Ok(SearchResult)` - A wrapped reference to the requested field value, either as an owned `Arc<Value>`
    ///   or a borrowed reference
    /// * `Err(ValidationError::FieldMissing)` - If the specified field doesn't exist in the value
    fn get_required_spec_node<'a>(
        &'a self,
        value: &'a Value,
        field: &str,
    ) -> Result<SearchResult<'a>, ValidationError> {
        let ref_result = self.resolve_possible_ref(value)?;

        match ref_result {
            SearchResult::Arc(arc_value) => match arc_value.get(field) {
                None => Err(ValidationError::FieldMissing(
                    field.to_string(),
                    value.clone(),
                )),

                // Any way to avoid a clone here?
                Some(val) => Ok(SearchResult::Arc(Arc::new(val.clone()))),
            },
            SearchResult::Ref(ref_value) => match ref_value.get(field) {
                None => Err(ValidationError::FieldMissing(
                    field.to_string(),
                    value.clone(),
                )),
                Some(val) => Ok(SearchResult::Ref(val)),
            },
        }
    }

    /// Resolves a JSON node that might contain a reference (via "$ref" field).
    ///
    /// # Arguments
    /// * `self` - The OpenApiTraverser instance that contains the reference resolution context
    /// * `node` - The JSON value that might contain a reference to resolve
    ///
    /// # Returns
    /// * `Ok(SearchResult::Arc)` - If the node contains a reference that has been previously resolved
    /// * `Ok(SearchResult::Ref)` - If the node does not contain a reference
    /// * `Err(ValidationError)` - If reference resolution fails (e.g., circular reference or missing field)
    fn resolve_possible_ref<'a>(&'a self, node: &'a Value) -> TraverseResult<'a> {
        // If the ref node exists, resolve it
        if let Some(ref_string) = node.get(REF_FIELD).and_then(|val| val.as_str()) {
            let entry = self.resolved_references.entry(String::from(ref_string));
            return match entry {
                Entry::Occupied(e) => Ok(SearchResult::Arc(e.get().clone())),
                Entry::Vacant(_) => {
                    let mut seen_references = HashSet::new();
                    let res = self.get_reference_path(ref_string, &mut seen_references)?;
                    return Ok(res);
                }
            };
        }

        Ok(SearchResult::Ref(node))
    }

    /// Resolves a reference string by navigating through the specification object to find the referenced schema.
    ///
    /// # Arguments
    /// * `ref_string` - A string containing a JSON reference path (e.g., "#/components/schemas/Pet")
    /// * `seen_references` - A mutable HashSet tracking references already encountered to detect circular references
    ///
    /// # Returns
    /// * `Ok(SearchResult)` - The resolved schema if the reference was successfully resolved
    /// * `Err(ValidationError::CircularReference)` - If a circular reference is detected
    /// * `Err(ValidationError::FieldMissing)` - If a path segment cannot be found in the specification
    fn get_reference_path(
        &self,
        ref_string: &str,
        seen_references: &mut HashSet<String>,
    ) -> TraverseResult {
        if seen_references.contains(ref_string) {
            return Err(ValidationError::CircularReference(
                seen_references.len(),
                String::from(ref_string),
            ));
        }
        seen_references.insert(String::from(ref_string));
        let path = ref_string
            .split(PATH_SEPARATOR)
            .filter(|node| !(*node).is_empty() && (*node != "#"))
            .collect::<Vec<&str>>();
        let mut current_schema = &self.specification;
        for segment in path {
            let refactored_segment = segment.replace(ENCODED_BACKSLASH, PATH_SEPARATOR);
            // Navigate to the next segment
            match current_schema.get(refactored_segment) {
                Some(next) => {
                    current_schema = next;
                }
                None => {
                    return Err(ValidationError::FieldMissing(
                        String::from(segment),
                        current_schema.clone(),
                    ));
                }
            }
        }
        let current_schema = self.resolve_possible_ref(current_schema)?;
        Ok(current_schema)
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
    use crate::{JsonPath, NAME_FIELD, OpenApiPayloadValidator, ValidationError};
    use memory_stats::memory_stats;
    use serde_json::{Value, json};
    use std::collections::HashMap;
    use std::fs;
    use std::sync::Arc;
    use unicase::UniCase;

    fn print_memory() {
        if let Some(usage) = memory_stats() {
            println!("Current physical memory usage: {}", usage.physical_mem);
            println!("Current virtual memory usage: {}", usage.virtual_mem);
        } else {
            println!("Couldn't get the current memory usage :(");
        }
    }

    #[test]
    fn test_find_operation() {
        let spec_string = fs::read_to_string("./test/openapi-v3.0.2.json").unwrap();
        let specification: Value = serde_json::from_str(&spec_string).unwrap();
        let validator = OpenApiPayloadValidator::new(specification).unwrap();
        print_memory();
        {
            let result = validator
                .traverser
                .get_operation("/pet/findByStatus/MultipleExamples", "get");
            assert!(result.is_ok());
            let result = result.unwrap();
            assert_eq!(
                "paths/~1pet~1findByStatus~1MultipleExamples/get",
                result.1.format_path()
            );
        }
        print_memory();
        {
            let result = validator
                .traverser
                .get_operation("/pet/findById/123", "get");
            assert!(result.is_ok());
            let result = result.unwrap();
            assert_eq!(
                "paths/~1pet~1findById~1{pet_id}/get",
                result.1.format_path()
            );
        }
        print_memory();
    }

    #[test]
    fn test_find_request_body() {
        let spec_string = fs::read_to_string("./test/openapi-v3.0.2.json").unwrap();
        let specification: Value = serde_json::from_str(&spec_string).unwrap();
        let validator = OpenApiPayloadValidator::new(specification).unwrap();
        let result: (Arc<Value>, JsonPath) =
            validator.traverser.get_operation("/pet", "post").unwrap();
        let operation = result.0.clone();
        assert!(operation.get("requestBody").is_some());
    }

    #[test]
    fn test_validate_wild_request() {
        let spec_string = fs::read_to_string("./test/wild-openapi-spec.json").unwrap();
        let specification: Value = serde_json::from_str(&spec_string).unwrap();
        let validator = OpenApiPayloadValidator::new(specification).unwrap();
        let example_request = json!({
          "layerTwo": {
            "layerThree": {
              "data": {
                "customField1": "value1",
                "customField2": 42
              }
            }
          }
        });
        let mut example_headers: HashMap<UniCase<String>, String> = HashMap::new();
        example_headers.insert(
            UniCase::from("Content-Type"),
            "application/json".to_string(),
        );
        let path = "/pet";
        let method = "post";

        for x in 0..25 {
            println!("it: {x}");
            print_memory();

            let result = validator.validate_request(
                path,
                method,
                Some(&example_request),
                Some(&example_headers),
                None,
            );
            match result {
                Ok(_) => assert!(true, "Body is valid"),
                Err(e) => {
                    println!("{e:#?}");
                    assert!(false, "Body should be valid")
                }
            }
        }
    }

    #[test]
    fn test_validate_request() {
        let spec_string = fs::read_to_string("./test/openapi-v3.0.2.json").unwrap();
        let specification: Value = serde_json::from_str(&spec_string).unwrap();
        let validator = OpenApiPayloadValidator::new(specification).unwrap();
        let example_request_body = json!({
            "name": "Ruby",
            "age": 5,
            "hunts": true,
            "breed": "Bengal"
        });

        let mut example_headers: HashMap<UniCase<String>, String> = HashMap::new();
        example_headers.insert(
            UniCase::from("Content-Type"),
            "application/json".to_string(),
        );
        let path = "/pet";
        let method = "post";
        let result = validator.validate_request(
            path,
            method,
            Some(&example_request_body),
            Some(&example_headers),
            None,
        );

        match result {
            Ok(_) => assert!(true, "Body is valid"),
            Err(e) => {
                println!("{e:#?}");
                assert!(false, "Body should be valid")
            }
        }

        let example_request_body = json!({
            "name": "Ruby",
            "age": 5,
            "hunts": true,
            "invalid_field": "some incorrect data"
        });
        let result = validator.validate_request(
            path,
            method,
            Some(&example_request_body),
            Some(&example_headers),
            None,
        );
        match result {
            Ok(_) => assert!(false, "Body should not be valid"),
            Err(e) => {
                println!("{e:#?}");
                assert!(true, "Body is invalid")
            }
        }
    }

    #[test]
    fn test_check_required_params() {
        let spec_string = fs::read_to_string("./test/openapi-v3.0.2.json").unwrap();
        let specification: Value = serde_json::from_str(&spec_string).unwrap();
        let validator = OpenApiPayloadValidator::new(specification).unwrap();

        // Test case 1: No parameters should pass
        let empty_params = json!([]);
        let empty_headers: Option<&HashMap<UniCase<String>, String>> = None;
        assert!(
            validator
                .check_required_params(&empty_params, empty_headers)
                .is_ok()
        );

        // Test case 2: Required parameter is present - should pass
        let param_schema = json!([
            {
                "name": "api-key",
                "in": "header",
                "required": true
            }
        ]);

        let mut headers = HashMap::new();
        headers.insert(UniCase::from("api-key".to_string()), "abc123".to_string());

        assert!(
            validator
                .check_required_params(&param_schema, Some(&headers))
                .is_ok()
        );

        // Test case 3: Required parameter is missing - should fail
        let param_schema = json!([
            {
                "name": "api-key",
                "in": "header",
                "required": true
            }
        ]);

        let empty_headers = HashMap::new();

        let result = validator.check_required_params(&param_schema, Some(&empty_headers));
        assert!(result.is_err());
        if let Err(ValidationError::RequiredParameterMissing(param_name, section)) = result {
            assert_eq!(param_name, "api-key");
            assert_eq!(section, "header");
        } else {
            panic!("Expected RequiredParameterMissing error");
        }

        // Test case 4: Optional parameter is missing - should pass
        let param_schema = json!([
            {
                "name": "optional-param",
                "in": "header",
                "required": false
            }
        ]);

        let empty_headers = HashMap::new();

        assert!(
            validator
                .check_required_params(&param_schema, Some(&empty_headers))
                .is_ok()
        );

        // Test case 5: Parameter schema missing 'name' field - should fail
        let param_schema = json!([
            {
                "in": "header",
                "required": true
            }
        ]);

        let headers = HashMap::new();

        let result = validator.check_required_params(&param_schema, Some(&headers));
        assert!(result.is_err());
        if let Err(ValidationError::FieldMissing(field, _)) = result {
            assert_eq!(field, NAME_FIELD);
        } else {
            panic!("Expected FieldMissing error for name field");
        }

        // Test case 6: Parameter schema missing 'in' field - should fail
        let param_schema = json!([
            {
                "name": "api-key",
                "required": true
            }
        ]);

        let headers = HashMap::new();

        let result = validator.check_required_params(&param_schema, Some(&headers));
        assert!(result.is_err());
        if let Err(ValidationError::FieldMissing(field, _)) = result {
            assert_eq!(field, "in");
        } else {
            panic!("Expected FieldMissing error for 'in' field");
        }

        // Test case 7: Multiple parameters - some required, some optional
        let param_schema = json!([
            {
                "name": "api-key",
                "in": "header",
                "required": true
            },
            {
                "name": "content-type",
                "in": "header",
                "required": true
            },
            {
                "name": "optional-param",
                "in": "header",
                "required": false
            }
        ]);

        let mut headers = HashMap::new();
        headers.insert(UniCase::from("api-key".to_string()), "abc123".to_string());
        headers.insert(
            UniCase::from("content-type".to_string()),
            "application/json".to_string(),
        );

        assert!(
            validator
                .check_required_params(&param_schema, Some(&headers))
                .is_ok()
        );

        // Test case 8: Parameter with default required value (no required field)
        let param_schema = json!([
            {
                "name": "default-optional",
                "in": "header"
                // No required field - should default to false
            }
        ]);

        let empty_headers = HashMap::new();

        assert!(
            validator
                .check_required_params(&param_schema, Some(&empty_headers))
                .is_ok()
        );

        // Test case 9: Non-array parameters should be handled gracefully
        let non_array_params = json!({
            "name": "api-key",
            "in": "header",
            "required": true
        });

        let headers = HashMap::new();

        assert!(
            validator
                .check_required_params(&non_array_params, Some(&headers))
                .is_ok()
        );
    }
}
