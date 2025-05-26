use dashmap::{DashMap, Entry};
use jsonschema::{Draft, Resource, ValidationOptions, Validator};
use serde_json::{Value, json};
use std::cmp::PartialEq;
use std::collections::{HashMap, HashSet};
use std::fmt::{Display, Formatter};
use std::str::FromStr;
use std::sync::Arc;
use unicase::UniCase;

const CONTENT_FIELD: &'static str = "content";
const SCHEMA_FIELD: &'static str = "schema";
const REQUEST_BODY_FIELD: &'static str = "requestBody";
const PATHS_FIELD: &'static str = "paths";
const PARAMETERS_FIELD: &'static str = "parameters";
const REF_FIELD: &'static str = "$ref";
const SECURITY_FIELD: &'static str = "security";
const PATH_SEPARATOR: &'static str = "/";
const ENCODED_BACKSLASH: &'static str = "~1";
const NAME_FIELD: &'static str = "name";
const OPENAPI_FIELD: &'static str = "openapi";
const REQUIRED_FIELD: &'static str = "openapi";

pub enum OpenApiVersion {
    V30x,
    V31x,
}

impl FromStr for OpenApiVersion {
    type Err = PayloadValidationError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.starts_with("3.1") {
            Ok(OpenApiVersion::V31x)
        } else if s.starts_with("3.0") {
            Ok(OpenApiVersion::V30x)
        } else {
            Err(PayloadValidationError::InvalidSchema(format!(
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

struct OpenApiPayloadValidator {
    traverser: OpenApiTraverser,
    validator_options: ValidationOptions,
}

impl OpenApiPayloadValidator {
    fn new(mut value: Value) -> Result<Self, PayloadValidationError> {
        // Assign ID for schema validation in the future.
        value["$id"] = json!("@@root");

        // Find the version defined in the spec and get the corresponding draft for validation.
        let draft = match Self::get_version_from_spec(&value) {
            Ok(version) => version.get_draft(),
            Err(e) => return Err(e),
        };

        // Create this resource once and re-use it for multiple validation calls.
        let resource = match Resource::from_contents(value.clone()) {
            Ok(res) => res,
            Err(_) => {
                return Err(PayloadValidationError::InvalidSchema(
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
    ) -> Result<OpenApiVersion, PayloadValidationError> {
        // Find the openapi field and grab the version. It should follow either 3.1.x or 3.0.x.
        if let Some(version) = specification
            .get(OPENAPI_FIELD)
            .and_then(|node| node.as_str())
        {
            return match OpenApiVersion::from_str(version) {
                Ok(version) => Ok(version),
                Err(e) => Err(e),
            };
        }

        Err(PayloadValidationError::InvalidSchema(
            "Provided spec does not contain 'openapi' field".to_string(),
        ))
    }

    fn validate_body(
        &self,
        request_body_path: &JsonPath,
        request_schema: &Value,
        request_body: Option<&Value>,
    ) -> Result<(), PayloadValidationError> {
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
    ) -> Result<(), PayloadValidationError> {
        if let Some(required_fields) = body_schema
            .get(REQUIRED_FIELD)
            .and_then(|required| required.as_array())
        {
            if !required_fields.is_empty() && request_body.is_none() {
                return Err(PayloadValidationError::InvalidRequest(format!(
                    "Request schema contains required fields {} but provided request is empty.",
                    serde_vec_2_string(required_fields)
                )));
            }

            for required in required_fields {
                let required_field = required.as_str().unwrap();

                if request_body.is_some_and(|body| body.get(required_field).is_none()) {
                    return Err(PayloadValidationError::InvalidRequest(format!(
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
    ) -> Result<(), PayloadValidationError> {
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
                        .get(REQUIRED_FIELD)
                        .and_then(|req| req.as_bool())
                        .unwrap_or(false);

                    let name = match param.get(NAME_FIELD).and_then(|name| name.as_str()) {
                        Some(x) => x,
                        None => {
                            return Err(PayloadValidationError::FieldMissing(
                                String::from(NAME_FIELD),
                                param.clone(),
                            ));
                        }
                    };

                    let schema = match param.get(SCHEMA_FIELD) {
                        Some(x) => x,
                        None => todo!(),
                    };

                    if let Some(header_value) =
                        headers.and_then(|headers| headers.get(&UniCase::<String>::from(name)))
                    {
                        if let Err(e) = Self::simple_validation(schema, &json!(header_value)) {
                            return Err(e);
                        }
                    } else if is_required {
                        return Err(PayloadValidationError::InvalidHeaders(format!(
                            "Missing request header: '{}'",
                            name
                        )));
                    }
                }
            }
        }
        Ok(())
    }

    fn simple_validation(schema: &Value, instance: &Value) -> Result<(), PayloadValidationError> {
        if let Err(e) = jsonschema::validate(schema, instance) {
            return Err(PayloadValidationError::InvalidSchema(format!(
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
    ) -> Result<(), PayloadValidationError> {
        let full_pointer_path = format!("@@root#/{}", json_path.format_path());
        let schema = json!({
            REF_FIELD: full_pointer_path
        });

        let validator = match self.validator_options.build(&schema) {
            Ok(val) => val,
            Err(_) => {
                return Err(PayloadValidationError::InvalidSchema(format!(
                    "Could not construct validator for json_path {}",
                    json_path.format_path()
                )));
            }
        };

        match validator.validate(instance) {
            Ok(_) => Ok(()),
            Err(e) => Err(PayloadValidationError::InvalidRequest(format!(
                "Schema validation failed: {}",
                e.to_string()
            ))),
        }
    }

    fn check_required_params(
        &self,
        param_schemas: &Value,
        request_params: Option<&HashMap<UniCase<String>, String>>,
    ) -> Result<(), PayloadValidationError> {
        if let (Some(headers), Some(param_schemas)) = (request_params, param_schemas.as_array()) {
            for param in param_schemas {
                let param_name = param
                    .get(NAME_FIELD)
                    .and_then(|name| name.as_str())
                    .unwrap();
                let section = param.get("in").and_then(|name| name.as_str()).unwrap();
                let param_required = param
                    .get(REQUIRED_FIELD)
                    .and_then(|required| required.as_bool())
                    .unwrap_or(false);

                if !headers.contains_key(&UniCase::<String>::from(param_name)) && param_required {
                    return Err(PayloadValidationError::InvalidRequest(format!(
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
    ) -> Result<(), PayloadValidationError> {
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
    ) -> Result<(), PayloadValidationError> {
        let (operation, mut path) = match self.traverser.get_operation(path, method) {
            Err(e) => return Err(e),
            Ok(val) => (val.0, val.1),
        };

        let request_schema = match Self::extract_content_type(headers) {
            None => None,
            Some(content_type) => {
                path.add_segment(REQUEST_BODY_FIELD)
                    .add_segment(CONTENT_FIELD)
                    .add_segment(content_type)
                    .add_segment(SCHEMA_FIELD);
                self.traverser
                    .get_request_body(&operation, content_type)
                    .unwrap_or_else(|_| None)
            }
        };

        let spec_parameters = self.traverser.get_request_parameters(&operation)?;

        if let Err(e) = match (body.is_some(), request_schema) {
            (true, Some(request_body_schema)) => {
                self.validate_body(&path, &request_body_schema.value(), body)
            }

            (true, None) => Err(PayloadValidationError::InvalidRequest(
                "Request body provided when endpoint has no request schema defined".to_string(),
            )),

            (_, _) => Ok(()),
        } {
            return Err(e);
        }

        if let Err(e) = match (headers.is_some(), &spec_parameters) {
            // if we have header params in our request and the spec contains params, do the validation.
            (true, Some(request_params)) => self.validate_headers(&request_params.value(), headers),

            // If no header params were provided and the spec contains params,
            // check to see if there are any required header params.
            (false, Some(request_params)) => {
                self.check_required_params(&request_params.value(), None)
            }

            // passthrough
            (_, _) => Ok(()),
        } {
            return Err(e);
        }

        if let Err(e) = match (query_params.is_some(), &spec_parameters) {
            // If we have query params in our request and the spec contains params, do the validation.
            (true, Some(request_params)) => {
                self.validate_query_params(&request_params.value(), query_params)
            }

            // If no query params were provided and the spec contains params,
            // check to see if there are any required query params.
            (false, Some(request_params)) => {
                self.check_required_params(&request_params.value(), None)
            }

            // If query params were provided, but the spec contains no param definitions, raise a validation error.
            (true, None) => Err(PayloadValidationError::InvalidRequest(
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

//type TraverseResult = Result<Arc<Value>, PayloadValidationError>;
type TraverseResult<'a> = Result<SearchResult<'a>, PayloadValidationError>;

pub enum SearchResult<'a> {
    Arc(Arc<Value>),
    Ref(&'a Value),
}

impl<'a> SearchResult<'a> {
    fn value(&'a self) -> &'a Value {
        match self {
            SearchResult::Arc(arc_val) => arc_val,
            SearchResult::Ref(val) => val,
        }
    }
}

struct OpenApiTraverser {
    specification: Value,
    resolved_references: DashMap<String, Arc<Value>>,
}

impl OpenApiTraverser {
    fn new(specification: Value) -> Self {
        Self {
            specification,
            resolved_references: DashMap::new(),
        }
    }

    /// Finds and returns the matching `operation` from the OpenAPI `specification` based on the `request path` and `method`.
    ///
    /// # Arguments
    ///
    /// * `request_path` - The path of the incoming request (e.g., "/users/123")
    /// * `request_method` - The HTTP method of the request (e.g., "GET", "POST")
    ///
    /// # Returns
    ///
    /// * `Ok((Arc<Value>, JsonPath))` - A tuple containing:
    ///   - An Arc-wrapped JSON Value representing the operation object from the OpenAPI spec
    ///   - A JsonPath object representing the path to the operation in the spec
    /// * `Err(PayloadValidationError)` - Returns an error if no matching `operation` is found or if the
    ///   `specification` is invalid
    fn get_operation(
        &self,
        request_path: &str,
        request_method: &str,
    ) -> Result<(Arc<Value>, JsonPath), PayloadValidationError> {
        log::debug!("Looking for path '{request_path}' and method '{request_method}'");

        // Grab all paths from the spec
        if let Ok(spec_paths) = self.get_paths() {
            // For each path there are 1 to n methods.
            if let Some(spec_paths) = spec_paths.value().as_object() {
                for (spec_path, spec_path_methods) in spec_paths.iter() {
                    let operations = match spec_path_methods.as_object() {
                        Some(x) => x,
                        None => {
                            return Err(PayloadValidationError::InvalidSchema(format!(
                                "Specification path {} is not an object type",
                                spec_path
                            )));
                        }
                    };

                    // Grab the operation matching our request method and test to see if the path matches our request path.
                    // If both method and path match, then we've found the operation associated with the request.
                    if let Some(operation) = operations.get(request_method) {
                        if Self::matches_spec_path(request_path, spec_path) {
                            log::debug!(
                                "OpenAPI path '{spec_path}' and method '{request_method}' match provided request path '{request_path}' and method '{request_method}'."
                            );
                            let mut json_path = JsonPath::new();
                            json_path
                                .add_segment(PATHS_FIELD)
                                .add_segment(spec_path)
                                .add_segment(&request_method.to_lowercase());
                            return Ok((Arc::new(operation.clone()), json_path));
                        }
                    }
                }
            }
        }
        Err(PayloadValidationError::InvalidPath(format!(
            "No path found in specification matching provided path '{request_path}' and method '{request_method}'"
        )))
    }

    /// Determines if a given `path` matches an OpenAPI `specification` `path` pattern.
    ///
    /// This function checks if a request path matches a `specification` path, handling path parameters
    /// enclosed in curly braces (e.g., "/users/{id}").
    ///
    /// # Arguments
    ///
    /// * `path_to_match` - The actual request path to check against the `specification`.
    /// * `spec_path` - The `specification` path pattern that may contain path parameters in the format "{param_name}".
    ///
    /// # Returns
    ///
    /// * `true` if the path matches the `specification` pattern, accounting for path parameters.
    /// * `false` if the path does not match the pattern or has a different number of segments.
    fn matches_spec_path(path_to_match: &str, spec_path: &str) -> bool {
        // If the spec path we are checking contains no path parameters,
        // then we can simply compare path strings.
        if !(spec_path.contains("{") && spec_path.contains("}")) {
            spec_path == path_to_match

        // if the request path contains path parameters, we need to compare each segment
        // When we reach a segment that is a parameter, compare the value in the path to the value in the spec.
        } else {
            let target_segments = path_to_match.split(PATH_SEPARATOR).collect::<Vec<&str>>();
            let spec_segments = spec_path.split(PATH_SEPARATOR).collect::<Vec<&str>>();

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

    /// Retrieves the request body schema from an operation based on a specified content type.
    ///
    /// # Arguments
    ///
    /// * `operation` - A reference to a JSON Value representing an operation in the OpenAPI specification
    /// * `content_type` - The media type (MIME type) to search for in the content field (e.g., "application/json")
    ///
    /// # Returns
    ///
    /// * `Ok(None)` - If the requestBody field is not present, or if the schema field is not present
    /// * `Ok(Some(SearchResult))` - If the requestBody with the specified content type and schema is found
    /// * `Err(PayloadValidationError)` - If there are issues finding required fields in
    fn get_request_body<'a>(
        &'a self,
        operation: &'a Value,
        content_type: &str,
    ) -> Result<Option<SearchResult<'a>>, PayloadValidationError> {
        // 'requestBody' is optional
        let request_body_node = match self.get_optional_spec_node(operation, REQUEST_BODY_FIELD)? {
            None => return Ok(None),
            Some(res) => res,
        };

        // 'content' is required
        let binding = self.get_spec_node(&request_body_node.value(), CONTENT_FIELD)?;
        let request_body_content_node = binding.value();

        // 'media type' is mandatory (application/json, application/xml, etc.)
        let binding = self.get_spec_node(&request_body_content_node, content_type)?;
        let request_body_media_type_node = binding.value();

        // 'schema' is optional
        match self.get_optional_spec_node(&request_body_media_type_node, SCHEMA_FIELD)? {
            None => Ok(None),
            Some(res) => match res {
                SearchResult::Arc(arc) => Ok(Some(SearchResult::Arc(arc))),
                SearchResult::Ref(val) => Ok(Some(SearchResult::Arc(Arc::new(val.clone())))),
            },
        }
    }

    /// Retrieves the `paths` object from the OpenAPI `specification`.
    ///
    /// # Returns
    ///
    /// * `TraverseResult` - A Result containing either:
    ///   * `Ok(Arc<Value>)` - An Arc pointer to the `paths` object if found
    ///   * `Err(PayloadValidationError)` - An error if the `paths` field is missing or reference resolution fails
    fn get_paths(&self) -> TraverseResult {
        self.get_spec_node(&self.specification, PATHS_FIELD)
    }

    /// Retrieves the "parameters" field from a given operation in an OpenAPI specification.
    ///
    /// # Arguments
    ///
    /// * `operation` - A reference to a JSON Value representing an operation in the OpenAPI specification
    ///
    /// # Returns
    ///
    /// * `Ok(None)` - If the "parameters" field doesn't exist in the operation
    /// * `Ok(Some(SearchResult))` - If the "parameters" field exists, returns the field's value wrapped in `Some`
    /// * `Err(PayloadValidationError)` - If any error occurs during retrieval (other than the field being optional)
    fn get_request_parameters<'a>(
        &'a self,
        operation: &'a Value,
    ) -> Result<Option<SearchResult<'a>>, PayloadValidationError> {
        match match self.get_optional_spec_node(operation, PARAMETERS_FIELD) {
            Ok(res) => res,
            Err(e) if e.kind() == PayloadValidationErrorKind::Missing => return Ok(None),
            Err(e) => return Err(e),
        } {
            None => Ok(None),
            Some(val) => Ok(Some(val)),
        }
    }

    /// Retrieves the security requirements for an operation from an OpenAPI specification.
    ///
    /// # Arguments
    ///
    /// * `operation` - A reference to a JSON Value representing an operation in the OpenAPI specification
    ///
    /// # Returns
    ///
    /// * `Ok(Some(SearchResult))` - If the security field exists in the operation
    /// * `Ok(None)` - If the security field doesn't exist in the operation
    /// * `Err(PayloadValidationError)` - If an error occurs during retrieval of the security field
    fn get_request_security<'a>(
        &'a self,
        operation: &'a Value,
    ) -> Result<Option<SearchResult<'a>>, PayloadValidationError> {
        match self.get_optional_spec_node(operation, SECURITY_FIELD)? {
            None => Ok(None),
            Some(val) => Ok(Some(val)),
        }
    }

    /// Attempts to retrieve an optional field from a JSON operation node, handling the case where the field may not exist.
    ///
    /// # Arguments
    ///
    /// * `operation` - A reference to a JSON Value representing an operation in the OpenAPI specification
    /// * `field` - The name of the field to extract from the operation
    ///
    /// # Returns
    ///
    /// * `Ok(Some(SearchResult))` - If the field exists, returns the field's value wrapped in `Some`
    /// * `Ok(None)` - If the field doesn't exist (and is therefore optional)
    /// * `Err(PayloadValidationError)` - If any error other than a missing field occurs during retrieval
    fn get_optional_spec_node<'a>(
        &'a self,
        operation: &'a Value,
        field: &str,
    ) -> Result<Option<SearchResult<'a>>, PayloadValidationError> {
        match self.get_spec_node(operation, field) {
            Ok(security) => Ok(Some(security)),
            Err(e) if e.kind() == PayloadValidationErrorKind::Missing => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Retrieves a specified field from a JSON value, handling possible reference resolution.
    ///
    /// # Arguments
    ///
    /// * `value` - The JSON value to extract a field from
    /// * `field` - The name of the field to extract from the value
    ///
    /// # Returns
    ///
    /// * `Ok(SearchResult<'a>)` - A SearchResult containing the extracted field value
    /// * `Err(PayloadValidationError)` - RequiredFieldMissing error if the field doesn't exist
    fn get_spec_node<'a>(
        &'a self,
        value: &'a Value,
        field: &str,
    ) -> Result<SearchResult<'a>, PayloadValidationError> {
        let ref_result = self.resolve_possible_ref(value)?;

        match ref_result {
            SearchResult::Arc(arc_value) => match arc_value.get(field) {
                None => Err(PayloadValidationError::FieldMissing(
                    String::from(field),
                    value.clone(),
                )),
                Some(val) => Ok(SearchResult::Arc(Arc::new(val.clone()))),
            },
            SearchResult::Ref(ref_value) => match ref_value.get(field) {
                None => Err(PayloadValidationError::FieldMissing(
                    String::from(field),
                    value.clone(),
                )),
                Some(val) => Ok(SearchResult::Ref(val)),
            },
        }
    }

    /// Attempts to retrieve a field from a JSON value.
    ///
    /// # Arguments
    ///
    /// * `value` - The JSON value to extract a field from
    /// * `field` - The name of the field to retrieve from the value
    ///
    /// # Returns
    ///
    /// * `Ok(SearchResult::Ref)` - A reference to the found JSON value if the field exists
    /// * `Err(PayloadValidationError::OptionalFieldMissing)` - Error if the field doesn't exist,
    ///   containing the field name and a clone of the original value
    fn get_node<'a>(value: &'a Value, field: &str) -> TraverseResult<'a> {
        match value.get(field) {
            None => Err(PayloadValidationError::FieldMissing(
                String::from(field),
                value.clone(),
            )),

            Some(val) => Ok(SearchResult::Ref(val)),
        }
    }

    /// Attempts to resolve a JSON reference in a node, returning the referenced value or the original node.
    ///
    /// # Arguments
    /// * `node` - The JSON value node to check for a reference (`$ref` field)
    ///
    /// # Returns
    ///
    /// * `Ok(SearchResult::Arc)` - An Arc-wrapped JSON Value representing the resolved reference
    /// * `Ok(SearchResult::Ref)` - The original node if it doesn't contain a reference
    /// * `Err(PayloadValidationError)` - An error if reference resolution fails
    fn resolve_possible_ref<'a>(&'a self, node: &'a Value) -> TraverseResult<'a> {
        // If the ref node exists, resolve it
        if let Some(ref_string) = node.get(REF_FIELD).and_then(|val| val.as_str()) {
            let entry = self.resolved_references.entry(String::from(ref_string));
            return match entry {
                Entry::Occupied(e) => Ok(SearchResult::Arc(e.get().clone())),
                Entry::Vacant(_) => {
                    let mut seen_references = HashSet::new();
                    let res = self.get_reference_path(ref_string, &mut seen_references)?;
                    return Ok(res);
                }
            };
        }

        Ok(SearchResult::Ref(node))
    }

    /// Resolves a JSON `reference` path within an OpenAPI `specification`.
    ///
    /// # Arguments
    ///
    /// * `ref_string` - A string representing the JSON `reference` path to resolve (e.g., "/components/schemas/Pet")
    /// * `seen_references` - A mutable HashSet that tracks references already processed to detect circular references
    ///
    /// # Returns
    ///
    /// * `Ok(Arc<Value>)` - An Arc-wrapped JSON Value representing the resolved reference
    /// * `Err(PayloadValidationError)` - An error if reference resolution fails due to:
    ///   - Circular references
    ///   - Missing fields in the path
    fn get_reference_path(
        &self,
        ref_string: &str,
        seen_references: &mut HashSet<String>,
    ) -> TraverseResult {
        if seen_references.contains(ref_string) {
            return Err(PayloadValidationError::InvalidSchema(format!(
                "Circular reference found when resolving reference string '{}'",
                ref_string
            )));
        }
        seen_references.insert(String::from(ref_string));
        let path = ref_string
            .split(PATH_SEPARATOR)
            .filter(|node| !(*node).is_empty() && (*node != "#"))
            .collect::<Vec<&str>>();
        let mut current_schema = &self.specification;
        for segment in path {
            let refactored_segment = segment.replace(ENCODED_BACKSLASH, PATH_SEPARATOR);
            // Navigate to the next segment
            match current_schema.get(refactored_segment) {
                Some(next) => {
                    current_schema = next;
                }
                None => {
                    return Err(PayloadValidationError::FieldMissing(
                        String::from(segment),
                        current_schema.clone(),
                    ));
                }
            }
        }
        let current_schema = self.resolve_possible_ref(current_schema)?;
        Ok(current_schema)
    }
}

#[derive(Ord, PartialOrd, Eq, PartialEq)]
pub enum PayloadValidationErrorKind {
    InvalidRequest,
    InvalidSpec,
    Missing,
}

#[derive(Debug)]
pub enum PayloadValidationError {
    InvalidSchema(String),
    InvalidRequest(String),
    InvalidResponse(String),
    InvalidPath(String),
    InvalidMethod(String),
    InvalidContentType(String),
    InvalidAccept(String),
    InvalidQueryParameters(String),
    InvalidHeaders(String),
    FieldMissing(String, Value),
    InvalidRef(String),
    InvalidType(String),
}

impl Display for PayloadValidationError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            PayloadValidationError::InvalidSchema(msg) => write!(f, "InvalidSchema: {}", msg),
            PayloadValidationError::InvalidRequest(msg) => write!(f, "InvalidRequest: {}", msg),
            PayloadValidationError::InvalidResponse(msg) => write!(f, "InvalidResponse: {}", msg),
            PayloadValidationError::InvalidPath(msg) => write!(f, "InvalidPath: {}", msg),
            PayloadValidationError::InvalidMethod(msg) => write!(f, "InvalidMethod: {}", msg),
            PayloadValidationError::InvalidContentType(msg) => {
                write!(f, "InvalidContentType: {}", msg)
            }
            PayloadValidationError::InvalidAccept(msg) => write!(f, "InvalidAccept: {}", msg),
            PayloadValidationError::InvalidQueryParameters(msg) => {
                write!(f, "InvalidQueryParameters: {}", msg)
            }
            PayloadValidationError::InvalidHeaders(msg) => write!(f, "InvalidHeaders: {}", msg),
            PayloadValidationError::FieldMissing(msg, node) => {
                write!(
                    f,
                    "RequiredFieldMissing: Object {} is missing required field {}",
                    node, msg
                )
            }
            PayloadValidationError::InvalidRef(msg) => write!(f, "InvalidRef: {}", msg),
            PayloadValidationError::InvalidType(msg) => write!(f, "InvalidType: {}", msg),
        }
    }
}

impl PayloadValidationError {
    pub fn kind(&self) -> PayloadValidationErrorKind {
        match self {
            // Invalid request
            PayloadValidationError::InvalidSchema(_)
            | PayloadValidationError::InvalidRequest(_)
            | PayloadValidationError::InvalidResponse(_)
            | PayloadValidationError::InvalidPath(_)
            | PayloadValidationError::InvalidMethod(_)
            | PayloadValidationError::InvalidContentType(_)
            | PayloadValidationError::InvalidAccept(_)
            | PayloadValidationError::InvalidQueryParameters(_)
            | PayloadValidationError::InvalidHeaders(_) => {
                PayloadValidationErrorKind::InvalidRequest
            }

            // Can be an error in some situations, but not always
            PayloadValidationError::FieldMissing(_, _) => PayloadValidationErrorKind::Missing,

            // Invalid specification
            PayloadValidationError::InvalidRef(_) | PayloadValidationError::InvalidType(_) => {
                PayloadValidationErrorKind::InvalidSpec
            }
        }
    }
}

impl std::error::Error for PayloadValidationError {}

#[derive(Debug, Clone)]
struct JsonPath(pub Vec<String>);

impl JsonPath {
    fn new() -> Self {
        JsonPath(Vec::new())
    }

    fn add_segment(&mut self, segment: &str) -> &mut Self {
        if segment.contains(PATH_SEPARATOR) {
            let segment = segment.replace(PATH_SEPARATOR, ENCODED_BACKSLASH);
            self.0.push(segment);
        } else {
            self.0.push(segment.to_owned());
        }

        self
    }

    fn format_path(&self) -> String {
        self.0.join(PATH_SEPARATOR)
    }
}

#[cfg(test)]
mod test {
    use crate::{JsonPath, OpenApiPayloadValidator, OpenApiTraverser};
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
        let validator = OpenApiPayloadValidator::new(specification).unwrap();
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
        let validator = OpenApiPayloadValidator::new(specification).unwrap();
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
        let validator = OpenApiPayloadValidator::new(specification).unwrap();
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
        let validator = OpenApiPayloadValidator::new(specification).unwrap();
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
