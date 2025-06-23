use crate::error::ValidationErrorType;
use crate::traverser::OpenApiTraverser;
use crate::types::Operation;
use crate::validator::Validator;
use crate::SECURITY_FIELD;
use jsonschema::ValidationOptions;
use serde_json::Value;
use std::collections::HashSet;

pub(crate) struct RequestScopeValidator<'validator> {
    request_instance: &'validator Vec<String>,
}

impl<'validator> RequestScopeValidator<'validator> {
    pub(crate) fn new<'node>(request_instance: &'node Vec<String>) -> Self
    where
        'node: 'validator,
    {
        Self { request_instance }
    }

    fn validate_scopes_using_schema(
        security_definitions: &Value,
        request_scopes: &HashSet<&str>,
        operation_id: &str,
    ) -> Result<(), ValidationErrorType> {
        let security_defs = match OpenApiTraverser::require_array(security_definitions) {
            Ok(security_defs) => security_defs,
            Err(e) => {
                return Err(ValidationErrorType::traversal_failed(
                    e,
                    &format!(
                        "Failed to parse security definitions as a vector in operation '{}'",
                        operation_id
                    ),
                ));
            }
        };

        if security_defs.is_empty() {
            log::debug!("Definition is empty, scopes automatically pass");
            return Ok(());
        }

        for security_definition in security_defs {
            let security_def = match OpenApiTraverser::require_object(security_definition) {
                Ok(security_def) => security_def,
                Err(e) => {
                    return Err(ValidationErrorType::traversal_failed(
                        e,
                        &format!(
                            "Failed to parse security definition as a map in operation '{}'",
                            operation_id
                        ),
                    ));
                }
            };

            for (schema_name, scope_list) in security_def {
                let scope_list = match OpenApiTraverser::require_array(scope_list) {
                    Ok(scope_list) => scope_list,
                    Err(e) => {
                        return Err(ValidationErrorType::traversal_failed(
                            e,
                            &format!(
                                "Failed to parse scope list as a list in operation '{}'",
                                operation_id
                            ),
                        ));
                    }
                };

                let mut scopes_match_schema = true;
                'scope_match: for scope in scope_list {
                    let scope = match OpenApiTraverser::require_str(scope) {
                        Ok(scope) => scope,
                        Err(e) => {
                            return Err(ValidationErrorType::traversal_failed(
                                e,
                                &format!(
                                    "Failed to parse scope as a string in operation '{}'",
                                    operation_id
                                ),
                            ));
                        }
                    };
                    if !request_scopes.contains(scope) {
                        scopes_match_schema = false;
                        break 'scope_match;
                    }
                }

                if scopes_match_schema {
                    log::debug!("Scopes match {schema_name}");
                    return Ok(());
                }
            }
        }
        Err(ValidationErrorType::assertion_failed(&format!(
            "Request scopes {} did not match any security definition in operation '{}'",
            request_scopes
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<String>>()
                .join(", "),
            operation_id
        )))
    }
}

impl Validator for RequestScopeValidator<'_> {
    fn validate(
        &self,
        traverser: &OpenApiTraverser,
        op: &Operation,
        _validation_options: &ValidationOptions,
    ) -> Result<(), ValidationErrorType> {
        let op = &op.data;
        let operation_id = OpenApiTraverser::get_as_str(&op, "operationId")
            .unwrap_or_else(|_| "default_operation_id");

        let scopes: HashSet<&str> = self.request_instance.iter().map(|s| s.as_str()).collect();
        let security_defs = match traverser.get_optional(op, SECURITY_FIELD) {
            Ok(security_defs) => security_defs,
            Err(e) => {
                return Err(ValidationErrorType::traversal_failed(
                    e,
                    &format!("Failed to get 'security' from operation '{}'", operation_id),
                ));
            }
        };

        if let Some(security_defs) = security_defs {
            return Self::validate_scopes_using_schema(
                security_defs.value(),
                &scopes,
                &operation_id,
            );
        }

        let global_security_defs =
            match traverser.get_optional(traverser.specification(), SECURITY_FIELD) {
                Ok(global_security_defs) => global_security_defs,
                Err(e) => {
                    return Err(ValidationErrorType::traversal_failed(
                        e,
                        "Failed to get global 'security' from specification",
                    ));
                }
            };

        if let Some(security_definitions) = global_security_defs {
            return Self::validate_scopes_using_schema(
                security_definitions.value(),
                &scopes,
                &operation_id,
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use crate::types::json_path::JsonPath;
    use crate::types::Operation;
    use crate::validator::OpenApiPayloadValidator;
    use serde_json::{json, Value};

    fn create_operation_with_security(security_requirements: Value) -> Operation {
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
    fn create_validator_with_security_definitions(definitions: Value) -> OpenApiPayloadValidator {
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
        let operation = validator
            .traverser()
            .get_operation_from_path_and_method("/test", "get")
            .unwrap();
        let scopes = vec!["read".to_string(), "write".to_string()];
        let result = validator.validate_request_scopes(&operation, &scopes);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_request_scopes_success_with_extra_scopes() {
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
        let operation = validator
            .traverser()
            .get_operation_from_path_and_method("/test", "get")
            .unwrap();
        let scopes = vec!["read".to_string(), "write".to_string(), "admin".to_string()];
        let result = validator.validate_request_scopes(&operation, &scopes);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_request_scopes_missing_required_scope() {
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
        let operation = validator
            .traverser()
            .get_operation_from_path_and_method("/test", "get")
            .unwrap();
        let scopes = vec!["read".to_string()];
        let result = validator.validate_request_scopes(&operation, &scopes);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_request_scopes_empty_scopes() {
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
        let operation = validator
            .traverser()
            .get_operation_from_path_and_method("/test", "get")
            .unwrap();
        let scopes = vec![];
        let result = validator.validate_request_scopes(&operation, &scopes);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_request_scopes_multiple_security_requirements_one_satisfied() {
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
        let operation = create_operation_with_security(json!([
            { "oauth2": ["read", "write"] },
            { "apiKey": [] }
        ]));
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
        let validator = create_validator_with_security_definitions(json!({}));
        let operation = create_operation_with_security(json!([]));
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
        let operation = create_operation_with_security(json!("malformed"));
        let scopes = vec!["read".to_string(), "write".to_string()];
        let result = validator.validate_request_scopes(&operation, &scopes);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_request_scopes_with_security_scheme_without_scopes() {
        let validator = create_validator_with_security_definitions(json!({
            "apiKey": {
                "type": "apiKey",
                "name": "api_key",
                "in": "header"
            }
        }));
        let operation = create_operation_with_security(json!([
            { "apiKey": [] }
        ]));
        let scopes = vec![];
        let result = validator.validate_request_scopes(&operation, &scopes);
        assert!(result.is_ok());
    }
}
