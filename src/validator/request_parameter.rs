use crate::error::ValidationErrorType;
use crate::traverser::{OpenApiTraverser, TraverserError};
use crate::types::primitive::OpenApiPrimitives;
use crate::types::{Operation, ParameterLocation};
use crate::validator::Validator;
use crate::{IN_FIELD, NAME_FIELD, PARAMETERS_FIELD, REQUIRED_FIELD, SCHEMA_FIELD};
use jsonschema::ValidationOptions;
use serde_json::json;
use std::collections::HashMap;

pub(crate) struct RequestParameterValidator<'validator> {
    request_instance: &'validator HashMap<String, String>,
    parameter_location: ParameterLocation,
}

impl<'validator> RequestParameterValidator<'validator> {
    pub(crate) fn new<'node>(
        request_instance: &'node HashMap<String, String>,
        parameter_location: ParameterLocation,
    ) -> Self
    where
        'node: 'validator,
    {
        Self {
            request_instance,
            parameter_location,
        }
    }
}

impl Validator for RequestParameterValidator<'_> {
    /// Validates request parameters against an OpenAPI operation definition.
    fn validate(
        &self,
        traverser: &OpenApiTraverser,
        op: &Operation,
        validation_options: &ValidationOptions,
    ) -> Result<(), ValidationErrorType> {
        let op_def = &op.data;
        let operation_id =
            OpenApiTraverser::get_as_str(op_def, "operationId").unwrap_or("default_operation_id");
        let param_defs = match match traverser.get_optional(op_def, PARAMETERS_FIELD) {
            Ok(res) => Ok(res),
            Err(e) => match e {
                TraverserError::MissingField(_) => Ok(None),
                _ => Err(e),
            },
        } {
            Ok(defs) => defs,
            Err(e) => {
                return Err(ValidationErrorType::traversal_failed(
                    e,
                    &format!(
                        "Failed to get 'parameters' from operation '{}'",
                        operation_id
                    ),
                ));
            }
        };

        match param_defs {
            Some(param_defs) => {
                let param_defs = match OpenApiTraverser::require_array(param_defs.value()) {
                    Ok(param_defs) => param_defs,
                    Err(e) => {
                        return Err(ValidationErrorType::traversal_failed(
                            e,
                            &format!(
                                "Failed to parse 'parameters' as a vector value in {}",
                                operation_id
                            ),
                        ));
                    }
                };

                for param_def in param_defs {
                    // Only look at parameters that match the current section.
                    let loc = match traverser.get_required(param_def, IN_FIELD) {
                        Ok(in_f) => in_f,
                        Err(e) => {
                            return Err(ValidationErrorType::traversal_failed(
                                e,
                                &format!(
                                    "Failed to get 'in' in parameter definition in operation '{}'",
                                    operation_id
                                ),
                            ));
                        }
                    };
                    let loc = match OpenApiTraverser::require_str(loc.value()) {
                        Ok(loc) => loc,
                        Err(e) => {
                            return Err(ValidationErrorType::traversal_failed(
                                e,
                                &format!(
                                    "Failed to parse 'in' from parameter as a string value in {}",
                                    operation_id
                                ),
                            ));
                        }
                    };

                    if loc.to_lowercase() == self.parameter_location.to_string().to_lowercase() {
                        let param_name = match traverser.get_required(param_def, NAME_FIELD) {
                            Ok(param_name) => param_name,
                            Err(e) => {
                                return Err(ValidationErrorType::traversal_failed(
                                    e,
                                    &format!(
                                        "Failed to get 'name' from parameter in operation '{}'",
                                        operation_id
                                    ),
                                ));
                            }
                        };

                        let param_name = match OpenApiTraverser::require_str(param_name.value()) {
                            Ok(param_name) => param_name,
                            Err(e) => {
                                return Err(ValidationErrorType::traversal_failed(
                                    e,
                                    &format!(
                                        "Failed to parse 'name' from parameter as a string value in {}",
                                        operation_id
                                    ),
                                ));
                            }
                        };

                        let is_param_required = match traverser
                            .get_optional(param_def, REQUIRED_FIELD)
                        {
                            Ok(is_param_required) => is_param_required,
                            Err(e) => {
                                return Err(ValidationErrorType::traversal_failed(
                                    e,
                                    &format!(
                                        "Failed to get 'required' from parameter '{}' in operation '{}'",
                                        param_name, operation_id
                                    ),
                                ));
                            }
                        };

                        let is_param_required: bool = match is_param_required {
                            None => false,
                            Some(val) => {
                                OpenApiTraverser::require_bool(val.value()).unwrap_or_else(|_| {
                                    log::trace!("Request parameter '{}' in operation '{}' does not have 'required' field defined. Using false as default.", param_name, operation_id);
                                    false
                                })
                            }
                        };

                        let param_schema = match traverser.get_required(param_def, SCHEMA_FIELD) {
                            Ok(param_schema) => param_schema,
                            Err(e) => {
                                return Err(ValidationErrorType::traversal_failed(
                                    e,
                                    &format!(
                                        "Failed to get 'schema' from parameter '{}' in operation '{}'",
                                        param_name, operation_id
                                    ),
                                ));
                            }
                        };

                        let param_schema = param_schema.value();
                        if let Some(req_param_val) = self.request_instance.get(param_name) {
                            let inst = json!(req_param_val);
                            if let Some(string) = inst.as_str() {
                                let inst = OpenApiPrimitives::convert_string_to_schema_type(
                                    param_schema,
                                    string,
                                )?;
                                Self::complex_validation_by_schema(
                                    validation_options,
                                    &param_schema,
                                    &inst,
                                )?
                            } else {
                                Self::complex_validation_by_schema(
                                    validation_options,
                                    &param_schema,
                                    &inst,
                                )?
                            }
                        } else if is_param_required {
                            return Err(ValidationErrorType::assertion_failed(&format!(
                                "Parameter '{}' is required but not found in request.",
                                param_name
                            )));
                        }
                    }
                }
                Ok(())
            }
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod test {
    use crate::validator::OpenApiPayloadValidator;
    use serde_json::json;

    // A helper-function to create a validator with a specific schema
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
        let operation = validator
            .traverser()
            .get_operation_from_path_and_method("/test", "get")
            .unwrap();
        let query_params = "limit=50&offset=10&filter=active";
        let result = validator.validate_request_query_parameters(&operation, query_params);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_query_params_missing_required() {
        let validator = create_validator();
        let operation = validator
            .traverser()
            .get_operation_from_path_and_method("/test", "get")
            .unwrap();
        let query_params = "offset=10&filter=active";
        let result = validator.validate_request_query_parameters(&operation, query_params);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_query_params_with_only_required() {
        let validator = create_validator();
        let operation = validator
            .traverser()
            .get_operation_from_path_and_method("/test", "get")
            .unwrap();
        let query_params = "limit=50";
        let result = validator.validate_request_query_parameters(&operation, query_params);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_query_params_invalid_value() {
        let validator = create_validator();
        let operation = validator
            .traverser()
            .get_operation_from_path_and_method("/test", "get")
            .unwrap();
        let query_params = "limit=200&offset=10";
        let result = validator.validate_request_query_parameters(&operation, query_params);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_query_params_non_numeric_for_integer() {
        let validator = create_validator();
        let operation = validator
            .traverser()
            .get_operation_from_path_and_method("/test", "get")
            .unwrap();
        let query_params = "limit=abc&offset=10";
        let result = validator.validate_request_query_parameters(&operation, query_params);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_query_params_invalid_enum_value() {
        let validator = create_validator();
        let operation = validator
            .traverser()
            .get_operation_from_path_and_method("/test", "get")
            .unwrap();
        let query_params = "limit=50&filter=invalid";
        let result = validator.validate_request_query_parameters(&operation, query_params);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_query_params_empty_string() {
        let validator = create_validator();
        let operation = validator
            .traverser()
            .get_operation_from_path_and_method("/test", "get")
            .unwrap();
        let query_params = "";
        let result = validator.validate_request_query_parameters(&operation, query_params);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_query_params_malformed_query() {
        let validator = create_validator();
        let operation = validator
            .traverser()
            .get_operation_from_path_and_method("/test", "get")
            .unwrap();
        let query_params = "limit=50&offset=";
        let result = validator.validate_request_query_parameters(&operation, query_params);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_query_params_duplicate_parameters() {
        let validator = create_validator();
        let operation = validator
            .traverser()
            .get_operation_from_path_and_method("/test", "get")
            .unwrap();
        let query_params = "limit=50&limit=75";
        let result = validator.validate_request_query_parameters(&operation, query_params);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_query_params_url_encoded_values() {
        let validator = create_validator();
        let operation = validator
            .traverser()
            .get_operation_from_path_and_method("/test", "get")
            .unwrap();
        let query_params = "limit=50&filter=active%20items";
        let result = validator.validate_request_query_parameters(&operation, query_params);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_query_params_extra_parameters() {
        let validator = create_validator();
        let operation = validator
            .traverser()
            .get_operation_from_path_and_method("/test", "get")
            .unwrap();
        let query_params = "limit=50&extra=value";
        let result = validator.validate_request_query_parameters(&operation, query_params);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_query_params_no_parameters_defined() {
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
        let operation = validator
            .traverser()
            .get_operation_from_path_and_method("/test", "get")
            .unwrap();
        let query_params = "param1=value1&param2=value2";
        let result = validator.validate_request_query_parameters(&operation, query_params);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_query_params_parsing_edge_cases() {
        let validator = create_validator();
        let operation = validator
            .traverser()
            .get_operation_from_path_and_method("/test", "get")
            .unwrap();
        let query_params = "&";
        let result = validator.validate_request_query_parameters(&operation, query_params);
        assert!(result.is_err()); // Missing required parameter
        let query_params = "limit";
        let result = validator.validate_request_query_parameters(&operation, query_params);
        assert!(result.is_err());
        let query_params = "limit=";
        let result = validator.validate_request_query_parameters(&operation, query_params);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_query_params_with_array_reference() {
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
        let operation = validator
            .traverser()
            .get_operation_from_path_and_method("/test", "get")
            .unwrap();
        let query_params = "limit=10";
        let result = validator.validate_request_query_parameters(&operation, query_params);
        println!("{:?}", result);
        assert!(result.is_ok());
    }
}
