use crate::error::{
    OperationSection, PayloadSection, Section, SpecificationSection, ValidationErrorType,
};
use crate::traverser::OpenApiTraverser;
use crate::types::Operation;
use crate::validator::Validator;
use crate::SECURITY_FIELD;
use jsonschema::ValidationOptions;
use serde_json::Value;
use std::collections::HashSet;

pub(crate) struct RequestScopeValidator<'a> {
    request_instance: &'a Vec<String>,
    section: Section,
}

impl<'a> RequestScopeValidator<'a> {
    pub(crate) fn new<'b>(request_instance: &'b Vec<String>) -> Self
    where
        'b: 'a,
    {
        Self {
            request_instance,
            section: Section::Specification(SpecificationSection::Paths(
                OperationSection::Security,
            )),
        }
    }

    fn validate_scopes_using_schema(
        security_definitions: &Value,
        request_scopes: &HashSet<&str>,
    ) -> Result<(), ValidationErrorType> {
        // get the array of maps
        let security_defs = OpenApiTraverser::require_array(security_definitions)?;

        if security_defs.is_empty() {
            log::debug!("Definition is empty, scopes automatically pass");
            return Ok(());
        }

        for security_definition in security_defs {
            // convert to map
            let security_def = OpenApiTraverser::require_object(security_definition)?;
            for (schema_name, scope_list) in security_def {
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
                    log::debug!("Scopes match {schema_name}");
                    return Ok(());
                }
            }
        }
        Err(ValidationErrorType::FieldExpected(
            request_scopes
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<String>>()
                .join(", "),
            Section::Payload(PayloadSection::Other),
        ))
    }
}

impl<'a> Validator for RequestScopeValidator<'a> {
    /// Validates whether a request has at least one of the required scopes specified in the operation's security definitions.
    fn validate(
        &self,
        traverser: &OpenApiTraverser,
        op: &Operation,
        _validation_options: &ValidationOptions,
    ) -> Result<(), ValidationErrorType> {
        let op = &op.data;
        let scopes: HashSet<&str> = self.request_instance.iter().map(|s| s.as_str()).collect();

        let security_defs = traverser.get_optional(op, SECURITY_FIELD)?;
        if let Some(security_defs) = security_defs {
            return Self::validate_scopes_using_schema(security_defs.value(), &scopes);
        }

        let global_security_defs =
            traverser.get_optional(traverser.specification(), SECURITY_FIELD)?;
        if let Some(security_definitions) = global_security_defs {
            return Self::validate_scopes_using_schema(security_definitions.value(), &scopes);
        }
        Ok(())
    }

    fn section(&self) -> &Section {
        &self.section
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

        // Test with a missing "write" scope
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

        // Create an operation with a malformed security requirement (not an array of objects)
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
}
