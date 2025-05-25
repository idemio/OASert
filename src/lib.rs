mod openapi_v30x;
mod openapi_v31x;
mod openapi_common;

use dashmap::{DashMap, Entry};
use jsonschema::{Draft, Resource, ValidationError, ValidationOptions, Validator};
use serde_json::{Map, Value, json};
use std::collections::{HashMap, HashSet};
use std::fmt::{Display, Formatter};
use std::str::FromStr;
use std::sync::Arc;
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
    fn new(mut value: Value, _validate_spec: bool) -> Result<Self, OpenApiValidationError> {
        // Assign ID for schema validation in the future.
        value["$id"] = json!("@@root");

        // Find the version defined in the spec and get the corresponding draft for validation.
        let draft = match Self::get_version_from_spec(&value) {
            Ok(version) => version.get_draft(),
            Err(e) => return Err(e),
        };

        // Validate the provided spec if the option is enabled.
//        if validate_spec {
//            match draft {
//                Draft::Draft4 => {
//                    let spec_schema: Value =
//                        serde_json::from_str(openapi_v30x::OPENAPI_V30X).unwrap();
//                    if let Err(e) = jsonschema::draft4::validate(&spec_schema, &value) {
//                        return Err(OpenApiValidationError::InvalidSchema(format!(
//                            "Provided 3.0.x openapi specification failed validation: {}",
//                            e.to_string()
//                        )));
//                    }
//                }
//                Draft::Draft202012 => {
//                    let spec_schema: Value =
//                        serde_json::from_str(openapi_v31x::OPENAPI_V31X).unwrap();
//                    if let Err(e) = jsonschema::draft202012::validate(&spec_schema, &value) {
//                        return Err(OpenApiValidationError::InvalidSchema(format!(
//                            "Provided 3.1.x openapi specification failed validation: {}",
//                            e.to_string()
//                        )));
//                    }
//                }
//                _ => unreachable!(""),
//            }
//        }

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
        param_schemas: &Value,
        headers: Option<&HashMap<UniCase<String>, String>>,
    ) -> Result<(), OpenApiValidationError> {
        if let Err(e) = self.check_required_params(param_schemas, headers) {
            return Err(e);
        }

        if let Some(param_schemas) = param_schemas.as_array() {
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
        param_schemas: &Value,
        request_params: Option<&HashMap<UniCase<String>, String>>,
    ) -> Result<(), OpenApiValidationError> {
        if let (Some(headers), Some(param_schemas)) = (request_params, param_schemas.as_array()) {
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
        request_params: &Value,
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
        let (operation, mut path) = match self.traverser.get_operation(path, method) {
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
                match self.traverser.get_request_body(&operation, content_type) {
                    Ok(val) => Some(val),
                    Err(_) => None,
                }
            }
        };

        let spec_parameters = match self.traverser.get_request_parameters(&operation) {
            Ok(val) => Some(val),
            Err(_) => None,
        };

        if let Err(e) = match (body.is_some(), request_schema) {
            (true, Some(request_body_schema)) => {
                self.validate_body(&path, &request_body_schema, body)
            }

            (true, None) => Err(OpenApiValidationError::InvalidRequest(
                "Request body provided when endpoint has no request schema defined".to_string(),
            )),

            (_, _) => Ok(()),
        } {
            return Err(e);
        }

        if let Err(e) = match (headers.is_some(), spec_parameters.clone()) {
            // if we have header params in our request and the spec contains params, do the validation.
            (true, Some(request_params)) => self.validate_headers(&request_params, headers),

            // If no header params were provided and the spec contains params,
            // check to see if there are any required header params.
            (false, Some(request_params)) => self.check_required_params(&request_params, None),

            // passthrough
            (_, _) => Ok(()),
        } {
            return Err(e);
        }

        if let Err(e) = match (query_params.is_some(), spec_parameters) {
            // If we have query params in our request and the spec contains params, do the validation.
            (true, Some(request_params)) => {
                self.validate_query_params(&request_params, query_params)
            }

            // If no query params were provided and the spec contains params,
            // check to see if there are any required query params.
            (false, Some(request_params)) => self.check_required_params(&request_params, None),

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

type TraverseResult = Result<Arc<Value>, OpenApiValidationError>;

struct OpenApiTraverser {
    specification: Value,
    resolved_references: DashMap<String, Arc<Value>>,
}

impl OpenApiTraverser {
    const CONTENT_FIELD: &'static str = "content";
    const SCHEMA_FIELD: &'static str = "schema";
    const REQUEST_BODY_FIELD: &'static str = "requestBody";
    const PATHS_FIELD: &'static str = "paths";
    const PARAMETERS_FIELD: &'static str = "parameters";
    const REF_FIELD: &'static str = "$ref";
    const SECURITY_FIELD: &'static str = "security";
    const PATH_SEPARATOR: &'static str = "/";
    const ENCODED_BACKSLASH: &'static str = "~1";

    fn new(specification: Value) -> Self {
        Self {
            specification,
            resolved_references: DashMap::new(),
        }
    }

    /// Finds and returns the matching operation from the OpenAPI specification based on the request path and method.
    ///
    /// # Arguments
    /// * `request_path` - The path of the incoming request (e.g., "/users/123")
    /// * `request_method` - The HTTP method of the request (e.g., "GET", "POST")
    ///
    /// # Returns
    /// * `Ok((Arc<Value>, JsonPath))` - A tuple containing:
    ///   - An Arc-wrapped JSON Value representing the operation object from the OpenAPI spec
    ///   - A JsonPath object representing the path to the operation in the spec
    /// * `Err(OpenApiValidationError)` - Returns an error if no matching operation is found or if the
    ///   specification is invalid
    fn get_operation(
        &self,
        request_path: &str,
        request_method: &str,
    ) -> Result<(Arc<Value>, JsonPath), OpenApiValidationError> {
        // Grab all paths from the spec
        if let Ok(spec_paths) = self.get_paths() {
            // For each path there are 1 to n methods.
            if let Some(spec_paths) = spec_paths.as_object() {
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
                        let path_params = self.get_request_parameters(operation)?;
                        let path_params = path_params.as_array();
                        if Self::matches_spec_path(path_params, request_path, spec_path) {
                            let mut json_path = JsonPath::new();
                            json_path
                                .add_segment("paths")
                                .add_segment(spec_path)
                                .add_segment(&request_method.to_lowercase());
                            return Ok((Arc::new(operation.clone()), json_path));
                        }
                    }
                }
            }
        }
        Err(OpenApiValidationError::InvalidPath(format!(
            "No path found in specification matching provided path '{}' and method '{}'",
            request_path, request_method
        )))
    }

    /// Determines if a given path matches an OpenAPI specification path pattern.
    ///
    /// This function checks if a request path matches a specification path, handling path parameters
    /// enclosed in curly braces (e.g., "/users/{id}").
    ///
    /// # Arguments
    ///
    /// * `_path_params` - An optional reference to a vector of values representing path parameters.
    ///   Currently unused in the implementation.
    /// * `path_to_match` - The actual request path to check against the specification.
    /// * `spec_path` - The specification path pattern that may contain path parameters in the format "{param_name}".
    ///
    /// # Returns
    ///
    /// * `true` if the path matches the specification pattern, accounting for path parameters.
    /// * `false` if the path does not match the pattern or has a different number of segments.
    fn matches_spec_path(
        _path_params: Option<&Vec<Value>>,
        path_to_match: &str,
        spec_path: &str,
    ) -> bool {
        // If the spec path we are checking contains no path parameters,
        // then we can simply compare path strings.
        if !(spec_path.contains("{") && spec_path.contains("}")) {
            spec_path == path_to_match

        // if the request path contains path parameters, we need to compare each segment
        // When we reach a segment that is a parameter, compare the value in the path to the value in the spec.
        } else {
            let target_segments = path_to_match.split(Self::PATH_SEPARATOR).collect::<Vec<&str>>();
            let spec_segments = spec_path.split(Self::PATH_SEPARATOR).collect::<Vec<&str>>();

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

    /// Retrieves the schema of a request body for a specific content type from an OpenAPI operation object.
    ///
    /// # Arguments
    ///
    /// * `operation` - A JSON value representing an OpenAPI operation object
    /// * `content_type` - A string specifying the media type (e.g., "application/json") to look up
    ///
    /// # Returns
    ///
    /// * `Ok(Arc<Value>)` - An `Arc` pointer to the schema of the request body for the specified content type
    /// * `Err(OpenApiValidationError)` - An error if any part of the path (requestBody, content, content_type, schema) is missing
    fn get_request_body(&self, operation: &Value, content_type: &str) -> TraverseResult {
        self.get(operation, Self::REQUEST_BODY_FIELD)
            .and_then(|node| self.get(&node, Self::CONTENT_FIELD))
            .and_then(|node| self.get(&node, content_type))
            .and_then(|node| self.get(&node, Self::SCHEMA_FIELD))
    }

    /// Retrieves the "paths" object from the OpenAPI specification.
    ///
    /// # Returns
    ///
    /// * `TraverseResult` - A Result containing either:
    ///   * `Ok(Arc<Value>)` - An Arc pointer to the paths object if found
    ///   * `Err(OpenApiValidationError)` - An error if the paths field is missing or reference resolution fails
    fn get_paths(&self) -> TraverseResult {
        self.get(&self.specification, Self::PATHS_FIELD)
    }

    /// Retrieves the "parameters" field from the given operation object.
    ///
    /// # Arguments
    ///
    /// * `operation` - A reference to a JSON `Value` representing an OpenAPI operation object
    ///
    /// # Returns
    ///
    /// * `Ok(Arc<Value>)` - An `Arc` pointer to the parameters JSON value if successful
    /// * `Err(OpenApiValidationError)` - An error if the parameters field is missing or reference resolution fails
    fn get_request_parameters<'a>(&'a self, operation: &'a Value) -> TraverseResult {
        self.get(operation, Self::PARAMETERS_FIELD)
    }

    /// Retrieves the security field from an operation object in an OpenAPI specification.
    ///
    /// # Arguments
    ///
    /// * `operation` - A reference to a JSON `Value` representing an operation object in an OpenAPI specification.
    ///
    /// # Returns
    ///
    /// * `Ok(Arc<Value>)` - An `Arc` pointer to the security field's value if it exists in the operation object.
    /// * `Err(OpenApiValidationError)` - An error if the security field is missing or reference resolution fails.
    fn get_request_security(&self, operation: &Value) -> TraverseResult {
        self.get(operation, Self::SECURITY_FIELD)
    }

    /// Retrieves a field from a JSON value, handling potential reference resolution.
    ///
    /// # Arguments
    ///
    /// * `value` - A reference to a JSON `Value` to get the field from. If this value contains a reference
    ///   (i.e., a "$ref" field), the reference will be resolved first.
    /// * `field` - The name of the field to retrieve from the value.
    ///
    /// # Returns
    ///
    /// * `Ok(Arc<Value>)` - An `Arc` pointer to the retrieved JSON value if successful
    /// * `Err(OpenApiValidationError)` - An error if the field is missing or reference resolution fails
    fn get(&self, value: &Value, field: &str) -> TraverseResult {
        self.check_for_ref(&value)
            .and_then(|val| Self::get_node(&val, field))
    }

    /// Retrieves a specific field from a JSON value.
    ///
    /// # Arguments
    ///
    /// * `value` - The JSON value to extract a field from
    /// * `field` - The name of the field to extract
    ///
    /// # Returns
    ///
    /// * `Ok(Arc<Value>)` - An Arc-wrapped clone of the requested field's value if found
    /// * `Err(OpenApiValidationError::FieldNotFound)` - If the specified field doesn't exist in the value
    fn get_node(value: &Value, field: &str) -> TraverseResult {
        match value.get(field) {
            None => Err(OpenApiValidationError::FieldNotFound(
                String::from(field),
                value.clone(),
            )),
            Some(val) => Ok(Arc::new(val.clone())),
        }
    }

    /// Checks if a JSON node contains a "$ref" field and resolves it if present.
    ///
    /// # Arguments
    ///
    /// * `node` - A reference to a JSON Value node that might contain a "$ref" field
    ///
    /// # Returns
    ///
    /// * `TraverseResult` - Either:
    ///   - `Ok(Arc<Value>)` with the resolved reference (if the node contains a "$ref")
    ///     or the original node wrapped in an Arc (if no "$ref" is present)
    ///   - `Err(OpenApiValidationError)` if reference resolution fails
    fn check_for_ref<'a>(&'a self, node: &'a Value) -> TraverseResult {
        // If the ref node exists, resolve it
        if let Some(ref_string) = node.get(Self::REF_FIELD).and_then(|val| val.as_str()) {
            let entry = self.resolved_references.entry(String::from(ref_string));
            return match entry {
                Entry::Occupied(e) => Ok(e.get().clone()),
                Entry::Vacant(_) => {
                    let mut seen_references = HashSet::new();
                    self.get_reference_path(ref_string, &mut seen_references)
                }
            };
        }
        Ok(Arc::new(node.clone()))
    }

    /// Resolves a JSON reference path within an OpenAPI specification.
    ///
    /// # Arguments
    ///
    /// * `ref_string` - A string representing the JSON reference path to resolve (e.g., "/components/schemas/Pet")
    /// * `seen_references` - A mutable HashSet that tracks references already processed to detect circular references
    ///
    /// # Returns
    ///
    /// * `Ok(Arc<Value>)` - An Arc-wrapped JSON Value representing the resolved reference
    /// * `Err(OpenApiValidationError)` - An error if reference resolution fails due to:
    ///   - Circular references
    ///   - Missing fields in the path
    fn get_reference_path(
        &self,
        ref_string: &str,
        seen_references: &mut HashSet<String>,
    ) -> TraverseResult {
        if seen_references.contains(ref_string) {
            return Err(OpenApiValidationError::InvalidSchema(format!(
                "Circular reference found when resolving reference string '{}'",
                ref_string
            )));
        }
        seen_references.insert(String::from(ref_string));
        let path = ref_string
            .split(Self::PATH_SEPARATOR)
            .filter(|node| !(*node).is_empty() && (*node != "#"))
            .collect::<Vec<&str>>();
        let mut current_schema = &self.specification;
        for segment in path {
            let refactored_segment = segment.replace(Self::ENCODED_BACKSLASH, Self::PATH_SEPARATOR);
            // Navigate to the next segment
            match current_schema.get(refactored_segment) {
                Some(next) => {
                    current_schema = next;
                }
                None => {
                    return Err(OpenApiValidationError::FieldNotFound(
                        String::from(segment),
                        current_schema.clone(),
                    ));
                }
            }
        }
        let current_schema = self.check_for_ref(current_schema)?;
        self.resolved_references
            .insert(String::from(ref_string), current_schema.clone());

        Ok(current_schema)
    }

// todo - make sure i dont need this before i delete
    //    fn get_reference_path(
    //        &self,
    //        ref_string: &str,
    //        seen_references: &mut HashSet<String>,
    //    ) -> TraverseResult {
    //        // Check for circular references
    //        if seen_references.contains(ref_string) {
    //            return Err(OpenApiValidationError::InvalidSchema(format!(
    //                "Circular reference found when resolving reference string '{}'",
    //                ref_string
    //            )));
    //        }
    //
    //        seen_references.insert(String::from(ref_string));
    //
    //        // Check if the reference is already in cache
    //        let entry = self.resolved_references.entry(String::from(ref_string));
    //        match entry {
    //            Entry::Occupied(e) => Ok(e.get().clone()),
    //            Entry::Vacant(_) => {
    //                let path = ref_string.split("/");
    //
    //                // Start with a reference to the specification and follow the path
    //                let mut current_schema = &self.specification;
    //                // This is to hold onto the results of any sub calls.
    //                let mut resolved_sub_reference: Arc<Value> = Arc::default();
    //
    //                for segment in path {
    //                    if segment == "#" {
    //                        continue;
    //                    }
    //
    //                    // Check if current node is itself a reference
    //                    if let Some(nested_ref) =
    //                        current_schema.get("$ref").and_then(|val| val.as_str())
    //                    {
    //                        if seen_references.contains(nested_ref) {
    //                            return Err(OpenApiValidationError::InvalidSchema(format!(
    //                                "Circular reference found when resolving nested reference '{}'",
    //                                nested_ref
    //                            )));
    //                        }
    //
    //                        // Resolve the nested reference
    //                        resolved_sub_reference =
    //                            self.get_reference_path(nested_ref, seen_references)?;
    //                        current_schema = &resolved_sub_reference;
    //                    }
    //
    //                    // Navigate to the next segment
    //                    match current_schema.get(segment) {
    //                        Some(next) => {
    //                            current_schema = next;
    //                        }
    //                        None => {
    //                            return Err(OpenApiValidationError::RequiredFieldMissing(format!(
    //                                "Node missing field '{}'",
    //                                segment
    //                            )));
    //                        }
    //                    }
    //                }
    //
    //                // Check if the final node is itself a reference
    //                if let Some(final_ref) = current_schema.get("$ref").and_then(|val| val.as_str()) {
    //                    let result = self.get_reference_path(final_ref, seen_references)?;
    //                    // Cache the result
    //                    self.resolved_references
    //                        .insert(String::from(ref_string), result.clone());
    //                    return Ok(result);
    //                }
    //
    //                let arc_result = Arc::new(current_schema.clone());
    //                // Cache the resolved reference
    //                self.resolved_references
    //                    .insert(String::from(ref_string), arc_result.clone());
    //
    //                Ok(arc_result)
    //            }
    //        }
    //    }
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
    FieldNotFound(String, Value),
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
            OpenApiValidationError::FieldNotFound(field, node) => write!(
                f,
                "FieldNotFound: Object {} is missing field '{}'",
                node, field
            ),
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
            self.0.push(segment.to_owned());
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
    use memory_stats::memory_stats;
    use serde_json::{Value, json};
    use std::collections::HashMap;
    use std::fs;
    use std::sync::Arc;
    use unicase::UniCase;

    fn print_memory() {
        if let Some(usage) = memory_stats() {
            println!("Current physical memory usage: {}", usage.physical_mem);
            println!("Current virtual memory usage: {}", usage.virtual_mem);
        } else {
            println!("Couldn't get the current memory usage :(");
        }
    }

    #[test]
    fn test_find_operation() {
        let spec_string = fs::read_to_string("./test/openapi-v3.0.2.json").unwrap();
        let specification: Value = serde_json::from_str(&spec_string).unwrap();
        let validator = OpenApiValidator::new(specification, false).unwrap();
        print_memory();
        {
            let result = validator
                .traverser
                .get_operation("/pet/findByStatus/MultipleExamples", "get");
            assert!(result.is_ok());
            let result = result.unwrap();
            assert_eq!(
                "paths/~1pet~1findByStatus~1MultipleExamples/get",
                result.1.format_path()
            );
        }
        print_memory();
        {
            let result = validator
                .traverser
                .get_operation("/pet/findById/123", "get");
            assert!(result.is_ok());
            let result = result.unwrap();
            assert_eq!(
                "paths/~1pet~1findById~1{pet_id}/get",
                result.1.format_path()
            );
        }
        print_memory();
    }

    #[test]
    fn test_find_request_body() {
        let spec_string = fs::read_to_string("./test/openapi-v3.0.2.json").unwrap();
        let specification: Value = serde_json::from_str(&spec_string).unwrap();
        let traverser = OpenApiTraverser::new(specification.clone());
        let validator = OpenApiValidator::new(specification, false).unwrap();
        let result: (Arc<Value>, JsonPath) =
            validator.traverser.get_operation("/pet", "post").unwrap();
        let operation = result.0.clone();
        assert!(operation.get("requestBody").is_some());
        let request_body = traverser.get_request_body(&result.0, "application/json");
        assert!(request_body.is_ok());
    }

    #[test]
    fn test_validate_wild_request() {
        let spec_string = fs::read_to_string("./test/wild-openapi-spec.json").unwrap();
        let specification: Value = serde_json::from_str(&spec_string).unwrap();
        let validator = OpenApiValidator::new(specification, false).unwrap();
        let example_request = json!({
          "layerTwo": {
            "layerThree": {
              "data": {
                "customField1": "value1",
                "customField2": 42
              }
            }
          }
        });
        let mut example_headers: HashMap<UniCase<String>, String> = HashMap::new();
        example_headers.insert(
            UniCase::from("Content-Type"),
            "application/json".to_string(),
        );
        let path = "/pet";
        let method = "post";

        for x in 0..25 {
            println!("it: {x}");
            print_memory();

            let result = validator.validate_request(
                path,
                method,
                Some(&example_request),
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
        }
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
