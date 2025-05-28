use crate::types::Operation;
use crate::{
    ENCODED_BACKSLASH, JsonPath, PARAMETERS_FIELD, PATH_SEPARATOR, PATHS_FIELD, REF_FIELD,
    SECURITY_FIELD, ValidationError, ValidationErrorKind,
};
use dashmap::{DashMap, Entry};
use serde_json::{Map, Value};
use std::collections::HashSet;
use std::sync::Arc;

type TraverseResult<'a> = Result<SearchResult<'a>, ValidationError>;

#[derive(Debug)]
pub enum SearchResult<'a> {
    Arc(Arc<Value>),
    Ref(&'a Value),
}

impl<'a> SearchResult<'a> {
    pub(crate) fn value(&'a self) -> &'a Value {
        match self {
            SearchResult::Arc(arc_val) => arc_val,
            SearchResult::Ref(val) => val,
        }
    }
}

pub struct OpenApiTraverser {
    specification: Value,
    resolved_references: DashMap<String, Arc<Value>>,
    resolved_operations: DashMap<(String, String), Arc<Operation>>,
}

impl OpenApiTraverser {
    pub(crate) fn new(specification: Value) -> Self {
        Self {
            specification,
            resolved_references: DashMap::new(),
            resolved_operations: DashMap::new(),
        }
    }

    /// Looks up an OpenAPI operation based on a request path and method.
    ///
    /// # Arguments
    ///
    /// * `request_path` - The actual request path to match against OpenAPI specification paths
    /// * `request_method` - The HTTP method of the request (e.g., "get", "post", "put")
    ///
    /// # Returns
    ///
    /// * `Ok((Arc<Value>, JsonPath))` - A tuple containing:
    ///         - The operation definition as an Arc<Value>
    ///         - A JsonPath pointing to the operation in the specification
    /// * `Err(ValidationError)` - Returns a ValidationError if:
    ///         - The operation is not found for the given path and method
    ///         - The path methods in the specification are not in the expected object format
    pub(crate) fn get_operation(
        &self,
        request_path: &str,
        request_method: &str,
    ) -> Result<Arc<Operation>, ValidationError> {
        log::debug!("Looking for path '{request_path}' and method '{request_method}'");

        // Grab all paths from the spec
        if let Ok(spec_paths) = OpenApiTraverser::get_as_object(&self.specification, PATHS_FIELD) {
            for (spec_path, spec_path_methods) in spec_paths.iter() {
                let operations = match spec_path_methods.as_object() {
                    Some(x) => x,
                    None => {
                        return Err(ValidationError::UnexpectedType(
                            spec_path.to_string(),
                            "object",
                            spec_path_methods.clone(),
                        ));
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
                        let operation = Arc::new(Operation {
                            data: operation.clone(),
                            path: json_path,
                        });

                        if !Self::path_has_parameter(spec_path) {
                            self.resolved_operations.insert(
                                (request_path.to_string(), request_method.to_string()),
                                operation.clone(),
                            );
                        }
                        return Ok(operation);
                    }
                }
            }
        }
        Err(ValidationError::MissingOperation(
            request_path.to_string(),
            request_method.to_string(),
        ))
    }

    fn path_has_parameter(path: &str) -> bool {
        path.contains("{") && path.contains("}")
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
        if !Self::path_has_parameter(spec_path) {
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

    //    fn get_paths(&self) -> TraverseResult {
    //        self.get_required_spec_node(&self.specification, PATHS_FIELD)
    //    }

    /// Retrieves the security requirements specified for an operation in an OpenAPI specification.
    ///
    /// # Arguments
    /// * `operation` - A JSON value representing an operation object from which to extract the security field
    ///
    /// # Returns
    ///
    /// * `Ok(Some(SearchResult))` - If the security field exists in the operation, contains a reference to its value
    /// * `Ok(None)` - If the security field doesn't exist in the operation
    /// * `Err(ValidationError)` - If an error occurs during retrieval (other than the field being missing)
    fn get_request_security<'a>(
        &'a self,
        operation: &'a Value,
    ) -> Result<Option<SearchResult<'a>>, ValidationError> {
        match self.get_optional_spec_node(operation, SECURITY_FIELD)? {
            None => Ok(None),
            Some(val) => Ok(Some(val)),
        }
    }

    /// Retrieves an optional field from a JSON value in an OpenAPI specification.
    ///
    /// This function attempts to get a specified field from a JSON operation object,
    /// but unlike `get_required_spec_node`, it treats missing fields as valid
    /// (returns None) rather than errors.
    ///
    /// # Arguments
    /// * `operation` - The JSON value (typically an operation object) to search within
    /// * `field` - The name of the optional field to extract
    ///
    /// # Returns
    ///
    /// * `Ok(Some(SearchResult))` - If the field exists, returns a wrapped reference
    ///   to the field value, either as an owned `Arc<Value>` or a borrowed reference
    /// * `Ok(None)` - If the specified field doesn't exist in the value
    /// * `Err(ValidationError)` - For any error other than a missing field
    pub(crate) fn get_optional_spec_node<'a>(
        &'a self,
        operation: &'a Value,
        field: &str,
    ) -> Result<Option<SearchResult<'a>>, ValidationError> {
        match self.get_required_spec_node(operation, field) {
            Ok(security) => Ok(Some(security)),
            Err(e) if e.kind() == ValidationErrorKind::MismatchingSchema => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub(crate) fn get_as_str<'a, 'b>(
        node: &'a Value,
        field: &str,
    ) -> Result<&'b str, ValidationError>
    where
        'a: 'b,
    {
        match node.get(field) {
            None => Err(ValidationError::FieldMissing(
                field.to_string(),
                node.clone(),
            )),
            Some(found) => Self::require_str(found),
        }
    }

    pub(crate) fn require_str<'a, 'b>(node: &'a Value) -> Result<&'b str, ValidationError>
    where
        'a: 'b,
    {
        match node.as_str() {
            None => todo!("Invalid type"),
            Some(string) => Ok(string),
        }
    }

    pub(crate) fn get_as_array<'a, 'b>(
        node: &'a Value,
        field: &str,
    ) -> Result<&'b Vec<Value>, ValidationError>
    where
        'a: 'b,
    {
        match node.get(field) {
            None => Err(ValidationError::FieldMissing(
                field.to_string(),
                node.clone(),
            )),
            Some(found) => Self::require_array(found),
        }
    }

    pub(crate) fn get_as_object<'a, 'b>(
        node: &'a Value,
        field: &str,
    ) -> Result<&'b Map<String, Value>, ValidationError>
    where
        'a: 'b,
    {
        match node.get(field) {
            None => Err(ValidationError::FieldMissing(
                field.to_string(),
                node.clone(),
            )),
            Some(found) => Self::require_object(found),
        }
    }

    pub(crate) fn require_object<'a, 'b>(
        node: &'a Value,
    ) -> Result<&'b Map<String, Value>, ValidationError>
    where
        'a: 'b,
    {
        match node.as_object() {
            None => todo!("Invalid type"),
            Some(map) => Ok(map),
        }
    }

    pub(crate) fn require_array<'a, 'b>(node: &'a Value) -> Result<&'b Vec<Value>, ValidationError>
    where
        'a: 'b,
    {
        match node.as_array() {
            None => todo!("Invalid type"),
            Some(array) => Ok(array),
        }
    }

    /// Attempts to retrieve a required field from a JSON value, following any references if present.
    ///
    /// # Arguments
    /// * `value` - The JSON value to search within
    /// * `field` - The name of the required field to extract
    ///
    /// # Returns
    /// * `Ok(SearchResult)` - A wrapped reference to the requested field value, either as an owned `Arc<Value>`
    ///   or a borrowed reference
    /// * `Err(ValidationError::FieldMissing)` - If the specified field doesn't exist in the value
    pub(crate) fn get_required_spec_node<'a>(
        &'a self,
        value: &'a Value,
        field: &str,
    ) -> Result<SearchResult<'a>, ValidationError> {
        let ref_result = self.resolve_possible_ref(value)?;
        match ref_result {
            SearchResult::Arc(val) => match val.get(field) {
                None => Err(ValidationError::FieldMissing(
                    field.to_string(),
                    val.as_ref().clone(),
                )),
                Some(v) => Ok(SearchResult::Arc(Arc::new(v.clone()))),
            },
            SearchResult::Ref(val) => match val.get(field) {
                None => Err(ValidationError::FieldMissing(
                    field.to_string(),
                    val.clone(),
                )),
                Some(v) => Ok(SearchResult::Ref(v)),
            },
        }
    }

    /// Resolves a JSON node that might contain a reference (via "$ref" field).
    ///
    /// # Arguments
    /// * `self` - The OpenApiTraverser instance that contains the reference resolution context
    /// * `node` - The JSON value that might contain a reference to resolve
    ///
    /// # Returns
    /// * `Ok(SearchResult::Arc)` - If the node contains a reference that has been previously resolved
    /// * `Ok(SearchResult::Ref)` - If the node does not contain a reference
    /// * `Err(ValidationError)` - If reference resolution fails (e.g., circular reference or missing field)
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

    /// Resolves a reference string by navigating through the specification object to find the referenced schema.
    ///
    /// # Arguments
    /// * `ref_string` - A string containing a JSON reference path (e.g., "#/components/schemas/Pet")
    /// * `seen_references` - A mutable HashSet tracking references already encountered to detect circular references
    ///
    /// # Returns
    /// * `Ok(SearchResult)` - The resolved schema if the reference was successfully resolved
    /// * `Err(ValidationError::CircularReference)` - If a circular reference is detected
    /// * `Err(ValidationError::FieldMissing)` - If a path segment cannot be found in the specification
    fn get_reference_path<'a, 'b>(
        &'a self,
        ref_string: &str,
        seen_references: &mut HashSet<String>,
    ) -> TraverseResult<'b>
    where
        'a: 'b,
    {
        if seen_references.contains(ref_string) {
            return Err(ValidationError::CircularReference(
                seen_references.len(),
                String::from(ref_string),
            ));
        }
        seen_references.insert(String::from(ref_string));
        let path = ref_string
            .split(PATH_SEPARATOR)
            .filter(|node| !(*node).is_empty() && (*node != "#"))
            .collect::<Vec<&str>>()
            .join("/");

        let current_schema = match &self.specification.pointer(&path) {
            None => {
                return Err(ValidationError::FieldMissing(
                    path,
                    self.specification.clone(),
                ));
            }
            Some(v) => self.resolve_possible_ref(v)?,
        };
        Ok(current_schema)
    }
}
