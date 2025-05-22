use std::collections::HashMap;
use std::fs;
use std::str::FromStr;
use jsonschema::Validator;
use oas3::OpenApiV3Spec;
use serde_json::{Value};
use openapiv3::OpenAPI;
use crate::openapi_v31x::OpenApiV31xValidator;
use crate::openapi_v30x::OpenApiV30xValidator;
use crate::OpenApiValidationError;
use crate::spec_validator::OpenApiValidator;

#[derive(Debug, Eq, PartialEq, Clone)]
pub enum OpenApiVersion {
    V30X,
    V31X,
}

impl OpenApiVersion {
    fn is_valid(&self, spec: &Value) -> Result<(), ()> {
        let res = match self {
            OpenApiVersion::V30X => {
                let openapi_version_schema = fs::read_to_string("./resource/openapi-v3.0.x.json").unwrap();
                let openapi_version_schema: Value = serde_json::from_str(&openapi_version_schema).unwrap();
                jsonschema::draft4::validate(&openapi_version_schema, spec)
            },
            OpenApiVersion::V31X => {
                let openapi_version_schema = fs::read_to_string("./resource/openapi-v3.1.x.json").unwrap();
                let openapi_version_schema: Value = serde_json::from_str(&openapi_version_schema).unwrap();
                jsonschema::draft4::validate(&openapi_version_schema, spec)
            }
        };
        if let Ok(_) = res {
            Ok(())
        } else {
            Err(())
        }
    }
}

impl FromStr for OpenApiVersion {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.starts_with("3.1") {
            Ok(Self::V31X)
        } else if s.starts_with("3.0") {
            Ok(Self::V30X)
        } else {
            Err(())
        }
    }
}

pub enum OpenApiValidators {
    V31X(OpenApiV31xValidator),
    V30X(OpenApiV30xValidator)
}

pub struct RequestValidator {
    traverser: OpenApiValidators,
    version: OpenApiVersion,
    validators: HashMap<String, Validator>
}

impl RequestValidator {

    fn create_openapi_v31x(value: Value) -> Result<OpenApiV31xValidator, ()> {
        match OpenApiV31xValidator::new(value) {
            Ok(validator) => Ok(validator),
            Err(_) => Err(())
        }
    }

    fn create_openapi_v30x(value: Value) -> Result<OpenApiV30xValidator, ()> {
        todo!()
    }

    pub fn version(&self) -> &OpenApiVersion {
        &self.version
    }
    pub fn new(specification: Value) -> Result<Self, ()> {
        let version = match specification.get("openapi") {
            None => return Err(()),
            Some(version_field) => {
                if let Some(version_field) = version_field.as_str() {
                    version_field
                } else {
                    return Err(())
                }
            }
        };
        if let Ok(version) = OpenApiVersion::from_str(version) {
            if let Err(_) = version.is_valid(&specification) {
                return Err(())
            }
            match version {
                OpenApiVersion::V30X => {

                    // TODO - implement 3.0.x validator
                    if let Ok(specification) = Self::create_openapi_v30x(specification) {
                        return Ok(Self {
                            traverser: OpenApiValidators::V30X(specification),
                            validators: HashMap::new(),
                            version
                        });
                    }
                }
                OpenApiVersion::V31X => {
                    if let Ok(specification) = Self::create_openapi_v31x(specification) {
                        return Ok(Self {
                            traverser: OpenApiValidators::V31X(specification),
                            validators: HashMap::new(),
                            version
                        });
                    }
                }
            }
        }
        Err(())
    }

    fn validate_request_body(&self, path: &str, method: &str, body: &Value) -> Result<(), ()> {
        todo!()
    }

}



#[cfg(test)]
mod test {
    use std::fs;
    use serde_json::Value;
    use crate::request_validator::{RequestValidator, OpenApiVersion};

    #[test]
    fn test_spec_node() {
        let spec = fs::read_to_string("./test/openapi.json").unwrap();
        let openapi_spec = fs::read_to_string("resource/openapi-v3.0.x.json").unwrap();
        let spec: Value = serde_json::from_str(&spec).unwrap();
        let openapi_spec: Value = serde_json::from_str(&openapi_spec).unwrap();
        let res = jsonschema::draft4::validate(&openapi_spec, &spec);
        match res {
            Ok(_) => assert!(true, "validation should pass"),
            Err(er) => assert!(false, "validation failed with err: {er:?}")
        }
    }

    #[test]
    fn test_new_validator() {
        let spec = fs::read_to_string("./test/openapi.json").unwrap();
        let spec: Value = serde_json::from_str(&spec).unwrap();
        let test_instance = RequestValidator::new(spec);
        assert!(test_instance.is_ok());
        let test_instance = test_instance.unwrap();
        assert_eq!(OpenApiVersion::V30X, test_instance.version().clone());
    }


}