use crate::error::{
    OperationSection, PayloadSection, Section, SpecificationSection, ValidationErrorType,
};
use crate::traverser::OpenApiTraverser;
use crate::types::Operation;
use crate::validator::Validator;
use crate::{CONTENT_FIELD, REQUEST_BODY_FIELD, REQUIRED_FIELD, SCHEMA_FIELD};
use jsonschema::ValidationOptions;
use serde_json::Value;

pub(crate) struct RequestBodyValidator<'a> {
    request_instance: Option<&'a Value>,
    content_type: Option<String>,
    section: Section,
}

impl<'a> RequestBodyValidator<'a> {
    pub(crate) fn new<'b>(request_instance: Option<&'b Value>, content_type: Option<String>) -> Self
    where
        'b: 'a,
    {
        Self {
            request_instance,
            content_type,
            section: Section::Payload(PayloadSection::Body),
        }
    }

    /// Validates that all required fields specified in a schema are present in the request body.
    fn check_required_body(
        traverser: &OpenApiTraverser,
        body_schema: &Value,
        request_body: Option<&Value>,
    ) -> Result<(), ValidationErrorType> {
        if let Some(required_fields) = traverser.get_optional(body_schema, REQUIRED_FIELD)? {
            let required_fields = OpenApiTraverser::require_array(required_fields.value())?;

            // if the body provided is empty and required fields are present, then it's an invalid body.
            if !required_fields.is_empty() && request_body.is_none() {
                return Err(ValidationErrorType::SectionExpected(Section::Payload(
                    PayloadSection::Body,
                )));
            }

            if let Some(body) = request_body {
                for required in required_fields {
                    let required_field = OpenApiTraverser::require_str(required)?;

                    // if the current required field is not present in the body, then it's a failure.
                    if body.get(required_field).is_none() {
                        return Err(ValidationErrorType::FieldExpected(
                            required_field.to_string(),
                            Section::Payload(PayloadSection::Body),
                        ));
                    }
                }
            }
        }
        Ok(())
    }
}

impl<'a> Validator for RequestBodyValidator<'a> {
    /// Validates the request body of an OpenAPI operation against the specification.
    fn validate(
        &self,
        traverser: &OpenApiTraverser,
        op: &Operation,
        validation_opts: &ValidationOptions,
    ) -> Result<(), ValidationErrorType> {
        let (op_def, mut op_path) = (&op.data, op.path.clone());
        let body = self.request_instance;

        let req_body_def = match traverser.get_optional(&op_def, REQUEST_BODY_FIELD)? {
            None if body.is_some() => {
                return Err(ValidationErrorType::SectionExpected(
                    Section::Specification(SpecificationSection::Paths(
                        OperationSection::RequestBody,
                    )),
                ));
            }
            None => return Ok(()),
            Some(val) => val,
        };

        let is_body_required = traverser.get_optional(req_body_def.value(), REQUIRED_FIELD)?;

        let is_body_required: bool = match is_body_required {
            None => true,
            Some(val) => val.value().as_bool().unwrap_or(true),
        };
        if let Some(ctype) = &self.content_type {
            let content_def = traverser.get_required(req_body_def.value(), CONTENT_FIELD)?;

            let media_def = traverser.get_required(content_def.value(), &ctype)?;

            let media_schema = traverser.get_required(media_def.value(), SCHEMA_FIELD)?;

            Self::check_required_body(traverser, media_schema.value(), body)?;
            if let Some(body_instance) = body {
                op_path
                    .add(REQUEST_BODY_FIELD)
                    .add(CONTENT_FIELD)
                    .add(&ctype)
                    .add(SCHEMA_FIELD);

                Self::complex_validation_by_path(
                    &validation_opts,
                    &op_path,
                    body_instance,
                    self.section.clone(),
                )?

            // if the body does not exist, make sure 'required' is set to false.
            } else if is_body_required {
                return Err(ValidationErrorType::SectionExpected(Section::Payload(
                    PayloadSection::Body,
                )));
            }
        } else if is_body_required {
            return Err(ValidationErrorType::FieldExpected(
                String::from("Content-Type"),
                Section::Payload(PayloadSection::Header),
            ));
        }

        Ok(())
    }

    fn section(&self) -> &Section {
        &self.section
    }
}
