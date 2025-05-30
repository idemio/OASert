use crate::types::Operation;
use crate::{
    JsonPath, PATH_SEPARATOR, PATHS_FIELD, REF_FIELD,
    ValidationError, ValidationErrorKind,
};
use dashmap::{DashMap, Entry};
use serde_json::{Map, Value};
use std::collections::HashSet;
use std::sync::Arc;

type TraverseResult<'a> = Result<SearchResult<'a>, ValidationError>;

#[derive(Debug)]
pub(crate) enum SearchResult<'a> {
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
    pub fn new(specification: Value) -> Self {
        Self {
            specification,
            resolved_references: DashMap::new(),
            resolved_operations: DashMap::new(),
        }
    }

    pub fn get_operation(
        &self,
        request_path: &str,
        request_method: &str,
    ) -> Result<Arc<Operation>, ValidationError> {
        log::debug!("Looking for path '{request_path}' and method '{request_method}'");

        let entry = self
            .resolved_operations
            .entry((String::from(request_path), String::from(request_method)));
        match entry {
            Entry::Occupied(e) => Ok(e.get().clone()),
            Entry::Vacant(_) => {
                // Grab all paths from the spec
                if let Ok(spec_paths) = get_as_object(&self.specification, PATHS_FIELD) {
                    for (spec_path, spec_path_methods) in spec_paths.iter() {
                        let operations = require_object(spec_path_methods)?;

                        // Grab the operation matching our request method and test to see if the path matches our request path.
                        // If both method and path match, then we've found the operation associated with the request.
                        if let Some(operation) = operations.get(request_method) {
                            if Self::matches_spec_path(request_path, spec_path) {
                                log::debug!(
                                    "OpenAPI path '{spec_path}' and method '{request_method}' match provided request path '{request_path}' and method '{request_method}'."
                                );
                                let mut json_path = JsonPath::new();
                                json_path
                                    .add(PATHS_FIELD)
                                    .add(spec_path)
                                    .add(&request_method.to_lowercase());

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
                Err(ValidationError::MissingOperation)
            }
        }
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
        node: &'a Value,
        field: &str,
    ) -> Result<Option<SearchResult<'a>>, ValidationError>
    where
        Self: 'a,
    {
        log::trace!(
            "Attempting to find optional field '{}' from '{}'",
            field,
            node.to_string()
        );
        match self.get_required_spec_node(node, field) {
            Ok(security) => Ok(Some(security)),
            Err(e) if e.kind() == ValidationErrorKind::MismatchingSchema => Ok(None),
            Err(e) => Err(e),
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
        node: &'a Value,
        field: &str,
    ) -> Result<SearchResult<'a>, ValidationError> {
        log::trace!(
            "Attempting to find required field '{}' from '{}'",
            field,
            node.to_string()
        );
        let ref_result = self.resolve_possible_ref(node)?;
        match ref_result {
            SearchResult::Arc(val) => match val.get(field) {
                None => Err(ValidationError::FieldMissing),
                Some(v) => Ok(SearchResult::Arc(Arc::new(v.clone()))),
            },
            SearchResult::Ref(val) => match val.get(field) {
                None => Err(ValidationError::FieldMissing),
                Some(v) => Ok(SearchResult::Ref(v)),
            },
        }
    }

    /// Resolves a JSON node that might contain a `reference` (via "$ref" field).
    ///
    /// # Arguments
    /// * `self` - The OpenApiTraverser instance that contains the `reference` resolution context
    /// * `node` - The JSON value that might contain a `reference` to resolve
    ///
    /// # Returns
    /// * `Ok(SearchResult::Arc)` - If the node contains a `reference` that has been previously resolved
    /// * `Ok(SearchResult::Ref)` - If the node does not contain a `reference`
    /// * `Err(ValidationError)` - If `reference` resolution fails (e.g., circular `reference` or missing field)
    fn resolve_possible_ref<'a>(&'a self, node: &'a Value) -> TraverseResult<'a> {
        if let Ok(ref_string) = get_as_str(node, REF_FIELD) {
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

    /// Resolves a `reference` string by navigating through the specification object to find the referenced schema.
    ///
    /// # Arguments
    /// * `ref_string` - A string containing a JSON `reference` path (e.g., "#/components/schemas/Pet")
    /// * `seen_references` - A mutable HashSet tracking references already encountered to detect circular references
    ///
    /// # Returns
    /// * `Ok(SearchResult)` - The resolved schema if the `reference` was successfully resolved
    /// * `Err(ValidationError::CircularReference)` - If a circular `reference` is detected
    /// * `Err(ValidationError::FieldMissing)` - If a path cannot be found in the specification
    fn get_reference_path<'a, 'b>(
        &'a self,
        ref_string: &str,
        seen_references: &mut HashSet<String>,
    ) -> TraverseResult<'b>
    where
        'a: 'b,
    {
        if seen_references.contains(ref_string) {
            return Err(ValidationError::CircularReference);
        }
        seen_references.insert(String::from(ref_string));
        let path = ref_string
            .split(PATH_SEPARATOR)
            .filter(|node| !(*node).is_empty() && (*node != "#"))
            .collect::<Vec<&str>>()
            .join("/");

        let current_schema = match &self.specification.pointer(&path) {
            None => {
                return Err(ValidationError::FieldMissing);
            }
            Some(v) => self.resolve_possible_ref(v)?,
        };
        Ok(current_schema)
    }
}

/// Retrieves a `boolean` value from a JSON object at the specified field.
///
/// # Arguments
///
/// * `node` - A reference to the JSON `Value` object from which to retrieve the field.
/// * `field` - A string slice representing the key of the field to be extracted.
///
/// # Returns
///
/// * `Ok(bool)` - If the field exists and its value can be successfully interpreted as a `boolean`.
/// * `Err(ValidationError::FieldMissing)` - If the field is not present in the given JSON object.
/// * `Err(ValidationError::UnexpectedType)` - If the field exists but its value is not a `boolean`.
pub(crate) fn get_as_bool<'a, 'b>(node: &'a Value, field: &str) -> Result<bool, ValidationError>
where
    'a: 'b,
{
    log::trace!("Grabbing {} from {} as a bool.", field, node.to_string());
    match node.get(field) {
        None => Err(ValidationError::FieldMissing),
        Some(found) => require_bool(found),
    }
}

/// Retrieves the value of a specified field from a JSON object.
///
/// # Arguments
///
/// * `node` - A reference to a `Value` representing the JSON object to search within.
/// * `field` - A string slice representing the key of the field to be retrieved.
///
/// # Returns
///
/// * `Ok(&Value)` - The value associated with the specified field if it exists.
/// * `Err(ValidationError::FieldMissing)` - An error indicating that the specified field was not found in the JSON object.
pub(crate) fn get_as_any<'a, 'b>(node: &'a Value, field: &str) -> Result<&'b Value, ValidationError>
where
    'a: 'b,
{
    log::trace!("Grabbing {} from {} as a str.", field, node.to_string());
    match node.get(field) {
        None => Err(ValidationError::FieldMissing),
        Some(found) => Ok(found),
    }
}

/// Retrieves the value of a specified field from a `Value` and attempts to return it as a string.
///
/// # Arguments
///
/// * `node` - A reference to a `Value` object representing the data structure to be queried.
/// * `field` - A string slice representing the key (field name) to retrieve from the `node`.
///
/// # Returns
///
/// * `Ok(&str)` - If the field exists in the `node` and its value can successfully be interpreted as a string.
/// * `Err(ValidationError::FieldMissing)` - If the specified field does not exist in the `node`.
/// * `Err(ValidationError::UnexpectedType)` - If the field exists but its value is not a string.
pub(crate) fn get_as_str<'a, 'b>(node: &'a Value, field: &str) -> Result<&'b str, ValidationError>
where
    'a: 'b,
{
    log::trace!("Grabbing {} from {} as a str.", field, node.to_string());
    match node.get(field) {
        None => Err(ValidationError::FieldMissing),
        Some(found) => require_str(found),
    }
}

/// Attempts to retrieve the value of a specified field from a `Value` object and interpret it as an `array`.
///
/// # Arguments
///
/// * `node` - A reference to a `Value` object from which the field will be retrieved.
/// * `field` - A string slice representing the name of the field to extract.
///
/// # Returns
///
/// * `Ok(&Vec<Value>)` - If the field is found, and its value is an `array`.
/// * `Err(ValidationError::FieldMissing)` - If the specified field is not present in the `node`.
/// * `Err(ValidationError::UnexpectedType)` - If the field is found but, its value is not of type `array`.
pub(crate) fn get_as_array<'a, 'b>(
    node: &'a Value,
    field: &str,
) -> Result<&'b Vec<Value>, ValidationError>
where
    'a: 'b,
{
    log::trace!("Grabbing {} from {} as an array.", field, node.to_string());
    match node.get(field) {
        None => Err(ValidationError::FieldMissing),
        Some(found) => require_array(found),
    }
}

/// Attempts to retrieve a field from a JSON `Value` as an `object`, returning an error if the field
/// is missing or the value is not an `object`.
///
/// # Arguments
/// - `node`: A reference to a `Value` that contains the data to be searched.
/// - `field`: A string slice referring to the name of the field to extract.
///
/// # Returns
/// * `Ok(&Map<String, Value>)` - If the field is found, and its value is an `object`.
/// * `Err(ValidationError::FieldMissing)` - If the specified field is not present in the `node`.
/// * `Err(ValidationError::UnexpectedType)` - If the field is found, but its value is not of type `object`.
pub(crate) fn get_as_object<'a, 'b>(
    node: &'a Value,
    field: &str,
) -> Result<&'b Map<String, Value>, ValidationError>
where
    'a: 'b,
{
    log::trace!("Grabbing {} from {} as an object.", field, node.to_string());
    match node.get(field) {
        None => Err(ValidationError::FieldMissing),
        Some(found) => require_object(found),
    }
}

/// Attempts to convert the provided JSON value into a boolean.
///
/// # Arguments
///
/// * `node` - A reference to a `Value` (from `serde_json`) representing a JSON structure.
///   It is expected to be a valid JSON value of type boolean.
///
/// # Returns
///
/// * `Ok(bool)` - If the `Value` provided is a boolean
/// * `Err(ValidationError::UnexpectedType)` - If the `Value` provided is not a boolean
pub(crate) fn require_bool<'a, 'b>(node: &'a Value) -> Result<bool, ValidationError>
where
    'a: 'b,
{
    match node.as_bool() {
        None => Err(ValidationError::UnexpectedType),
        Some(bool) => Ok(bool),
    }
}

/// Attempts to extract a `string` (`&str`) from a JSON `Value`.
///
/// # Arguments
/// * `node` - A reference to a JSON `Value` from which the function will attempt to extract a `string`.
///
/// # Returns
/// * `Ok(&str)` - If the `Value` is a `string`.
/// * `Err(ValidationError::UnexpectedType` - If the `Value` is not a `string`
pub(crate) fn require_str<'a, 'b>(node: &'a Value) -> Result<&'b str, ValidationError>
where
    'a: 'b,
{
    match node.as_str() {
        None => Err(ValidationError::UnexpectedType),
        Some(string) => Ok(string),
    }
}

/// Validates and extracts an object from a JSON `Value`.
///
/// # Arguments
///
/// * `node` - A reference to a `Value`, which is expected to be a JSON object.
///
/// # Returns
///
/// * `Ok(&Map<String, Value>)` - If the `Value` is an object
/// * `Err(ValidationError::UnexpectedType)` - If the `Value` is not an object.
pub(crate) fn require_object<'a, 'b>(
    node: &'a Value,
) -> Result<&'b Map<String, Value>, ValidationError>
where
    'a: 'b,
{
    match node.as_object() {
        None => Err(ValidationError::UnexpectedType),
        Some(map) => Ok(map),
    }
}

/// Attempts to ensure that a given JSON `Value` is of `array` type.
///
/// # Arguments
///
/// * `node` - A reference to a `Value` (from `serde_json`) that is evaluated to check whether it is an `array`.
///
/// # Returns
///
/// * `Ok(&Vec<Value>)` - If the `node` is an `array`
/// * `Err(ValidationError::UnexpectedType)` - If the `node` is not an `array`.
pub(crate) fn require_array<'a, 'b>(node: &'a Value) -> Result<&'b Vec<Value>, ValidationError>
where
    'a: 'b,
{
    match node.as_array() {
        None => Err(ValidationError::UnexpectedType),
        Some(array) => Ok(array),
    }
}
