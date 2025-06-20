use crate::error::{OperationSection, Section, SpecificationSection, ValidationErrorType};
use crate::traverser::OpenApiTraverser;
use crate::types::primitive::OpenApiPrimitives;
use crate::types::{Operation, ParameterLocation};
use crate::validator::Validator;
use crate::{IN_FIELD, NAME_FIELD, PARAMETERS_FIELD, REQUIRED_FIELD, SCHEMA_FIELD};
use jsonschema::ValidationOptions;
use serde_json::json;
use std::collections::HashMap;

pub(crate) struct RequestParameterValidator<'a> {
    request_instance: &'a HashMap<String, String>,
    parameter_location: ParameterLocation,
    section: Section,
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
            section: Section::Specification(SpecificationSection::Paths(
                OperationSection::Parameters,
            )),
        }
    }
}

impl<'a> Validator for RequestParameterValidator<'a> {
    /// Validates request parameters against an OpenAPI operation definition.
    fn validate(
        &self,
        traverser: &OpenApiTraverser,
        op: &Operation,
        validation_options: &ValidationOptions,
    ) -> Result<(), ValidationErrorType> {
        let op_def = &op.data;
        let param_defs = match traverser.get_optional(op_def, PARAMETERS_FIELD) {
            Ok(res) => Ok(res),
            Err(e) => match e {
                ValidationErrorType::FieldExpected(_, _) => Ok(None),
                _ => Err(e),
            },
        }?;

        match param_defs {
            Some(param_defs) => {
                let param_defs = OpenApiTraverser::require_array(param_defs.value())?;

                for param_def in param_defs {
                    // Only look at parameters that match the current section.
                    let loc = traverser.get_required(param_def, IN_FIELD)?;
                    let loc = OpenApiTraverser::require_str(loc.value())?;

                    if loc.to_lowercase() == self.parameter_location.to_string().to_lowercase() {
                        let param_name = traverser.get_required(param_def, NAME_FIELD)?;

                        let param_name = OpenApiTraverser::require_str(param_name.value())?;
                        let is_param_required =
                            traverser.get_optional(param_def, REQUIRED_FIELD)?;

                        let is_param_required: bool = match is_param_required {
                            None => false,
                            Some(val) => {
                                OpenApiTraverser::require_bool(val.value()).unwrap_or(false)
                            }
                        };

                        let param_schema = traverser.get_required(param_def, SCHEMA_FIELD)?;

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
                                    self.section.clone(),
                                )?
                            } else {
                                Self::complex_validation_by_schema(
                                    validation_options,
                                    &param_schema,
                                    &inst,
                                    self.section.clone(),
                                )?
                            }
                        } else if is_param_required {
                            return Err(ValidationErrorType::FieldExpected(
                                param_name.to_string(),
                                self.section.clone(),
                            ));
                        }
                    }
                }
                Ok(())
            }
            None => Ok(()),
        }
    }

    fn section(&self) -> &Section {
        &self.section
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
        let operation = validator.traverser().get_operation("/test", "get").unwrap();
        let query_params = "limit=50&offset=10&filter=active";
        let result = validator.validate_request_query_parameters(&operation, query_params);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_query_params_missing_required() {
        let validator = create_validator();
        let operation = validator.traverser().get_operation("/test", "get").unwrap();
        let query_params = "offset=10&filter=active";
        let result = validator.validate_request_query_parameters(&operation, query_params);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_query_params_with_only_required() {
        let validator = create_validator();
        let operation = validator.traverser().get_operation("/test", "get").unwrap();
        let query_params = "limit=50";
        let result = validator.validate_request_query_parameters(&operation, query_params);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_query_params_invalid_value() {
        let validator = create_validator();
        let operation = validator.traverser().get_operation("/test", "get").unwrap();
        let query_params = "limit=200&offset=10";
        let result = validator.validate_request_query_parameters(&operation, query_params);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_query_params_non_numeric_for_integer() {
        let validator = create_validator();
        let operation = validator.traverser().get_operation("/test", "get").unwrap();
        let query_params = "limit=abc&offset=10";
        let result = validator.validate_request_query_parameters(&operation, query_params);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_query_params_invalid_enum_value() {
        let validator = create_validator();
        let operation = validator.traverser().get_operation("/test", "get").unwrap();
        let query_params = "limit=50&filter=invalid";
        let result = validator.validate_request_query_parameters(&operation, query_params);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_query_params_empty_string() {
        let validator = create_validator();
        let operation = validator.traverser().get_operation("/test", "get").unwrap();
        let query_params = "";
        let result = validator.validate_request_query_parameters(&operation, query_params);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_query_params_malformed_query() {
        let validator = create_validator();
        let operation = validator.traverser().get_operation("/test", "get").unwrap();
        let query_params = "limit=50&offset=";
        let result = validator.validate_request_query_parameters(&operation, query_params);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_query_params_duplicate_parameters() {
        let validator = create_validator();
        let operation = validator.traverser().get_operation("/test", "get").unwrap();
        let query_params = "limit=50&limit=75";
        let result = validator.validate_request_query_parameters(&operation, query_params);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_query_params_url_encoded_values() {
        let validator = create_validator();
        let operation = validator.traverser().get_operation("/test", "get").unwrap();
        let query_params = "limit=50&filter=active%20items";
        let result = validator.validate_request_query_parameters(&operation, query_params);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_query_params_extra_parameters() {
        let validator = create_validator();
        let operation = validator.traverser().get_operation("/test", "get").unwrap();
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
        let operation = validator.traverser().get_operation("/test", "get").unwrap();
        let query_params = "param1=value1&param2=value2";
        let result = validator.validate_request_query_parameters(&operation, query_params);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_query_params_parsing_edge_cases() {
        let validator = create_validator();
        let operation = validator.traverser().get_operation("/test", "get").unwrap();
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
        let operation = validator.traverser().get_operation("/test", "get").unwrap();
        let query_params = "limit=10";
        let result = validator.validate_request_query_parameters(&operation, query_params);
        assert!(result.is_ok());
    }
}
