pub mod traverser;
pub mod types;

use crate::traverser::{OpenApiTraverser, SearchResult};
use crate::types::{OpenApiVersion, Operation, ParameterLocation, RequestParamData};
use jsonschema::{Resource, ValidationOptions, Validator};
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
                return Err(ValidationError::SchemaValidationFailed(e.to_string()));
            }
        };

        // Assign draft and provide resource
        let options = Validator::options()
            .with_draft(draft)
            .with_resource("@@inner", resource);

        Ok(Self {
            traverser: OpenApiTraverser::new(value),
            options,
        })
    }

    /// Validates that the required fields specified in the `body_schema` are present in the `request_body`.
    ///
    /// # Arguments
    ///
    /// * `body_schema` - A reference to a JSON value representing the schema definition for the body,
    ///   which may include a "required" array specifying mandatory fields.
    /// * `request_body` - An optional reference to a JSON value representing the request body provided by the client.
    ///   Can be `None` if no `body` is included in the request.
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
        if let Ok(required_fields) = traverser::get_as_array(body_schema, REQUIRED_FIELD) {
            // if the body provided is empty and required fields are present, then it's an invalid body.
            if !required_fields.is_empty() && request_body.is_none() {
                return Err(ValidationError::ValueExpected("request body".to_string()));
            }

            if let Some(body) = request_body {
                for required in required_fields {
                    let required_field = traverser::require_str(required)?;

                    // if the current required field is not present in the body, then it's a failure.
                    if body.get(required_field).is_none() {
                        return Err(ValidationError::RequiredPropertyMissing(
                            required_field.to_string(),
                            request_body.unwrap_or(&json!("null")).clone(),
                        ));
                    }
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
    /// - `Ok(())`: If the instance is valid, according to the schema.
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
            // the parameter should have the `name` field defined.
            let param_name = traverser::get_as_str(param, NAME_FIELD)?;

            // the parameter should have the `in` field defined.
            let section = traverser::get_as_str(param, IN_FIELD)?;

            // if the parameter has 'required' field use that, otherwise false.
            let param_required = traverser::get_as_bool(param, REQUIRED_FIELD).unwrap_or(false);

            // check to see if the required parameter is present in our request.
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
        param_definitions: &Vec<Value>,
        request_params: &HashMap<UniCase<String>, String>,
        section: ParameterLocation,
    ) -> Result<(), ValidationError> {
        for param_definition in param_definitions {
            // Only look at parameters that match the current section.
            if traverser::get_as_str(param_definition, IN_FIELD)
                .is_ok_and(|v| v == section.to_string())
            {
                let name = traverser::get_as_str(param_definition, NAME_FIELD)?;
                let schema = traverser::get_as_any(param_definition, SCHEMA_FIELD)?;

                if let Some(request_param_value) =
                    request_params.get(&UniCase::<String>::from(name))
                {
                    Self::simple_validation(schema, &json!(request_param_value))?
                }
            }
        }
        Ok(())
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
        headers: &impl RequestParamData,
    ) -> Result<(), ValidationError> {
        let headers = headers.get();
        let (operation, mut path) = (&operation.data, operation.path.clone());
        if let Some(content_type) = Self::extract_content_type(&headers) {
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
                path.add(REQUEST_BODY_FIELD)
                    .add(CONTENT_FIELD)
                    .add(&content_type)
                    .add(SCHEMA_FIELD);
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
    
    
    /// Retrieves the parameters from a given OpenAPI operation definition.
    ///
    /// This function attempts to retrieve the `parameters` field from the given
    /// operation. If the field exists, it wraps the result in a `SearchResult` enum. 
    /// If the field is missing but the schema is valid, it returns `Ok(None)`. 
    /// If any other error occurs, it returns the corresponding `ValidationError`.
    ///
    /// # Arguments
    ///
    /// * `self` - A reference to the `OpenApiPayloadValidator` instance, which contains
    ///   the traverser used for fetching the parameters.
    /// * `operation` - A reference to an `Operation` representing the OpenAPI operation being processed. 
    ///   This operation contains the data and path needed for the lookup.
    ///
    /// # Returns
    ///
    /// * `Ok(Some(SearchResult))` - If the `parameters` field is successfully retrieved,
    ///   returns a wrapped reference to the field value, either as an owned `Arc<Value>` 
    ///   or a borrowed reference.
    /// * `Ok(None)` - If the `parameters` field does not exist but the schema is considered valid.
    /// * `Err(ValidationError)` - If an error occurs while trying to retrieve the `parameters` field.
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
    
    
    /// Validates the headers of a request against the operation's parameter requirements.
    ///
    /// # Arguments
    /// * `operation` - Reference to an `Operation` struct, representing the API operation being validated.
    /// * `headers` - A reference to an implementation of the `RequestParamData` trait that provides 
    ///   access to the request headers as a `HashMap`.
    ///
    /// # Returns
    /// * `Ok(())` - If the headers are successfully validated against the operation's parameter requirements.
    /// * `Err(ValidationError)` - If headers fail validation, such as missing required parameters or other constraints.
    pub fn validate_request_header_params(
        &self,
        operation: &Operation,
        headers: &impl RequestParamData,
    ) -> Result<(), ValidationError> {
        let headers = headers.get();
        let parameters = self.get_parameters(operation)?;
        self.validate_request_params(parameters, &headers, ParameterLocation::Header)
    }

    
    /// Validates the query parameters of a request against the operation's defined
    /// query parameter schema.
    ///
    /// # Arguments
    /// - `operation`: Reference to the OpenAPI operation definition whose query
    ///   parameters need to be validated. This contains the operation's metadata in
    ///   a structured form.
    /// - `query_params`: A reference to an object implementing the `RequestParamData`
    ///   trait, which provides the actual query parameters from the request as a key-value
    ///   mapping.
    ///
    /// # Returns
    /// - `Ok(())`: If the validation succeeds and all query parameters conform to the
    ///   operation's schema.
    /// - `Err(ValidationError)`: If the query parameters are invalid or required parameters
    ///   are missing. The specific variant of `ValidationError` provides more details about
    ///   the validation failure.
    pub fn validate_request_query_parameters(
        &self,
        operation: &Operation,
        query_params: &impl RequestParamData,
    ) -> Result<(), ValidationError> {
        let query_params = query_params.get();
        let parameters = self.get_parameters(operation)?;
        self.validate_request_params(parameters, &query_params, ParameterLocation::Query)
    }
    
    
    /// Validates the provided request parameters against the defined schema.
    ///
    /// # Arguments
    ///
    /// - `schema_parameters`: An optional `SearchResult` containing the schema definitions for the
    ///   expected parameters. If `None`, no validation is performed.
    /// - `request_parameters`: A reference to a `HashMap` with case-insensitive keys (`UniCase<String>`)
    ///   representing parameter names and their associated string values.
    /// - `parameter_location`: Specifies the location of the parameters (e.g., header, query, path, or cookie),
    ///   as an instance of `ParameterLocation`.
    ///
    /// # Returns
    ///
    /// - `Ok(())`: If all required parameters are present and valid according to the schema.
    /// - `Err(ValidationError)`: If validation fails, specific validation errors may include:
    ///     - `ValidationError::RequiredParameterMissing`: If a required parameter is missing from the request.
    ///     - `ValidationError::UnexpectedType`: If an expected array is not provided in the schema definition.
    ///     - `ValidationError::SchemaValidationFailed`: If a provided parameter value does not comply with its schema.
    fn validate_request_params(
        &self,
        schema_parameters: Option<SearchResult>,
        request_parameters: &HashMap<UniCase<String>, String>,
        parameter_location: ParameterLocation,
    ) -> Result<(), ValidationError> {
        match schema_parameters {
            Some(request_params) => {
                let request_params = traverser::require_array(request_params.value())?;
                self.check_required_params(request_params, request_parameters)?;
                self.validate_params(request_params, request_parameters, parameter_location)
            }
            None => Ok(()),
        }
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
    UnexpectedType(&'static str, Value),
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
            ValidationError::UnexpectedType(expected, node) => {
                write!(
                    f,
                    "UnexpectedType: Expected '{expected}' but '{node}' was found"
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
            | ValidationError::UnexpectedType(_, _)
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
    use crate::types::{Operation, RequestParamData};
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
        let result = validator.validate_request_body(&operation, None, &headers_struct);
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
        let result = validator.validate_request_body(&operation, Some(&body), &headers_struct);
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
        let result = validator.validate_request_body(&operation, None, &headers_struct);
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
        let result = validator.validate_request_body(&operation, Some(&body), &headers_struct);
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
        let headers_struct = TestParamStruct { data: headers };
        let body = json!({});
        let result = validator.validate_request_body(&operation, Some(&body), &headers_struct);
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
        let result = validator.validate_request_body(&operation, Some(&body), &headers_struct);
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
        let headers_struct = TestParamStruct { data: headers };
        let body = json!({});
        let result = validator.validate_request_body(&operation, Some(&body), &headers_struct);
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
        let headers_struct = TestParamStruct { data: headers };
        let body = json!({});
        let result = validator.validate_request_body(&operation, Some(&body), &headers_struct);
        assert!(result.is_err());
        if let Err(ValidationError::FieldMissing(field, _)) = result {
            assert_eq!(field, "schema");
        } else {
            panic!("Expected ValidationError::FieldMissing");
        }
    }
}
