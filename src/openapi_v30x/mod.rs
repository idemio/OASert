//use crate::OpenApiValidationError;
//use crate::openapi_util::JsonPath;
//use crate::spec_validator::OpenApiValidator;
//use jsonschema::Validator;
//use openapiv3::{OpenAPI, Operation, Parameter, ParameterSchemaOrContent, ReferenceOr, Schema, SchemaKind, Type};
//use serde_json::{Value, json};
//use std::collections::HashMap;
//
//pub struct OpenApiV30xValidator {
//    specification: OpenAPI,
//    root_schema: Value,
//    validators: HashMap<String, Validator>,
//}
//const ROOT_SCHEMA_ID: &'static str = "@@root";
//const PATHS_KEY: &'static str = "paths";
//const PATH_SPLIT: char = '/';
//const REQUEST_BODY_KEY: &'static str = "requestBody";
//const CONTENT_KEY: &'static str = "content";
//const SCHEMA_KEY: &'static str = "schema";
//impl OpenApiV30xValidator {
//    pub fn new(root_schema: Value) -> Result<Self, ()> {
//        let mut root_schema = root_schema;
//        let spec: OpenAPI = match serde_json::from_value(root_schema.clone()) {
//            Ok(spec) => spec,
//            Err(_) => return Err(()),
//        };
//        root_schema["$id"] = json!(ROOT_SCHEMA_ID);
//        Ok(Self {
//            specification: spec,
//            root_schema,
//            validators: HashMap::new(),
//        })
//    }
//
//    fn build_request_body_path(
//        &self,
//        operation: &Operation,
//        headers: &HashMap<String, String>,
//        path: &JsonPath,
//    ) -> Result<JsonPath, OpenApiValidationError> {
//        let mut request_body_path = path.clone();
//        let content_type_header = match headers
//            .into_iter()
//            .find(|(header_name, _)| header_name.to_lowercase().starts_with("content-type"))
//        {
//            None => {
//                return Err(OpenApiValidationError::InvalidRequest(
//                    "No content type provided".to_string(),
//                ));
//            }
//            Some((_, header_value)) => header_value,
//        };
//
//        let binding = content_type_header.split(";").collect::<Vec<&str>>();
//        let content_type_header = match binding.iter().find(|header_value| {
//            header_value.starts_with("text")
//                || header_value.starts_with("application")
//                || header_value.starts_with("multipart")
//        }) {
//            None => {
//                return Err(OpenApiValidationError::InvalidContentType(format!(
//                    "Invalid content type provided: {}",
//                    content_type_header
//                )));
//            }
//            Some(header_value) => header_value,
//        };
//
//        let (_, path) =
//            Self::resolve_request_body(&self.specification, operation, content_type_header)
//                .ok_or_else(|| "Failed to resolve the request body schema".to_string())
//                .unwrap();
//
//        request_body_path.append_path(path);
//        Ok(request_body_path)
//    }
//
//    fn validate_request_headers(
//        spec: &OpenAPI,
//        operation: &Operation,
//        headers: &HashMap<String, String>,
//        _json_path: &JsonPath,
//    ) -> Result<(), OpenApiValidationError> {
//        todo!()
//    }
//
//    fn validate_request_query_params(
//        spec: &OpenAPI,
//        operation: &Operation,
//        query_params: &HashMap<String, String>,
//        _json_path: &JsonPath,
//    ) -> Result<(), OpenApiValidationError> {
//        todo!()
//    }
//
//    fn resolve_request_body(
//        spec: &OpenAPI,
//        operation: &Operation,
//        content_type: &str,
//    ) -> Option<(Schema, JsonPath)> {
//        let request_body_ref = operation.request_body.as_ref()?.as_item()?;
//        let mut json_path = JsonPath::new();
//        json_path.add_segment(REQUEST_BODY_KEY.to_string());
//
//        let content = request_body_ref.content.get(content_type)?;
//        json_path.add_segment(CONTENT_KEY.to_string());
//        json_path.add_segment(content_type.to_string());
//
//        let schema = content.schema.as_ref()?.as_item()?.clone();
//        json_path.add_segment(SCHEMA_KEY.to_string());
//
//        Some((schema, json_path))
//    }
//
//    pub fn find_matching_operation<'a>(
//        path_to_match: &'a str,
//        method_to_match: &'a str,
//        spec: &'a OpenAPI,
//    ) -> Option<(&'a Operation, JsonPath)> {
//        let paths = &spec.paths;
//        let paths = paths.iter();
//        for (spec_path, op) in paths {
//            if let Some(op) = op.as_item() {
//                let potential_match = match method_to_match.to_lowercase().as_str() {
//                    "post" => &op.post,
//                    "get" => &op.get,
//                    "delete" => &op.delete,
//                    "head" => &op.head,
//                    "options" => &op.options,
//                    "patch" => &op.patch,
//                    "put" => &op.put,
//                    "trace" => &op.trace,
//                    &_ => return None,
//                };
//
//                if let Some(potential_match) = potential_match {
//                    // Perform our check to see if this matches
//                    let path_method_item_params = &op.parameters;
//                    if Self::match_openapi_endpoint_path_segments(
//                        path_to_match,
//                        spec_path,
//                        path_method_item_params,
//                        spec,
//                    ) {
//                        let mut path = JsonPath::new();
//                        path.add_segment(PATHS_KEY.to_string())
//                            .add_segment(spec_path.to_string())
//                            .add_segment(method_to_match.to_lowercase().to_string());
//                        return Some((potential_match, path));
//                    }
//                }
//            }
//        }
//        None
//    }
//
//    fn match_openapi_endpoint_path_segments(
//        target_path: &str,
//        spec_path: &str,
//        path_method_item_params: &Vec<ReferenceOr<Parameter>>,
//        spec: &OpenAPI,
//    ) -> bool {
//        let target_segments = target_path.split(PATH_SPLIT).collect::<Vec<&str>>();
//        let spec_segments = spec_path.split(PATH_SPLIT).collect::<Vec<&str>>();
//
//        // The number of segments in the path and the number of segments that match the given path.
//        // If the numbers are equal, it means we've found a match.
//        let (matching_segments, segment_count) =
//            spec_segments.iter().zip(target_segments.iter()).fold(
//                (0, 0),
//                |(mut matches, mut count), (spec_segment, target_segment)| {
//                    count += 1;
//
//                    if let Some(param_name) =
//                        Self::extract_openapi_path_parameter_name(spec_segment)
//                    {
//                        match Self::path_parameter_value_matches_type(
//                            param_name,
//                            target_segment,
//                            path_method_item_params,
//                            spec,
//                        ) {
//                            Ok(_) => matches += 1,
//                            Err(_) => return (matches, count),
//                        }
//
//                    // The case where we check to see if the segment values are the same (non-path parameter)
//                    } else if spec_segment == target_segment {
//                        matches += 1;
//                    }
//
//                    (matches, count)
//                },
//            );
//        matching_segments == segment_count
//    }
//
//    fn path_parameter_value_matches_type(
//        param_name: &str,
//        target_segment: &str,
//        path_method_item_params: &Vec<ReferenceOr<Parameter>>,
//        spec: &OpenAPI,
//    ) -> Result<(), OpenApiValidationError> {
//        if let Some(param) =
//            Self::get_path_parameter_definition(param_name, path_method_item_params)
//        {
//            match &param.parameter_data_ref().format {
//                ParameterSchemaOrContent::Schema(schema) => {
//                    if let Some(schema) = schema.as_item() {
//                        match &schema.schema_kind {
//                            SchemaKind::Type(param_schema) => {
//                                if let Ok(value) =
//                                    Self::try_cast_path_param_to_schema_type(target_segment, &param_schema)
//                                {
//                                    let value = &value;
//                                    let res = Self::validate_with_schema(value, &param_schema);
//                                    return res;
//                                }
//                            }
//                            _ => return Err(OpenApiValidationError::InvalidSchema("Header/Query parameters should not contain anyOf, onOf, allOf, etc.".to_string()))
//                        }
//                    }
//                }
//                _ => {
//                    return Err(OpenApiValidationError::InvalidSchema(
//                        "Header/query parameter missing schema".to_string(),
//                    ));
//                }
//            }
//        }
//        Ok(())
//    }
//
//    fn validate_with_schema(value: &Value, schema: &Type) -> Result<(), OpenApiValidationError> {
//        let schema_as_value = match Self::object_schema_to_value(schema) {
//            Ok(val) => val,
//            Err(e) => return Err(e),
//        };
//        match jsonschema::validate(&schema_as_value, value) {
//            Ok(_) => Ok(()),
//            Err(e) => Err(OpenApiValidationError::InvalidSchema(format!(
//                "Invalid schema: {}",
//                e.to_string()
//            ))),
//        }
//    }
//
//    fn object_schema_to_value(schema: &Type) -> Result<Value, OpenApiValidationError> {
//        match serde_json::to_value(schema) {
//            Ok(val) => Ok(val),
//            Err(e) => Err(OpenApiValidationError::InvalidSchema(format!(
//                "Failed to convert schema to value: {}",
//                e.to_string()
//            ))),
//        }
//    }
//
//    fn try_cast_path_param_to_schema_type(
//        target_segment: &str,
//        param_type: &Type,
//    ) -> Result<Value, ()> {
//        match param_type {
//            Type::String(_) => Ok(json!(target_segment)),
//            Type::Number(_) => {
//                let cast: f64 = target_segment.parse().unwrap();
//                Ok(json!(cast))
//            }
//            Type::Integer(_) => {
//                let cast: i64 = target_segment.parse().unwrap();
//                Ok(json!(cast))
//            }
//            Type::Boolean(_) => {
//                let cast: bool = target_segment.parse().unwrap();
//                Ok(json!(cast))
//            }
//            _ => Err(()),
//        }
//    }
//
//    fn get_path_parameter_definition(
//        param_name: &str,
//        endpoint_params: &Vec<ReferenceOr<Parameter>>,
//    ) -> Option<Parameter> {
//        // Look through each parameter for the operation, if the 'name' field value
//        // matches the provided 'param_name' then return that.
//        // Returns None if there is no matching parameter schema for the operation.
//        endpoint_params.iter().find_map(|param| {
//            param.as_item().and_then(|param| {
//                if param_name == param.parameter_data_ref().name.as_str() {
//                    Some(param.clone())
//                } else {
//                    None
//                }
//            })
//        })
//    }
//}
//
//impl OpenApiValidator for OpenApiV30xValidator {
//    fn validate_request(
//        &self,
//        path: &str,
//        method: &str,
//        body: Option<&Value>,
//        headers: Option<&HashMap<String, String>>,
//        query_params: Option<&HashMap<String, String>>,
//    ) -> Result<(), OpenApiValidationError> {
//        let (operation, path) = match Self::find_matching_operation(path, method, &self.specification) {
//            Some((op, path)) => (op, path),
//            None => {
//                return Err(OpenApiValidationError::InvalidPath(format!(
//                    "Could not find matching operation for provided path: {}",
//                    path
//                )));
//            }
//        };
//
//        let body_and_path: Option<(&Value, JsonPath)> = match (body, headers) {
//            (Some(body), Some(headers)) => {
//                let request_body_path =
//                    match self.build_request_body_path(&operation, headers, &path) {
//                        Ok(path) => path,
//                        Err(e) => return Err(e),
//                    };
//                Some((body, request_body_path))
//            }
//            (Some(_), None) => {
//                return Err(OpenApiValidationError::InvalidRequest(
//                    "No content type provided".to_string(),
//                ));
//            }
//            (_, _) => None,
//        };
//
//        let body_result = match body_and_path {
//            None => Ok(()),
//            Some((body, path)) => self.validate_schema_from_pointer(body, &path),
//        };
//
//        if let Err(e) = body_result {
//            return Err(e);
//        }
//
//        if let Some(headers) = headers {
//            if let Err(e) =
//                Self::validate_request_headers(&self.specification, &operation, headers, &path)
//            {
//                return Err(e);
//            }
//        }
//
//        if let Some(query_params) = query_params {
//            if let Err(e) = Self::validate_request_query_params(
//                &self.specification,
//                &operation,
//                query_params,
//                &path,
//            ) {
//                return Err(e);
//            }
//        }
//
//
//        todo!()
//        //let (operation, path) = match
//    }
//
//    fn validators(&self) -> &HashMap<String, Validator> {
//        &self.validators
//    }
//
//    fn validators_mut(&mut self) -> &mut HashMap<String, Validator> {
//        &mut self.validators
//    }
//
//    fn root_schema(&self) -> &Value {
//        &self.root_schema
//    }
//}
