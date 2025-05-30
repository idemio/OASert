use crate::traverser::OpenApiTraverser;
use crate::types::{Operation, ParameterLocation};
use crate::{
    CONTENT_FIELD, IN_FIELD, JsonPath, NAME_FIELD, PARAMETERS_FIELD, REF_FIELD, REQUEST_BODY_FIELD,
    REQUIRED_FIELD, SCHEMA_FIELD, SECURITY_FIELD, ValidationError, ValidationErrorKind, traverser,
};
use jsonschema::ValidationOptions;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use unicase::UniCase;

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
    /// * `Err(ValidationError::SchemaValidationFailed)` - If either schema building fails or validation fails
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
        if let Err(e) = jsonschema::validate(schema, instance) {
            return Err(ValidationError::SchemaValidationFailed);
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
                    traverser::require_array(parameter_definitions.value())?;
                for parameter_definition in parameter_definitions {
                    // Only look at parameters that match the current section.
                    if traverser::get_as_str(parameter_definition, IN_FIELD)
                        .is_ok_and(|v| v == parameter_location.to_string())
                    {
                        let parameter_name = traverser::get_as_str(parameter_definition, NAME_FIELD)?;
                        let parameter_schema = traverser::get_as_any(parameter_definition, SCHEMA_FIELD)?;
                        let is_parameter_required = traverser::get_as_bool(parameter_definition, REQUIRED_FIELD).unwrap_or(false);
                        if let Some(request_parameter_value) =
                            self.request_instance.get(&UniCase::<String>::from(parameter_name))
                        {
                            Self::simple_validation(parameter_schema, &json!(request_parameter_value))?
                        } else if is_parameter_required {
                            return Err(ValidationError::RequiredParameterMissing)
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
        body_schema: &Value,
        request_body: Option<&Value>,
    ) -> Result<(), ValidationError> {
        if let Ok(required_fields) = traverser::get_as_array(body_schema, REQUIRED_FIELD) {
            // if the body provided is empty and required fields are present, then it's an invalid body.
            if !required_fields.is_empty() && request_body.is_none() {
                return Err(ValidationError::ValueExpected);
            }

            if let Some(body) = request_body {
                for required in required_fields {
                    let required_field = traverser::require_str(required)?;

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
            traverser::get_as_bool(request_body_definition.value(), REQUIRED_FIELD).unwrap_or(true);
        if let Some(content_type) = &self.content_type {
            let content_definition =
                traverser.get_required_spec_node(request_body_definition.value(), CONTENT_FIELD)?;

            let media_type_definition =
                traverser.get_required_spec_node(content_definition.value(), &content_type)?;

            let request_media_type_definition =
                traverser.get_required_spec_node(media_type_definition.value(), SCHEMA_FIELD)?;

            Self::check_required_body(request_media_type_definition.value(), body_instance)?;
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

        let request_scopes: HashSet<&str> = self.request_instance.iter()
            .map(|s| s.as_str())
            .collect();
        
        if let Some(security_definitions) = security_definitions {
            // get the array of maps
            let security_definitions = traverser::require_array(security_definitions.value())?;

            for security_definition in security_definitions {
                // convert to map
                let security_definition = traverser::require_object(security_definition)?;

                for (_, scope_list) in security_definition {
                    // convert to list
                    let scope_list = traverser::require_array(scope_list)?;

                    // check to see if the scope is found in our request scopes
                    for scope in scope_list {
                        let scope = traverser::require_str(scope)?;
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
