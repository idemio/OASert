use crate::OpenApiValidationError;
use crate::openapi_util::JsonPath;
use jsonschema::{Resource, ValidationError, ValidationOptions, Validator};
use serde_json::{Map, Value, json};
use std::collections::{HashMap, HashSet};
use unicase::UniCase;

fn vec_2_string(vec: &Vec<Value>) -> String {
    let mut out = String::new();
    for el in vec {
        out.push_str(&el.to_string());
    }
    out
}

pub struct OpenApiValidatorV2 {
    traverser: OpenApiTraverser,
    validator_options: ValidationOptions,
}

impl OpenApiValidatorV2 {
    pub fn new(mut value: Value) -> Result<Self, OpenApiValidationError> {
        value["$id"] = json!("@@root");

        let resource = match Resource::from_contents(value.clone()) {
            Ok(res) => res,
            Err(_) => {
                return Err(OpenApiValidationError::InvalidSchema(
                    "Invalid specification provided!".to_string(),
                ));
            }
        };
        let options = Validator::options().with_resource("@@inner", resource);

        Ok(Self {
            traverser: OpenApiTraverser::new(value),
            validator_options: options,
        })
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
        request_body_path: &JsonPath,
        request_schema: &Value,
        request_body: Option<&Value>,
    ) -> Result<(), OpenApiValidationError> {
        if let Err(e) = self.check_required_body(request_schema, request_body) {
            return Err(e);
        }
        if let Some(body) = request_body {
            return self.complex_validation(request_body_path, body);
        }
        Ok(())
    }

    fn check_required_body(
        &self,
        body_schema: &Value,
        request_body: Option<&Value>,
    ) -> Result<(), OpenApiValidationError> {
        if let Some(required_fields) = body_schema
            .get("required")
            .and_then(|required| required.as_array())
        {
            if !required_fields.is_empty() && request_body.is_none() {
                return Err(OpenApiValidationError::InvalidRequest(format!(
                    "Request schema contains required fields {} but provided request is empty.",
                    vec_2_string(required_fields)
                )));
            }
            for required in required_fields {
                let required_field = required.as_str().unwrap();
                if request_body.is_some_and(|body| body.get(required_field).is_none()) {
                    return Err(OpenApiValidationError::InvalidRequest(format!(
                        "Request body missing required field '{}'",
                        required_field
                    )));
                }
            }
        }
        Ok(())
    }

    fn validate_headers(
        &self,
        param_schemas: &Vec<Value>,
        headers: Option<&HashMap<UniCase<String>, String>>,
    ) -> Result<(), OpenApiValidationError> {
        if let Err(e) = self.check_required_params(param_schemas, headers) {
            return Err(e);
        }

        for param in param_schemas {
            if param
                .get("in")
                .is_some_and(|param| param.as_str().is_some_and(|param| param == "header"))
            {
                let is_required = param
                    .get("required")
                    .and_then(|req| req.as_bool())
                    .unwrap_or(false);
                let name = param.get("name").and_then(|name| name.as_str()).unwrap();
                let schema = param.get("schema").unwrap();
                if let Some(header_value) =
                    headers.and_then(|headers| headers.get(&UniCase::<String>::from(name)))
                {
                    if let Err(e) = Self::simple_validation(schema, &json!(header_value)) {
                        return Err(e);
                    }
                } else if is_required {
                    return Err(OpenApiValidationError::InvalidHeaders(format!(
                        "Missing request header: '{}'",
                        name
                    )));
                }
            }
        }
        Ok(())
    }

    fn simple_validation(schema: &Value, instance: &Value) -> Result<(), OpenApiValidationError> {
        if let Err(e) = jsonschema::validate(schema, instance) {
            return Err(OpenApiValidationError::InvalidSchema(format!(
                "Validation failed: {}",
                e.to_string()
            )));
        }
        Ok(())
    }

    fn complex_validation(
        &self,
        json_path: &JsonPath,
        instance: &Value,
    ) -> Result<(), OpenApiValidationError> {
        let full_pointer_path = format!("@@root#/{}", json_path.format_path());
        let schema = json!({
            "$ref": full_pointer_path
        });
        let validator = match self.validator_options.build(&schema) {
            Ok(val) => val,
            Err(_) => {
                return Err(OpenApiValidationError::InvalidSchema(format!(
                    "Could not construct validator for json_path {}",
                    json_path.format_path()
                )));
            }
        };
        match validator.validate(instance) {
            Ok(_) => Ok(()),
            Err(e) => Err(OpenApiValidationError::InvalidRequest(format!(
                "Schema validation failed: {}",
                e.to_string()
            ))),
        }
    }

    fn check_required_params(
        &self,
        param_schemas: &Vec<Value>,
        request_params: Option<&HashMap<UniCase<String>, String>>,
    ) -> Result<(), OpenApiValidationError> {
        if let Some(headers) = request_params {
            for param in param_schemas {
                let param_name = param.get("name").and_then(|name| name.as_str()).unwrap();
                let section = param.get("in").and_then(|name| name.as_str()).unwrap();
                let param_required = param
                    .get("required")
                    .and_then(|required| required.as_bool())
                    .unwrap_or(false);
                if !headers.contains_key(&UniCase::<String>::from(param_name)) && param_required {
                    return Err(OpenApiValidationError::InvalidRequest(format!(
                        "Request {} param missing '{}' ",
                        section, param_name
                    )));
                }
            }
        }
        Ok(())
    }

    fn validate_query_params(
        &self,
        request_params: &Vec<Value>,
        query_params: Option<&HashMap<UniCase<String>, String>>,
    ) -> Result<(), OpenApiValidationError> {
        if let Err(e) = self.check_required_params(request_params, query_params) {
            return Err(e);
        }
        todo!()
    }

    fn extract_content_type(headers: Option<&HashMap<UniCase<String>, String>>) -> Option<&str> {
        if let Some(headers) =
            headers.and_then(|headers| headers.get(&UniCase::from("content-type")))
        {
            return Some(headers);
        }
        None
    }

    pub fn validate_request(
        &self,
        path: &str,
        method: &str,
        body: Option<&Value>,
        headers: Option<&HashMap<UniCase<String>, String>>,
        query_params: Option<&HashMap<UniCase<String>, String>>,
    ) -> Result<(), OpenApiValidationError> {
        let (operation, mut path) = match self.get_operation(path, method) {
            None => {
                return Err(OpenApiValidationError::InvalidPath(
                    "No path found".to_string(),
                ));
            }
            Some(val) => (val.0, val.1),
        };

        let request_schema = match Self::extract_content_type(headers) {
            None => None,
            Some(content_type) => {
                path.add_segment("requestBody".to_string());
                path.add_segment(content_type.to_string());
                path.add_segment("schema".to_string());
                self.traverser.get_request_body(operation, content_type)
            }
        };
        let request_params = self.traverser.get_request_parameters(operation);

        if let Err(e) = match (body.is_some(), request_schema) {
            (true, Some(request_body_schema)) => {
                self.validate_body(&path, request_body_schema, body)
            }
            (true, None) => Err(OpenApiValidationError::InvalidRequest(
                "Request body provided when endpoint has no request schema defined".to_string(),
            )),
            (_, _) => Ok(()),
        } {
            return Err(e);
        }

        if let Err(e) = match (headers.is_some(), request_params) {
            (true, Some(request_params)) => self.validate_headers(request_params, headers),
            (false, Some(request_params)) => self.check_required_params(request_params, None),
            (true, None) => Err(OpenApiValidationError::InvalidRequest(
                "Request parameters provided when endpoint has no parameters defined".to_string(),
            )),
            (_, _) => Ok(()),
        } {
            return Err(e);
        }

        if let Err(e) = match (query_params.is_some(), request_params) {
            (true, Some(request_params)) => {
                self.validate_query_params(request_params, query_params)
            }
            (false, Some(request_params)) => self.check_required_params(request_params, None),
            (true, None) => Err(OpenApiValidationError::InvalidRequest(
                "Query parameters provided when endpoint has no parameters defined".to_string(),
            )),
            (_, _) => Ok(()),
        } {
            return Err(e);
        }

        Ok(())
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

    pub fn get_request_body<'a>(
        &'a self,
        operation: &'a Value,
        content_type: &'a str,
    ) -> Option<&'a Value> {
        if let Some(schema) = Self::get(&self.specification, operation, "requestBody")
            .and_then(|node| Self::get(&self.specification, node, content_type))
            .and_then(|node| Self::get(&self.specification, node, "schema"))
        {
            return Some(schema);
        }
        None
    }

    pub fn get_request_parameters<'a>(&'a self, operation: &'a Value) -> Option<&'a Vec<Value>> {
        if let Some(parameters) =
            Self::get(&self.specification, operation, "parameters").and_then(|node| node.as_array())
        {
            return Some(parameters);
        }
        None
    }

    pub fn get_request_security<'a>(&'a self, operation: &'a Value) -> Option<&'a Vec<Value>> {
        if let Some(security) =
            Self::get(&self.specification, operation, "security").and_then(|node| node.as_array())
        {
            return Some(security);
        }
        None
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
    use std::collections::HashMap;
    use crate::openapi::OpenApiValidatorV2;
    use crate::openapi_util::JsonPath;
    use serde_json::{Value, json};
    use std::fs;
    use unicase::UniCase;

    #[test]
    fn test_find_operation() {
        let spec_string = fs::read_to_string("./test/openapi.json").unwrap();
        let specification: Value = serde_json::from_str(&spec_string).unwrap();
        let validator = OpenApiValidatorV2::new(specification).unwrap();
        let result: Option<(&Value, JsonPath)> =
            validator.get_operation("/pet/findByStatus/MultipleExamples", "get");
        assert!(result.is_some());

        let result = result.unwrap();
        assert_eq!(
            "paths/~1pet~1findByStatus~1MultipleExamples/get",
            result.1.format_path()
        )
    }

    #[test]
    fn test_validate_request() {
        let spec_string = fs::read_to_string("./test/openapi.json").unwrap();
        let specification: Value = serde_json::from_str(&spec_string).unwrap();
        let validator = OpenApiValidatorV2::new(specification).unwrap();
        let example_request_body = json!({
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

        let mut example_headers: HashMap<UniCase<String>, String> = HashMap::new();
        example_headers.insert(UniCase::from("Content-Type"), "application/json".to_string());
        let path = "/pet";
        let method = "post";
        let result = validator.validate_request(path, method, Some(&example_request_body), Some(&example_headers), None);

        assert!(result.is_ok());
    }
}
