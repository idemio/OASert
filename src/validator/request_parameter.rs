use crate::traverser::{OpenApiTraverser, TraverserError};
use crate::types::operation::Operation;
use crate::types::primitive::OpenApiPrimitives;
use crate::types::ParameterLocation;
use crate::validator::{ValidationError, Validator};
use crate::{IN_FIELD, NAME_FIELD, PARAMETERS_FIELD, REQUIRED_FIELD, SCHEMA_FIELD};
use jsonschema::ValidationOptions;
use serde_json::json;
use std::collections::HashMap;

pub(crate) struct RequestParameterValidator<'validator> {
    request_instance: &'validator HashMap<String, String>,
    parameter_location: ParameterLocation,
}

impl<'a> RequestParameterValidator<'a> {
    pub(crate) fn new<'n>(
        request_instance: &'n HashMap<String, String>,
        parameter_location: ParameterLocation,
    ) -> Self
    where
        'n: 'a,
    {
        Self {
            request_instance,
            parameter_location,
        }
    }
}

const OPERATION_ID_FIELD: &str = "operationId";

impl Validator for RequestParameterValidator<'_> {
    /// Validates request parameters against an OpenAPI operation definition.
    fn validate(
        &self,
        traverser: &OpenApiTraverser,
        op: &Operation,
        validation_options: &ValidationOptions,
    ) -> Result<(), ValidationError> {
        let op_def = &op.data;
        let operation_id = OpenApiTraverser::get_as_str(op_def, OPERATION_ID_FIELD);
        let param_defs = match match traverser.get_optional(op_def, PARAMETERS_FIELD) {
            Ok(res) => Ok(res),
            Err(e) => match e {
                TraverserError::MissingField(_) => Ok(None),
                _ => Err(e),
            },
        } {
            Ok(defs) => defs,
            Err(e) => {
                return Err(ValidationError::validation_traversal_error(e));
            }
        };

        match param_defs {
            Some(param_defs) => {
                let param_defs = match OpenApiTraverser::require_array(param_defs.value()) {
                    Ok(param_defs) => param_defs,
                    Err(e) => {
                        return Err(ValidationError::validation_traversal_error(e));
                    }
                };

                for param_def in param_defs {
                    // Only look at parameters that match the current section.
                    let loc = match traverser.get_required(param_def, IN_FIELD) {
                        Ok(in_f) => in_f,
                        Err(e) => {
                            return Err(ValidationError::validation_traversal_error(e));
                        }
                    };
                    let loc = match OpenApiTraverser::require_str(loc.value()) {
                        Ok(loc) => loc,
                        Err(e) => {
                            return Err(ValidationError::validation_traversal_error(e));
                        }
                    };

                    if loc.to_lowercase() == self.parameter_location.to_string().to_lowercase() {
                        let param_name = match traverser.get_required(param_def, NAME_FIELD) {
                            Ok(param_name) => param_name,
                            Err(e) => {
                                return Err(ValidationError::validation_traversal_error(e));
                            }
                        };

                        let param_name = match OpenApiTraverser::require_str(param_name.value()) {
                            Ok(param_name) => param_name,
                            Err(e) => {
                                return Err(ValidationError::validation_traversal_error(e));
                            }
                        };

                        let is_param_required =
                            match traverser.get_optional(param_def, REQUIRED_FIELD) {
                                Ok(is_param_required) => is_param_required,
                                Err(e) => {
                                    return Err(ValidationError::validation_traversal_error(e));
                                }
                            };

                        let is_param_required: bool = match is_param_required {
                            None => false,
                            Some(val) => OpenApiTraverser::require_bool(val.value())
                                .unwrap_or_else(|_| false),
                        };

                        let param_schema = match traverser.get_required(param_def, SCHEMA_FIELD) {
                            Ok(param_schema) => param_schema,
                            Err(e) => {
                                return Err(ValidationError::validation_traversal_error(e));
                            }
                        };

                        let param_schema = param_schema.value();
                        if let Some(req_param_val) = self.request_instance.get(param_name) {
                            let inst = json!(req_param_val);
                            if let Some(string) = inst.as_str() {
                                let inst = OpenApiPrimitives::convert_string_to_schema_type(
                                    param_schema,
                                    string,
                                )
                                .map_err(|e| ValidationError::validation_primitive_error(e))?;
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
                            return Err(ValidationError::validation_error(
                                &format!(
                                    "Parameter '{}' is required but not found in request.",
                                    param_name
                                ),
                                param_schema,
                            ));
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
