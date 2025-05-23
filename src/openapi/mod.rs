use crate::OpenApiValidationError;
use crate::openapi_util::JsonPath;
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};

pub struct OpenApiValidatorV2 {
    traverser: OpenApiTraverser,
}

impl OpenApiValidatorV2 {
    pub fn new(value: Value) -> Self {
        Self {
            traverser: OpenApiTraverser::new(value),
        }
    }

    fn get_operation(&self, path: &str, method: &str) -> Option<(&Value, JsonPath)> {
        if let Some(spec_paths) = &self.traverser.get_from_spec_as_map("paths") {
            for (spec_path, op) in spec_paths.iter() {
                let path_params = op.get("parameters").and_then(|v| v.as_array());
                if Self::matches_spec_path(path_params, path, spec_path) {
                    let mut json_path = JsonPath::new();
                    json_path
                        .add_segment("paths".to_string())
                        .add_segment(spec_path.to_string())
                        .add_segment(method.to_lowercase().to_string());
                    return Some((op, json_path));
                }
            }
        }
        None
    }

    fn matches_spec_path(
        _path_params: Option<&Vec<Value>>,
        path_to_match: &str,
        spec_path: &str,
    ) -> bool {
        if !(spec_path.contains("{") && spec_path.contains("}")) {
            spec_path == path_to_match
        } else {
            let target_segments = path_to_match.split('/').collect::<Vec<&str>>();
            let spec_segments = spec_path.split('/').collect::<Vec<&str>>();
            if spec_segments.len() != target_segments.len() {
                return false;
            }

            let (matching_segments, segment_count) =
                spec_segments.iter().zip(target_segments.iter()).fold(
                    (0, 0),
                    |(mut matches, mut count), (spec_segment, target_segment)| {
                        count += 1;
                        if let Some(_) = spec_segment.find("{").and_then(|start| {
                            spec_segment
                                .find("}")
                                .map(|end| &spec_segment[start + 1..end])
                        }) {
                            // assume the path param type matches
                            matches += 1;
                        } else if spec_segment == target_segment {
                            matches += 1;
                        }

                        (matches, count)
                    },
                );
            matching_segments == segment_count
        }
    }

    fn validate_body(
        &self,
        endpoint_operation: &Value,
        body: &Value,
        headers: &HashMap<String, String>,
    ) -> Result<(), OpenApiValidationError> {
        todo!()
    }

    fn check_required_body(
        &self,
        body_schema: &Value,
        body: Option<&Value>,
    ) -> Result<(), OpenApiValidationError> {
        todo!()
    }

    fn validate_headers(
        &self,
        request_params: &Vec<Value>,
        headers: &HashMap<String, String>,
    ) -> Result<(), OpenApiValidationError> {
        todo!()
    }

    fn check_required_header_params(
        &self,
        request_params: &Vec<Value>,
        headers: Option<&HashMap<String, String>>,
    ) -> Result<(), OpenApiValidationError> {
        todo!()
    }

    fn validate_query_params(
        &self,
        request_params: &Vec<Value>,
        query_params: &HashMap<String, String>,
    ) -> Result<(), OpenApiValidationError> {
        todo!()
    }

    fn check_required_query_params(
        &self,
        query_schemas: &Vec<Value>,
        query_params: Option<&HashMap<String, String>>,
    ) -> Result<(), OpenApiValidationError> {
        todo!()
    }

    pub fn validate_request(
        &self,
        path: &str,
        method: &str,
        body: Option<&Value>,
        headers: Option<&HashMap<String, String>>,
        query_params: Option<&HashMap<String, String>>,
        scopes: Option<&HashSet<String>>,
    ) -> Result<(), OpenApiValidationError> {
        let (operation, path) = match self.get_operation(path, method) {
            None => {
                return Err(OpenApiValidationError::InvalidPath(
                    "No path found".to_string(),
                ));
            }
            Some(val) => (val.0, val.1),
        };

        // TODO - get request body schema, headers, query params, path params first. then validate between them
        let request_schema: Option<&Value> = self.traverser.get_request_body(operation);
        let request_params: Option<&Vec<Value>> = self.traverser.get_request_parameters(operation);
        let request_security: Option<&Vec<Value>> = self.traverser.get_request_security(operation);

        if let Err(e) = match (body, headers, request_schema) {
            (Some(body), Some(headers), Some(request_body_schema)) => {
                self.validate_body(request_body_schema, body, headers)
            }
            (Some(_), None, Some(_)) => Err(OpenApiValidationError::InvalidRequest(
                "Request contains body but no content-type".to_string(),
            )),
            (Some(_), _, None) => Err(OpenApiValidationError::InvalidRequest(
                "Request body provided when endpoint has no request schema defined".to_string(),
            )),
            (_, _, _) => Ok(()),
        } {
            return Err(e);
        }

        if let Err(e) = match (headers, request_params) {
            (Some(headers), Some(request_params)) => self.validate_headers(request_params, headers),
            (None, Some(request_params)) => self.check_required_header_params(request_params, None),
            (Some(_), None) => Err(OpenApiValidationError::InvalidRequest(
                "Request parameters provided when endpoint has no parameters defined".to_string(),
            )),
            (_, _) => Ok(()),
        } {
            return Err(e);
        }

        if let Err(e) = match (query_params, request_params) {
            (Some(query), Some(request_params)) => {
                self.validate_query_params(request_params, query)
            }
            (None, Some(request_params)) => self.check_required_query_params(request_params, None),
            (Some(_), None) => Err(OpenApiValidationError::InvalidRequest(
                "Query parameters provided when endpoint has no parameters defined".to_string(),
            )),
            (_, _) => Ok(()),
        } {
            return Err(e);
        }

        todo!()
    }
}

pub struct OpenApiTraverser {
    _resolved_references: HashMap<String, Value>,
    specification: Value,
}

impl OpenApiTraverser {
    pub fn new(specification: Value) -> Self {
        Self {
            _resolved_references: HashMap::new(),
            specification,
        }
    }

    pub fn get_request_body(&self, operation: &Value) -> Option<&Value> {
        todo!()
    }

    pub fn get_request_parameters(&self, operation: &Value) -> Option<&Vec<Value>> {
        todo!()
    }

    pub fn get_request_security(&self, operation: &Value) -> Option<&Vec<Value>> {
        todo!()
    }

    pub fn get_from_spec_as_map<'a>(
        &'a self,
        path_or_node: &'a str,
    ) -> Option<&'a Map<String, Value>> {
        Self::get_as_map(&self.specification, &self.specification, path_or_node)
    }

    pub fn get_from_spec_as_array<'a>(&'a self, path_or_node: &'a str) -> Option<&'a Vec<Value>> {
        Self::get_as_array(&self.specification, &self.specification, path_or_node)
    }

    pub fn get_from_spec<'a>(&'a self, path_or_node: &'a str) -> Option<&'a Value> {
        Self::get(&self.specification, &self.specification, path_or_node)
    }

    pub fn get_as_map<'a>(
        specification: &'a Value,
        value: &'a Value,
        path_or_node: &'a str,
    ) -> Option<&'a Map<String, Value>> {
        if let Some(found_value) =
            Self::get(specification, value, path_or_node).and_then(|node| node.as_object())
        {
            return Some(found_value);
        }
        None
    }

    pub fn get_as_array<'a>(
        specification: &'a Value,
        value: &'a Value,
        node: &'a str,
    ) -> Option<&'a Vec<Value>> {
        if let Some(found_value) =
            Self::get(specification, value, node).and_then(|node| node.as_array())
        {
            return Some(found_value);
        }
        None
    }

    pub fn get<'a>(
        specification: &'a Value,
        value: &'a Value,
        path_or_node: &'a str,
    ) -> Option<&'a Value> {
        if let Some(value) = Self::check_for_ref(specification, value) {
            return if path_or_node.contains("/") {
                Self::get_path(specification, value, path_or_node)
            } else {
                Self::get_node(specification, value, path_or_node)
            };
        }
        None
    }

    pub fn get_node<'a>(
        specification: &'a Value,
        value: &'a Value,
        node: &'a str,
    ) -> Option<&'a Value> {
        if let Some(value) = Self::check_for_ref(specification, value) {
            return value.get(node);
        }
        None
    }

    pub fn get_path<'a>(
        specification: &'a Value,
        value: &'a Value,
        path: &'a str,
    ) -> Option<&'a Value> {
        let path = path.split("/").collect::<Vec<&str>>();
        let mut current_value = value;
        let mut seen_references: HashSet<&str> = HashSet::new();
        for index in 0..path.len() {
            if let Some(map_value) = current_value.as_object() {
                if let Some(ref_map_value) = map_value.get("$ref").and_then(|val| val.as_str()) {
                    if seen_references.contains(ref_map_value) {
                        return None;
                    }

                    match Self::resolve(specification, current_value, ref_map_value) {
                        None => return None,
                        Some(resolved) => {
                            seen_references.insert(ref_map_value);
                            current_value = resolved
                        }
                    }
                }
            }

            let segment = path.get(index).unwrap();
            if *segment == "#" {
                continue;
            }
            if let Some(next) = current_value.get(segment) {
                current_value = next;
            } else {
                return None;
            }
        }
        Some(current_value)
    }

    pub fn get_from_spec_path<'a>(&'a self, path: &'a str) -> Option<&'a Value> {
        Self::get_path(&self.specification, &self.specification, path)
    }

    fn check_for_ref<'a>(specification: &'a Value, node: &'a Value) -> Option<&'a Value> {
        if let Some(ref_string) = node.get("$ref").and_then(|val| val.as_str()) {
            return match Self::resolve(specification, node, ref_string) {
                None => None,
                Some(resolved) => Some(resolved),
            };
        }
        Some(node)
    }

    fn resolve<'a>(
        specification: &'a Value,
        value: &'a Value,
        ref_string: &'a str,
    ) -> Option<&'a Value> {
        if let Some(found_path) = Self::get_path(&specification, &value, ref_string) {
            Some(found_path)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod test {
    use crate::openapi::OpenApiValidatorV2;
    use crate::openapi_util::JsonPath;
    use serde_json::Value;
    use std::fs;

    #[test]
    fn test_find_operation() {
        let spec_string = fs::read_to_string("./test/openapi.json").unwrap();
        let specification: Value = serde_json::from_str(&spec_string).unwrap();
        let validator = OpenApiValidatorV2::new(specification);
        let result: Option<(&Value, JsonPath)> =
            validator.get_operation("/pet/findByStatus/MultipleExamples", "get");
        assert!(result.is_some());

        let result = result.unwrap();
        assert_eq!(
            "paths/~1pet~1findByStatus~1MultipleExamples/get",
            result.1.format_path()
        )
    }
}
