use crate::traverser::OpenApiTraverser;
use crate::types::operation::Operation;
use crate::validator::{ValidationError, Validator};
use crate::{CONTENT_FIELD, REQUEST_BODY_FIELD, REQUIRED_FIELD, SCHEMA_FIELD};
use jsonschema::ValidationOptions;
use serde_json::Value;

pub(crate) struct RequestBodyValidator<'v> {
    request_instance: Option<&'v Value>,
    content_type: Option<&'v str>,
}

impl<'v> RequestBodyValidator<'v> {
    pub(crate) fn new<'node>(
        request_instance: Option<&'node Value>,
        content_type: Option<&'v str>,
    ) -> Self
    where
        'node: 'v,
    {
        Self {
            request_instance,
            content_type,
        }
    }

    /// Validates that all required fields specified in a schema are present in the request body.
    fn check_required_body(
        traverser: &OpenApiTraverser,
        body_schema: &Value,
        request_body: Option<&Value>,
        operation_id: &str,
    ) -> Result<(), ValidationError> {
        if let Some(required_fields) = match traverser.get_optional(body_schema, REQUIRED_FIELD) {
            Ok(req) => req,
            Err(e) => {
                return Err(ValidationError::validation_traversal_error(e));
            }
        } {
            let required_fields = match OpenApiTraverser::require_array(required_fields.value()) {
                Ok(required_fields) => required_fields,
                Err(e) => {
                    return Err(ValidationError::validation_traversal_error(e));
                }
            };

            // if the body provided is empty and required fields are present, then it's an invalid body.
            if !required_fields.is_empty() && request_body.is_none() {
                return Err(ValidationError::validation_error(
                    &format!(
                        "Request body is missing, but the request body has required fields in operation '{}'",
                        operation_id,
                    ),
                    body_schema,
                ));
            }

            if let Some(body) = request_body {
                for required in required_fields {
                    let required_field = match OpenApiTraverser::require_str(required) {
                        Ok(required_field) => required_field,
                        Err(e) => {
                            return Err(ValidationError::validation_traversal_error(e));
                        }
                    };

                    // if the current required field is not present in the body, then it's a failure.
                    if body.get(required_field).is_none() {
                        return Err(ValidationError::validation_error(
                            &format!(
                                "'{}' is required but missing from the requestBody in operation '{}'",
                                required_field, operation_id
                            ),
                            body_schema,
                        ));
                    }
                }
            }
        }
        Ok(())
    }
}

impl Validator for RequestBodyValidator<'_> {
    /// Validates the request body of an OpenAPI operation against the specification.
    fn validate(
        &self,
        traverser: &OpenApiTraverser,
        op: &Operation,
        validation_opts: &ValidationOptions,
    ) -> Result<(), ValidationError> {
        let (op_def, mut op_path) = (&op.data, op.path.clone());
        let body = self.request_instance;

        let operation_id = OpenApiTraverser::get_as_str(&op_def, "operationId")
            .unwrap_or_else(|_| "default_operation_id");

        let req_body_def = match match traverser.get_optional(&op_def, REQUEST_BODY_FIELD) {
            Ok(req_body_def) => req_body_def,
            Err(e) => {
                return Err(ValidationError::validation_traversal_error(e));
            }
        } {
            None if body.is_some_and(|body| !body.is_null()) => {
                return Err(ValidationError::validation_error(
                    &format!(
                        "Request body is present, but 'requestBody' is missing from operation '{}'",
                        operation_id
                    ),
                    op_def,
                ));
            }
            None => return Ok(()),
            Some(val) => val,
        };

        let is_body_required = match traverser.get_optional(req_body_def.value(), REQUIRED_FIELD) {
            Ok(is_body_required) => is_body_required,
            Err(e) => {
                return Err(ValidationError::validation_traversal_error(e));
            }
        };

        let is_body_required: bool = match is_body_required {
            None => true,
            Some(val) => val.value().as_bool().unwrap_or(true),
        };
        if let Some(content_type) = &self.content_type {
            let content_def = match traverser.get_required(req_body_def.value(), CONTENT_FIELD) {
                Ok(content_def) => content_def,
                Err(e) => {
                    return Err(ValidationError::validation_traversal_error(e));
                }
            };

            let media_def = match traverser.get_required(content_def.value(), &content_type) {
                Ok(media_def) => media_def,
                Err(e) => {
                    return Err(ValidationError::validation_traversal_error(e));
                }
            };

            let media_schema = match traverser.get_required(media_def.value(), SCHEMA_FIELD) {
                Ok(media_schema) => media_schema,
                Err(e) => {
                    return Err(ValidationError::validation_traversal_error(e));
                }
            };

            Self::check_required_body(traverser, media_schema.value(), body, &operation_id)?;

            if let Some(body_instance) = body {
                op_path
                    .add(REQUEST_BODY_FIELD)
                    .add(CONTENT_FIELD)
                    .add(&content_type)
                    .add(SCHEMA_FIELD);

                Self::complex_validation_by_path(&validation_opts, &op_path, body_instance)?

            // if the body does not exist, make sure 'required' is set to false.
            } else if is_body_required {
                return Err(ValidationError::validation_error(
                    &format!(
                        "Request body is missing, but is required for operation '{}'",
                        operation_id
                    ),
                    op_def,
                ));
            }
        } else if is_body_required {
            return Err(ValidationError::validation_error(
                &format!(
                    "Content-Type header is missing, but is required for operation '{}'",
                    operation_id
                ),
                op_def,
            ));
        }

        Ok(())
    }
}
