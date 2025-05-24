mod openapi_v30x;
mod openapi_v31x;

use jsonschema::{Draft, Resource, ValidationError, ValidationOptions, Validator};
use serde_json::{Map, Value, json};
use std::collections::{HashMap, HashSet};
use std::fmt::{Display, Formatter};
use std::str::FromStr;
use unicase::UniCase;

pub enum OpenApiVersion {
    V30x,
    V31x,
}

impl FromStr for OpenApiVersion {
    type Err = OpenApiValidationError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.starts_with("3.1") {
            Ok(OpenApiVersion::V31x)
        } else if s.starts_with("3.0") {
            Ok(OpenApiVersion::V30x)
        } else {
            Err(OpenApiValidationError::InvalidSchema(format!(
                "Provided version '{}' does not match either 3.1.x or 3.0.x",
                s
            )))
        }
    }
}

impl OpenApiVersion {
    fn get_draft(&self) -> Draft {
        match self {
            OpenApiVersion::V30x => Draft::Draft4,
            OpenApiVersion::V31x => Draft::Draft202012,
        }
    }
}

fn serde_vec_2_string(vec: &Vec<Value>) -> String {
    let mut out = String::new();
    out.push_str("[");
    let mut index = 0;
    for el in vec {
        out.push_str(&el.to_string());
        if index + 1 < vec.len() {
            out.push_str(", ");
        }
        index += 1;
    }
    out.push_str("]");
    out
}

struct OpenApiValidator {
    traverser: OpenApiTraverser,
    validator_options: ValidationOptions,
}

impl OpenApiValidator {
    fn new(mut value: Value, validate_spec: bool) -> Result<Self, OpenApiValidationError> {
        // Assign ID for schema validation in the future.
        value["$id"] = json!("@@root");

        // Find the version defined in the spec and get the corresponding draft for validation.
        let draft = match Self::get_version_from_spec(&value) {
            Ok(version) => version.get_draft(),
            Err(e) => return Err(e),
        };

        // Validate the provided spec if the option is enabled.
        if validate_spec {
            match draft {
                Draft::Draft4 => {
                    let spec_schema: Value =
                        serde_json::from_str(openapi_v30x::OPENAPI_V30X).unwrap();
                    if let Err(e) = jsonschema::draft4::validate(&spec_schema, &value) {
                        return Err(OpenApiValidationError::InvalidSchema(format!(
                            "Provided 3.0.x openapi specification failed validation: {}",
                            e.to_string()
                        )));
                    }
                }
                Draft::Draft202012 => {
                    let spec_schema: Value =
                        serde_json::from_str(openapi_v31x::OPENAPI_V31X).unwrap();
                    if let Err(e) = jsonschema::draft202012::validate(&spec_schema, &value) {
                        return Err(OpenApiValidationError::InvalidSchema(format!(
                            "Provided 3.1.x openapi specification failed validation: {}",
                            e.to_string()
                        )));
                    }
                }
                _ => unreachable!(""),
            }
        }

        // Create this resource once and re-use it for multiple validation calls.
        let resource = match Resource::from_contents(value.clone()) {
            Ok(res) => res,
            Err(_) => {
                return Err(OpenApiValidationError::InvalidSchema(
                    "Invalid specification provided!".to_string(),
                ));
            }
        };

        // Assign draft and provide resource
        let options = Validator::options()
            .with_draft(draft)
            .with_resource("@@inner", resource);

        Ok(Self {
            traverser: OpenApiTraverser::new(value),
            validator_options: options,
        })
    }

    fn get_version_from_spec(
        specification: &Value,
    ) -> Result<OpenApiVersion, OpenApiValidationError> {
        // Find the openapi field and grab the version. It should follow either 3.1.x or 3.0.x.
        if let Some(version) = specification.get("openapi").and_then(|node| node.as_str()) {
            return match OpenApiVersion::from_str(version) {
                Ok(version) => Ok(version),
                Err(e) => Err(e),
            };
        }

        Err(OpenApiValidationError::InvalidSchema(
            "Provided spec does not contain 'openapi' field".to_string(),
        ))
    }

    fn get_operation(
        &self,
        request_path: &str,
        request_method: &str,
    ) -> Result<(&Value, JsonPath), OpenApiValidationError> {
        // Grab all paths from the spec
        if let Ok(spec_paths) = &self.traverser.get_paths() {
            // For each path there are 1 to n methods.
            for (spec_path, spec_path_methods) in spec_paths.iter() {
                let operations = match spec_path_methods.as_object() {
                    Some(x) => x,
                    None => {
                        return Err(OpenApiValidationError::InvalidSchema(format!(
                            "Specification path {} is not an object type",
                            spec_path
                        )));
                    }
                };

                // Grab the operation matching our request method and test to see if the path matches our request path.
                // If both method and path match, then we've found the operation associated with the request.
                if let Some(operation) = operations.get(request_method) {
                    let path_params = operation.get("parameters").and_then(|v| v.as_array());
                    if Self::matches_spec_path(path_params, request_path, spec_path) {
                        let mut json_path = JsonPath::new();
                        json_path
                            .add_segment("paths")
                            .add_segment(spec_path)
                            .add_segment(&request_method.to_lowercase());
                        return Ok((operation, json_path));
                    }
                }
            }
        }
        Err(OpenApiValidationError::InvalidPath(format!(
            "No path found in specification matching provided path '{}' and method '{}'",
            request_path, request_method
        )))
    }

    fn matches_spec_path(
        _path_params: Option<&Vec<Value>>,
        path_to_match: &str,
        spec_path: &str,
    ) -> bool {
        // Fast branch.
        // If the spec path we are checking contains no path parameters,
        // then we can simply compare path strings.
        if !(spec_path.contains("{") && spec_path.contains("}")) {
            spec_path == path_to_match

        // if the request path contains path parameters, we need to compare each segment
        // When we reach a segment that is a parameter, compare the value in the path to the value in the spec.
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

                        // TODO - compare the value in the request with the schema in the spec.
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
                    serde_vec_2_string(required_fields)
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

        Ok(())
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
            Err(e) => return Err(e),
            Ok(val) => (val.0, val.1),
        };

        let request_schema = match Self::extract_content_type(headers) {
            None => None,
            Some(content_type) => {
                path.add_segment("requestBody")
                    .add_segment("content")
                    .add_segment(content_type)
                    .add_segment("schema");
                match self.traverser.get_request_body(operation, content_type) {
                    Ok(val) => Some(val),
                    Err(_) => None
                }
            }
        };

        let spec_parameters = match self.traverser.get_request_parameters(operation) {
            Ok(val) => Some(val),
            Err(_) => None
        };

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

        if let Err(e) = match (headers.is_some(), spec_parameters) {
            // if we have header params in our request and the spec contains params, do the validation.
            (true, Some(request_params)) => self.validate_headers(request_params, headers),

            // If no header params were provided and the spec contains params,
            // check to see if there are any required header params.
            (false, Some(request_params)) => self.check_required_params(request_params, None),

            // passthrough
            (_, _) => Ok(()),
        } {
            return Err(e);
        }

        if let Err(e) = match (query_params.is_some(), spec_parameters) {
            // If we have query params in our request and the spec contains params, do the validation.
            (true, Some(request_params)) => {
                self.validate_query_params(request_params, query_params)
            }

            // If no query params were provided and the spec contains params,
            // check to see if there are any required query params.
            (false, Some(request_params)) => self.check_required_params(request_params, None),

            // If query params were provided, but the spec contains no param definitions, raise a validation error.
            (true, None) => Err(OpenApiValidationError::InvalidRequest(
                "Query parameters provided when endpoint has no parameters defined".to_string(),
            )),

            // passthrough
            (_, _) => Ok(()),
        } {
            return Err(e);
        }

        Ok(())
    }
}

struct OpenApiTraverser {
    // map of previously resolved references
    // TODO - make use of this
    _resolved_references: HashMap<String, Value>,
    // spec to traverse over
    specification: Value,
}

impl OpenApiTraverser {
    fn new(specification: Value) -> Self {
        Self {
            _resolved_references: HashMap::new(),
            specification,
        }
    }

    fn get_request_body<'a>(
        &'a self,
        operation: &'a Value,
        content_type: &'a str,
    ) -> Result<&'a Value, OpenApiValidationError> {
        Self::get(&self.specification, operation, "requestBody")
            .and_then(|node| Self::get(&self.specification, node, "content"))
            .and_then(|node| Self::get(&self.specification, node, content_type))
            .and_then(|node| Self::get(&self.specification, node, "schema"))
    }

    fn get_paths<'a>(&self) -> Result<&Map<String, Value>, OpenApiValidationError> {
        Self::get_as_map(&self.specification, &self.specification, "paths")
    }

    fn get_request_parameters<'a>(
        &'a self,
        operation: &'a Value,
    ) -> Result<&'a Vec<Value>, OpenApiValidationError> {
        Self::get_as_array(&self.specification, operation, "parameters")
    }

    fn get_request_security<'a>(
        &'a self,
        operation: &'a Value,
    ) -> Result<&'a Vec<Value>, OpenApiValidationError> {
        Self::get_as_array(&self.specification, operation, "security")
    }

    fn get_as_map<'a>(
        specification: &'a Value,
        value: &'a Value,
        field: &'a str,
    ) -> Result<&'a Map<String, Value>, OpenApiValidationError> {
        let node = match Self::get(specification, value, field) {
            Ok(node) => node,
            Err(e) => return Err(e),
        };
        match node.as_object() {
            None => Err(OpenApiValidationError::InvalidType(format!(
                "Value '{field}' is not a map type"
            ))),
            Some(map) => Ok(map),
        }
    }

    fn get_as_array<'a>(
        specification: &'a Value,
        value: &'a Value,
        field: &'a str,
    ) -> Result<&'a Vec<Value>, OpenApiValidationError> {
        let node = match Self::get(specification, value, field) {
            Ok(node) => node,
            Err(e) => return Err(e),
        };
        match node.as_array() {
            None => Err(OpenApiValidationError::InvalidType(format!(
                "Value '{node}' is not an array type"
            ))),
            Some(array) => Ok(array),
        }
    }

    fn get<'a>(
        specification: &'a Value,
        value: &'a Value,
        field: &'a str,
    ) -> Result<&'a Value, OpenApiValidationError> {
        Self::check_for_ref(specification, value).and_then(|val| Self::get_node(val, field))
    }

    fn get_node<'a>(value: &'a Value, field: &'a str) -> Result<&'a Value, OpenApiValidationError> {
        match value.get(field) {
            None => Err(OpenApiValidationError::RequiredFieldMissing(format!(
                "Node {value} is missing field '{field}'"
            ))),
            Some(val) => Ok(val),
        }
    }

    fn get_reference_path<'a>(
        specification: &'a Value,
        value: &'a Value,
        ref_string: &'a str,
        seen_references: &mut HashSet<String>,
    ) -> Result<&'a Value, OpenApiValidationError> {
        let path = ref_string.split("/").collect::<Vec<&str>>();
        let mut current_value = value;
        let mut seen_references = seen_references;
        for index in 0..path.len() {
            if let Some(ref_map_value) = current_value.get("$ref").and_then(|val| val.as_str()) {
                if seen_references.contains(ref_map_value) {
                    return Err(OpenApiValidationError::InvalidSchema(format!(
                        "Circular reference found when resolving reference string '{}'",
                        ref_string
                    )));
                }
                match Self::get_reference_path(
                    specification,
                    current_value,
                    ref_map_value,
                    &mut seen_references,
                ) {
                    Ok(resolved) => {
                        seen_references.insert(ref_map_value.to_string());
                        current_value = resolved
                    }
                    Err(e) => return Err(e),
                }
            }

            let segment = match path.get(index) {
                Some(x) => x,
                None => {
                    return Err(OpenApiValidationError::InvalidPath(
                        "Provided path is malformed".to_string(),
                    ));
                }
            };

            if *segment == "#" {
                continue;
            }

            if let Some(next) = current_value.get(segment) {
                current_value = next;
            } else {
                return Err(OpenApiValidationError::RequiredFieldMissing(format!(
                    "Node {current_value} is missing field '{segment}'"
                )));
            }
        }
        Ok(current_value)
    }

    fn check_for_ref<'a>(
        specification: &'a Value,
        node: &'a Value,
    ) -> Result<&'a Value, OpenApiValidationError> {
        if let Some(ref_string) = node.get("$ref").and_then(|val| val.as_str()) {
            let mut seen_references = HashSet::new();
            return Self::get_reference_path(
                &specification,
                &node,
                ref_string,
                &mut seen_references,
            );
        }
        Ok(node)
    }
}

#[derive(Debug)]
pub enum OpenApiValidationError {
    InvalidSchema(String),
    InvalidRequest(String),
    InvalidResponse(String),
    InvalidPath(String),
    InvalidMethod(String),
    InvalidContentType(String),
    InvalidAccept(String),
    InvalidQueryParameters(String),
    InvalidHeaders(String),
    RequiredFieldMissing(String),
    InvalidRef(String),
    InvalidType(String),
}

impl Display for OpenApiValidationError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            OpenApiValidationError::InvalidSchema(msg) => write!(f, "InvalidSchema: {}", msg),
            OpenApiValidationError::InvalidRequest(msg) => write!(f, "InvalidRequest: {}", msg),
            OpenApiValidationError::InvalidResponse(msg) => write!(f, "InvalidResponse: {}", msg),
            OpenApiValidationError::InvalidPath(msg) => write!(f, "InvalidPath: {}", msg),
            OpenApiValidationError::InvalidMethod(msg) => write!(f, "InvalidMethod: {}", msg),
            OpenApiValidationError::InvalidContentType(msg) => {
                write!(f, "InvalidContentType: {}", msg)
            }
            OpenApiValidationError::InvalidAccept(msg) => write!(f, "InvalidAccept: {}", msg),
            OpenApiValidationError::InvalidQueryParameters(msg) => {
                write!(f, "InvalidQueryParameters: {}", msg)
            }
            OpenApiValidationError::InvalidHeaders(msg) => write!(f, "InvalidHeaders: {}", msg),
            OpenApiValidationError::RequiredFieldMissing(msg) => {
                write!(f, "RequiredFieldMissing: {}", msg)
            }
            OpenApiValidationError::InvalidRef(msg) => write!(f, "InvalidRef: {}", msg),
            OpenApiValidationError::InvalidType(msg) => write!(f, "InvalidType: {}", msg),
        }
    }
}

impl std::error::Error for OpenApiValidationError {}

#[derive(Debug, Clone)]
struct JsonPath(pub Vec<String>);

impl JsonPath {
    fn new() -> Self {
        JsonPath(Vec::new())
    }

    fn add_segment(&mut self, segment: &str) -> &mut Self {
        if segment.contains("/") {
            let segment = segment.replace("/", "~1");
            self.0.push(segment);
        } else {
            self.0.push(segment.to_string());
        }

        self
    }

    fn format_path(&self) -> String {
        self.0.join("/")
    }
}

#[cfg(test)]
mod test {
    use crate::{JsonPath, OpenApiTraverser, OpenApiValidator};
    use serde_json::{Value, json};
    use std::collections::HashMap;
    use std::fs;
    use unicase::UniCase;

    #[test]
    fn test_find_operation() {
        let spec_string = fs::read_to_string("./test/openapi-v3.0.2.json").unwrap();
        let specification: Value = serde_json::from_str(&spec_string).unwrap();
        let validator = OpenApiValidator::new(specification, false).unwrap();

        let result = validator.get_operation("/pet/findByStatus/MultipleExamples", "get");
        assert!(result.is_ok());
        let result = result.unwrap();
        assert_eq!(
            "paths/~1pet~1findByStatus~1MultipleExamples/get",
            result.1.format_path()
        );

        let result = validator.get_operation("/pet/findById/123", "get");
        assert!(result.is_ok());
        let result = result.unwrap();
        assert_eq!(
            "paths/~1pet~1findById~1{pet_id}/get",
            result.1.format_path()
        )
    }

    #[test]
    fn test_find_request_body() {
        let spec_string = fs::read_to_string("./test/openapi-v3.0.2.json").unwrap();
        let specification: Value = serde_json::from_str(&spec_string).unwrap();
        let traverser = OpenApiTraverser::new(specification.clone());
        let validator = OpenApiValidator::new(specification, false).unwrap();
        let result: (&Value, JsonPath) = validator.get_operation("/pet", "post").unwrap();

        let operation = result.0;
        assert!(operation.get("requestBody").is_some());
        let request_body = traverser.get_request_body(result.0, "application/json");
        assert!(request_body.is_ok());
    }

    #[test]
    fn test_validate_request() {
        let spec_string = fs::read_to_string("./test/openapi-v3.0.2.json").unwrap();
        let specification: Value = serde_json::from_str(&spec_string).unwrap();
        let validator = OpenApiValidator::new(specification, false).unwrap();
        let example_request_body = json!({
            "name": "Ruby",
            "age": 5,
            "hunts": true,
            "breed": "Bengal"
        });

        let mut example_headers: HashMap<UniCase<String>, String> = HashMap::new();
        example_headers.insert(
            UniCase::from("Content-Type"),
            "application/json".to_string(),
        );
        let path = "/pet";
        let method = "post";
        let result = validator.validate_request(
            path,
            method,
            Some(&example_request_body),
            Some(&example_headers),
            None,
        );

        match result {
            Ok(_) => assert!(true, "Body is valid"),
            Err(e) => {
                println!("{e:#?}");
                assert!(false, "Body should be valid")
            }
        }

        let example_request_body = json!({
            "name": "Ruby",
            "age": 5,
            "hunts": true,
            "invalid_field": "some incorrect data"
        });
        let result = validator.validate_request(
            path,
            method,
            Some(&example_request_body),
            Some(&example_headers),
            None,
        );
        match result {
            Ok(_) => assert!(false, "Body should not be valid"),
            Err(e) => {
                println!("{e:#?}");
                assert!(true, "Body is invalid")
            }
        }
    }
}
