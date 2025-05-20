use crate::OpenApiValidationError;
use crate::openapi_util::JsonPath;
use crate::spec_validator::SpecValidator;
use jsonschema::Validator;
use oas3::Spec;
use oas3::spec::{
    ObjectOrReference, ObjectSchema, Operation, Parameter, ParameterIn, PathItem, SchemaType,
    SchemaTypeSet,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};
use std::str::FromStr;

pub struct OpenApiV31xValidator {
    specification: Spec,
    root_schema: Value,
    validators: HashMap<String, Validator>,
}

const PATH_SPLIT: char = '/';
const PATHS_KEY: &'static str = "paths";
const REQUEST_BODY_KEY: &'static str = "requestBody";
const CONTENT_KEY: &'static str = "content";
const SCHEMA_KEY: &'static str = "schema";
const ROOT_SCHEMA_ID: &'static str = "@@root";

impl OpenApiV31xValidator {
    pub fn new(root_schema: Value) -> Result<Self, ()> {
        let mut root_schema = root_schema;
        let spec: Spec = match serde_json::from_value(root_schema.clone()) {
            Ok(spec) => spec,
            Err(_) => return Err(()),
        };
        root_schema["$id"] = json!(ROOT_SCHEMA_ID);
        Ok(Self {
            specification: spec,
            root_schema,
            validators: HashMap::new(),
        })
    }
    fn build_request_body_path(
        &self,
        operation: &Operation,
        headers: &HashMap<String, String>,
        path: &JsonPath,
    ) -> Result<JsonPath, OpenApiValidationError> {
        let mut request_body_path = path.clone();
        let content_type_header = match headers
            .into_iter()
            .find(|(header_name, _)| header_name.to_lowercase().starts_with("content-type"))
        {
            None => {
                return Err(OpenApiValidationError::InvalidRequest(
                    "No content type provided".to_string(),
                ));
            }
            Some((_, header_value)) => header_value,
        };

        let binding = content_type_header.split(";").collect::<Vec<&str>>();
        let content_type_header = match binding.iter().find(|header_value| {
            header_value.starts_with("text")
                || header_value.starts_with("application")
                || header_value.starts_with("multipart")
        }) {
            None => {
                return Err(OpenApiValidationError::InvalidContentType(format!(
                    "Invalid content type provided: {}",
                    content_type_header
                )));
            }
            Some(header_value) => header_value,
        };

        let (_, path) =
            Self::resolve_request_body(&self.specification, operation, content_type_header)
                .ok_or_else(|| "Failed to resolve the request body schema".to_string())
                .unwrap();

        request_body_path.append_path(path);
        Ok(request_body_path)
    }

    fn resolve_request_body(
        spec: &Spec,
        operation: &Operation,
        content_type: &str,
    ) -> Option<(ObjectSchema, JsonPath)> {
        let request_body_ref = operation.request_body.as_ref()?.resolve(&spec).ok()?;
        let mut json_path = JsonPath::new();
        json_path.add_segment(REQUEST_BODY_KEY.to_string());

        let content = request_body_ref.content.get(content_type)?;
        json_path.add_segment(CONTENT_KEY.to_string());
        json_path.add_segment(content_type.to_string());

        let schema = content.schema.as_ref()?.resolve(&spec).ok()?;
        json_path.add_segment(SCHEMA_KEY.to_string());

        Some((schema, json_path))
    }

    fn validate_request_query_params(
        spec: &Spec,
        operation: &Operation,
        query_params: &HashMap<String, String>,
        _json_path: &JsonPath,
    ) -> Result<(), OpenApiValidationError> {
        match Self::filter_and_validate_params(
            query_params,
            ParameterIn::Query,
            &operation.parameters,
            &spec,
        ) {
            true => Ok(()),
            false => Err(OpenApiValidationError::InvalidHeaders(
                "Validation failed".to_string(),
            )),
        }
    }

    pub fn find_matching_operation<'a>(
        path_to_match: &'a str,
        method_to_match: &'a str,
        spec: &'a Spec,
        path_param: bool,
    ) -> Option<(&'a Operation, JsonPath)> {
        let spec_paths = match &spec.paths {
            Some(paths) => paths,
            None => return None,
        };

        if path_param {
            Self::detailed_path_search(path_to_match, method_to_match, spec_paths, spec)
        } else {
            if let Some(op) = spec.operation(
                &http::method::Method::from_str(method_to_match).unwrap(),
                path_to_match,
            ) {
                let mut path = JsonPath::new();
                path.add_segment(PATHS_KEY.to_string())
                    .add_segment(path_to_match.to_string())
                    .add_segment(method_to_match.to_lowercase().to_string());
                return Some((op, path));
            }
            None
        }
    }

    fn detailed_path_search<'a>(
        path_to_match: &'a str,
        method_to_match: &'a str,
        paths: &'a BTreeMap<String, PathItem>,
        spec: &'a Spec,
    ) -> Option<(&'a Operation, JsonPath)> {
        // Find the matching method
        for (spec_path, path_item) in paths.iter() {
            if let Some((_, op)) = path_item
                .methods()
                .into_iter()
                .find(|(method, _)| method.as_str() == method_to_match)
            {
                // Perform our check to see if this matches
                let path_method_item_params = &op.parameters;
                if Self::match_openapi_endpoint_path_segments(
                    path_to_match,
                    spec_path,
                    path_method_item_params,
                    spec,
                ) {
                    let mut path = JsonPath::new();
                    path.add_segment(PATHS_KEY.to_string())
                        .add_segment(spec_path.to_string())
                        .add_segment(method_to_match.to_lowercase().to_string());
                    return Some((op, path));
                }
            }
        }
        None
    }

    fn match_openapi_endpoint_path_segments(
        target_path: &str,
        spec_path: &str,
        path_method_item_params: &Vec<ObjectOrReference<Parameter>>,
        spec: &Spec,
    ) -> bool {
        let target_segments = target_path.split(PATH_SPLIT).collect::<Vec<&str>>();
        let spec_segments = spec_path.split(PATH_SPLIT).collect::<Vec<&str>>();

        // The number of segments in the path and the number of segments that match the given path.
        // If the numbers are equal, it means we've found a match.
        let (matching_segments, segment_count) =
            spec_segments.iter().zip(target_segments.iter()).fold(
                (0, 0),
                |(mut matches, mut count), (spec_segment, target_segment)| {
                    count += 1;

                    // If the path in the spec contains a path parameter,
                    // we need to make sure the value in the given_path's value at the segment
                    // follows the schema rules defined in the specification.
                    // If the validation fails, we do not consider it a match.
                    if let Some(param_name) =
                        Self::extract_openapi_path_parameter_name(spec_segment)
                    {
                        match Self::path_parameter_value_matches_type(
                            param_name,
                            target_segment,
                            path_method_item_params,
                            spec,
                        ) {
                            Ok(_) => matches += 1,
                            Err(_) => return (matches, count),
                        }

                    // The case where we check to see if the segment values are the same (non-path parameter)
                    } else if spec_segment == target_segment {
                        matches += 1;
                    }

                    (matches, count)
                },
            );
        matching_segments == segment_count
    }

    fn path_parameter_value_matches_type(
        param_name: &str,
        target_segment: &str,
        path_method_item_params: &Vec<ObjectOrReference<Parameter>>,
        spec: &Spec,
    ) -> Result<(), OpenApiValidationError> {
        if let Some(param) =
            Self::get_path_parameter_definition(param_name, path_method_item_params, spec)
        {
            if let Some(resolved_schema) =
                param.schema.and_then(|schema| schema.resolve(&spec).ok())
            {
                if let Ok(value) =
                    Self::try_cast_path_param_to_schema_type(target_segment, &resolved_schema)
                {
                    let value = &value;
                    let res = Self::validate_with_schema(value, &resolved_schema);
                    return res;
                }
            }
        }
        Ok(())
    }

    fn get_path_parameter_definition(
        param_name: &str,
        endpoint_params: &Vec<ObjectOrReference<Parameter>>,
        spec: &Spec,
    ) -> Option<Parameter> {
        // Look through each parameter for the operation, if the 'name' field value
        // matches the provided 'param_name' then return that.
        // Returns None if there is no matching parameter schema for the operation.
        endpoint_params.iter().find_map(|param| {
            param.resolve(&spec).ok().and_then(|param| {
                if param_name == param.name.as_str() {
                    Some(param.clone())
                } else {
                    None
                }
            })
        })
    }

    fn try_cast_to_type(target_segment: &str, schema_type: &SchemaType) -> Result<Value, ()> {
        match schema_type {
            SchemaType::Boolean => {
                let cast: bool = target_segment.parse().unwrap();
                Ok(json!(cast))
            }
            SchemaType::Integer => {
                let cast: i64 = target_segment.parse().unwrap();
                Ok(json!(cast))
            }
            SchemaType::Number => {
                let cast: f64 = target_segment.parse().unwrap();
                Ok(json!(cast))
            }
            SchemaType::String => Ok(json!(target_segment)),

            // invalid type for path parameter
            _ => Err(()),
        }
    }

    fn try_cast_path_param_to_schema_type(
        target_segment: &str,
        schema: &ObjectSchema,
    ) -> Result<Value, ()> {
        let param_type = schema.schema_type.as_ref().unwrap();
        match param_type {
            SchemaTypeSet::Single(single) => Self::try_cast_to_type(target_segment, &single),
            SchemaTypeSet::Multiple(multi) => {
                for m_type in multi {
                    let res = Self::try_cast_to_type(target_segment, &m_type);
                    if res.is_ok() {
                        return res;
                    }
                }
                Err(())
            }
        }
    }

    fn validate_request_headers(
        spec: &Spec,
        operation: &Operation,
        headers: &HashMap<String, String>,
        _json_path: &JsonPath,
    ) -> Result<(), OpenApiValidationError> {
        match Self::filter_and_validate_params(
            headers,
            ParameterIn::Header,
            &operation.parameters,
            &spec,
        ) {
            true => Ok(()),
            false => Err(OpenApiValidationError::InvalidQueryParameters(
                "Validation failed".to_string(),
            )),
        }
    }

    fn filter_and_validate_params(
        given_parameters: &HashMap<String, String>,
        given_parameter_type: ParameterIn,
        operation_parameters: &Vec<ObjectOrReference<Parameter>>,
        spec: &Spec,
    ) -> bool {
        // Filters the current operation parameters to the ones that have the matching
        // 'ParameterIn' type. I.e. ParameterIn::Header, ParameterIn::Query, etc.
        let relevant_parameters = operation_parameters
            .iter()
            .filter(|param| {
                param
                    .resolve(&spec)
                    .is_ok_and(|param| param.location == given_parameter_type)
            })
            .collect::<Vec<&ObjectOrReference<Parameter>>>();

        Self::validate_operation_parameters(given_parameters, &relevant_parameters, &spec)
    }

    fn validate_operation_parameters(
        given_parameters: &HashMap<String, String>,
        operation_parameters_sub_set: &Vec<&ObjectOrReference<Parameter>>,
        specification: &Spec,
    ) -> bool {
        for parameter in operation_parameters_sub_set {
            if let Ok(resolved_param) = parameter.resolve(&specification) {
                if let Some((_, param_value)) = given_parameters
                    .iter()
                    .find(|(param_key, _)| param_key.as_str() == resolved_param.name)
                {
                    if let Some(_) = resolved_param.content {
                    } else if let Some(schema) = resolved_param.schema {
                        if let Ok(schema) = schema.resolve(&specification) {
                            if let Err(_) = Self::validate_with_schema(
                                &Value::String(param_value.clone()),
                                &schema,
                            ) {
                                return false;
                            }
                        }
                    }

                /* If the header is not found, check to see if it's required or not. */
                } else if resolved_param.required.unwrap_or(false) {
                    return false;
                }
            }
        }
        true
    }

    fn validate_with_schema(
        value: &Value,
        schema: &ObjectSchema,
    ) -> Result<(), OpenApiValidationError> {
        let schema_as_value = match Self::object_schema_to_value(schema) {
            Ok(val) => val,
            Err(e) => return Err(e),
        };
        match jsonschema::validate(&schema_as_value, value) {
            Ok(_) => Ok(()),
            Err(e) => Err(OpenApiValidationError::InvalidSchema(format!(
                "Invalid schema: {}",
                e.to_string()
            ))),
        }
    }

    fn object_schema_to_value(schema: &ObjectSchema) -> Result<Value, OpenApiValidationError> {
        match serde_json::to_value(schema) {
            Ok(val) => Ok(val),
            Err(e) => Err(OpenApiValidationError::InvalidSchema(format!(
                "Failed to convert schema to value: {}",
                e.to_string()
            ))),
        }
    }
}

impl SpecValidator for OpenApiV31xValidator {
    fn validate_request(
        &self,
        path: &str,
        method: &str,
        body: Option<&Value>,
        headers: Option<&HashMap<String, String>>,
        query_params: Option<&HashMap<String, String>>,
    ) -> Result<(), OpenApiValidationError> {
        let (operation, path) =
            match Self::find_matching_operation(path, method, &self.specification, true) {
                Some((operation, path)) => (operation, path),
                None => {
                    return Err(OpenApiValidationError::InvalidPath(format!(
                        "Could not find matching operation for provided path: {}",
                        path
                    )));
                }
            };

        // A body was provided, so we validate it.
        // This also means the headers must also be provided because we need to find out the content-type
        // This is used to find out which schema to validate against in the openapi specification.
        let body_and_path: Option<(&Value, JsonPath)> = match (body, headers) {
            (Some(body), Some(headers)) => {
                let request_body_path =
                    match self.build_request_body_path(&operation, headers, &path) {
                        Ok(path) => path,
                        Err(e) => return Err(e),
                    };
                Some((body, request_body_path))
            }
            (Some(_), None) => {
                return Err(OpenApiValidationError::InvalidRequest(
                    "No content type provided".to_string(),
                ));
            }
            (_, _) => None,
        };

        let body_result = match body_and_path {
            None => Ok(()),
            Some((body, path)) => self.validate_schema_from_pointer(body, &path),
        };

        if let Err(e) = body_result {
            return Err(e);
        }

        if let Some(headers) = headers {
            if let Err(e) =
                Self::validate_request_headers(&self.specification, &operation, headers, &path)
            {
                return Err(e);
            }
        }

        if let Some(query_params) = query_params {
            if let Err(e) = Self::validate_request_query_params(
                &self.specification,
                &operation,
                query_params,
                &path,
            ) {
                return Err(e);
            }
        }

        Ok(())
    }

    fn validators(&self) -> &HashMap<String, Validator> {
        &self.validators
    }

    fn validators_mut(&mut self) -> &mut HashMap<String, Validator> {
        &mut self.validators
    }

    fn root_schema(&self) -> &Value {
        &self.root_schema
    }
}

#[cfg(test)]
mod test {
    use crate::openapi_v31x::OpenApiV31xValidator;
    use crate::spec_validator::SpecValidator;
    use oas3::spec::{ObjectOrReference, ObjectSchema, SchemaType, SchemaTypeSet};
    use serde_json::{Value, json};
    use std::collections::HashMap;
    use std::fs;

    #[test]
    fn test_post_validation() {
        let test_request_path = "/pet";
        let test_request_method = "POST";
        let mut test_request_headers: HashMap<String, String> = HashMap::new();
        test_request_headers.insert("Accept".to_string(), "application/json".to_string());
        test_request_headers.insert("Content-Type".to_string(), "application/json".to_string());

        let post_body = json!({
            "id": 1,
            "category": {
              "id": 1,
              "name": "cat"
            },
            "name": "fluffy",
            "photoUrls": [
              "http://example.com/path/to/cat/1.jpg",
              "http://example.com/path/to/cat/2.jpg"
            ],
            "tags": [
              {
                "id": 1,
                "name": "cat"
              }
            ],
            "status": "available"
        });
        let spec = fs::read_to_string("test/openapi.json").unwrap();
        let spec: Value = serde_json::from_str(&spec).unwrap();
        let validator = OpenApiV31xValidator::new(spec).unwrap();
        let result = validator.validate_request(
            test_request_path,
            test_request_method,
            Some(&post_body),
            Some(&test_request_headers),
            None::<&HashMap<String, String>>,
        );
        match result {
            Ok(_) => assert!(true, "validation should pass"),
            Err(e) => assert!(false, "validation failed: {e:?}"),
        }

        let invalid_post_body = json!({
            "id": 1,
            "category": {
              "id": 1,
              "name": "cat"
            },
            "name": "fluffy",
            "invalid_field": [
              "http://example.com/path/to/cat/1.jpg",
              "http://example.com/path/to/cat/2.jpg"
            ],
            "tags": [
              {
                "id": 1,
                "name": "cat"
              }
            ],
            "status": "available"
        });
        let result = validator.validate_request(
            test_request_path,
            test_request_method,
            Some(&invalid_post_body),
            Some(&test_request_headers),
            None::<&HashMap<String, String>>,
        );
        assert!(!result.is_ok());
    }

    #[test]
    fn test_get_validation() {
        let test_request_path = "/pet/findById/123";
        let test_request_method = "GET";
        let mut test_request_headers: HashMap<String, String> = HashMap::new();
        test_request_headers.insert("Accept".to_string(), "application/json".to_string());

        let spec = fs::read_to_string("test/openapi.json").unwrap();
        let spec: Value = serde_json::from_str(&spec).unwrap();
        let validator = OpenApiV31xValidator::new(spec).unwrap();
        let result = validator.validate_request(
            test_request_path,
            test_request_method,
            None,
            Some(&test_request_headers),
            None::<&HashMap<String, String>>,
        );

        assert!(result.is_ok());
    }

    /// Example schema taken from: https://swagger.io/docs/specification/v3_0/data-models/oneof-anyof-allof-not/
    #[test]
    fn test_validate_object_rules_all_of() {
        //let file = fs::read_to_string("test/openapi.json").unwrap();

        let mut pet_type_schema = ObjectSchema::default();
        pet_type_schema.schema_type = Some(SchemaTypeSet::Single(SchemaType::String));

        let mut pet_props = ObjectSchema::default();
        pet_props.schema_type = Some(SchemaTypeSet::Single(SchemaType::Object));
        pet_props.required = vec!["pet_type".to_string()];
        pet_props.properties.insert(
            "pet_type".to_string(),
            ObjectOrReference::Object(pet_type_schema),
        );

        let mut cat_hunting_schema = ObjectSchema::default();
        cat_hunting_schema.schema_type = Some(SchemaTypeSet::Single(SchemaType::Boolean));

        let mut cat_age_schema = ObjectSchema::default();
        cat_age_schema.schema_type = Some(SchemaTypeSet::Single(SchemaType::Integer));

        let mut cat_props = ObjectSchema::default();
        cat_props.schema_type = Some(SchemaTypeSet::Single(SchemaType::Object));
        cat_props.required = vec!["hunts".to_string(), "age".to_string()];
        cat_props.properties.insert(
            "hunts".to_string(),
            ObjectOrReference::Object(cat_hunting_schema),
        );
        cat_props
            .properties
            .insert("age".to_string(), ObjectOrReference::Object(cat_age_schema));

        let mut cat_schema = ObjectSchema::default();
        cat_schema.all_of.push(ObjectOrReference::Object(pet_props));
        cat_schema.all_of.push(ObjectOrReference::Object(cat_props));

        // has pet_type, and all cat schema props
        let valid_request_body = json!({
            "pet_type": "Cat",
            "hunts": true,
            "age": 9
        });

        assert!(
            OpenApiV31xValidator::validate_with_schema(&valid_request_body, &cat_schema).is_ok()
        );

        // Missing pet_type
        let invalid_request_body = json!({
            "age": 3,
            "hunts": true
        });

        assert!(
            !OpenApiV31xValidator::validate_with_schema(&invalid_request_body, &cat_schema).is_ok()
        );

        // Cat schema does not have 'bark' property, but additional properties are allowed
        let invalid_request_body = json!({
            "pet_type": "Cat",
            "age": 3,
            "hunts": true,
            "bark": true
        });

        assert!(
            OpenApiV31xValidator::validate_with_schema(&invalid_request_body, &cat_schema).is_ok()
        );
    }
}
