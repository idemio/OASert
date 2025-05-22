use crate::OpenApiValidationError;
use crate::openapi_util::JsonPath;
use jsonschema::{Resource, Validator};
use serde_json::{Value, json};
use std::collections::HashMap;
const PATH_PARAM_LEFT: char = '{';
const PATH_PARAM_RIGHT: char = '}';
pub trait OpenApiValidator {
    fn validate_request(
        &self,
        path: &str,
        method: &str,
        body: Option<&Value>,
        headers: Option<&HashMap<String, String>>,
        query_params: Option<&HashMap<String, String>>,
    ) -> Result<(), OpenApiValidationError>;
    fn validators(&self) -> &HashMap<String, Validator>;
    fn validators_mut(&mut self) -> &mut HashMap<String, Validator>;
    fn root_schema(&self) -> &Value;
    fn get_validator(
        &self,
        json_path: &JsonPath,
        spec: Value,
    ) -> Result<Validator, OpenApiValidationError> {
        let string_path = json_path.format_path();
        let validator =
            match ValidatorFactory::build_validator_for_path(string_path, spec) {
                Ok(v) => v,
                Err(e) => return Err(e),
            };
        Ok(validator)
//        let current_validators = self.validators_mut();
//        let string_path = json_path.format_path();
//        if current_validators.contains_key(&string_path) {
//            Ok(current_validators.get(&string_path).unwrap())
//        } else {
//            let validator =
//                match ValidatorFactory::build_validator_for_path(string_path, spec) {
//                    Ok(v) => v,
//                    Err(e) => return Err(e),
//                };
//            let path = json_path.format_path();
//            current_validators.insert(path.clone(), validator);
//            Ok(current_validators.get(&path).unwrap())
//        }
    }

    /// Extracts the path parameter name (between the chars '{' and '}')
    /// returns None if there is no path parameter in the segment.
    fn extract_openapi_path_parameter_name(segment: &str) -> Option<&str> {
        segment.find(PATH_PARAM_LEFT).and_then(|start| {
            segment
                .find(PATH_PARAM_RIGHT)
                .map(|end| &segment[start + 1..end])
        })
    }

    fn validate_schema_from_pointer(
        &self,
        instance: &Value,
        json_path: &JsonPath,
    ) -> Result<(), OpenApiValidationError> {
        let root_schema = self.root_schema().clone();
        let validator = match self.get_validator(json_path, root_schema) {
            Ok(v) => v,
            Err(e) => return Err(e),
        };

        match validator.validate(instance) {
            Ok(_) => Ok(()),
            Err(_) => Err(OpenApiValidationError::InvalidSchema(
                "Validation failed".to_string(),
            )),
        }
    }
}

pub(crate) struct ValidatorFactory;
impl ValidatorFactory {
    pub fn build_validator_for_path(
        json_path: String,
        specification: Value,
    ) -> Result<Validator, OpenApiValidationError> {
        let full_pointer_path = format!("@@root#/{}", json_path);
        let schema = json!({
            "$ref": full_pointer_path
        });

        let resource = match Resource::from_contents(specification) {
            Ok(res) => res,
            Err(_) => {
                return Err(OpenApiValidationError::InvalidSchema(
                    "Invalid specification provided".to_string(),
                ));
            }
        };

        let validator = match Validator::options()
            .with_resource("@@inner", resource)
            .build(&schema)
        {
            Ok(validator) => validator,
            Err(e) => {
                return Err(OpenApiValidationError::InvalidPath(
                    "Invalid json path provided".to_string(),
                ));
            }
        };
        Ok(validator)
    }
}
