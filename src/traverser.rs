use crate::error::{
    ComponentSection, OperationSection, Section, SpecificationSection, ValidationErrorType,
};
use crate::types::json_path::JsonPath;
use crate::types::primitive::OpenApiPrimitives;
use crate::types::Operation;
use crate::{PATHS_FIELD, PATH_SEPARATOR, REF_FIELD};
use dashmap::{DashMap, Entry};
use serde_json::{Map, Value};
use std::collections::HashSet;
use std::sync::Arc;

type TraverseResult<'a> = Result<SearchResult<'a>, ValidationErrorType>;

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
    pub(crate) fn new(specification: Value) -> Self {
        Self {
            specification,
            resolved_references: DashMap::new(),
            resolved_operations: DashMap::new(),
        }
    }

    pub fn specification(&self) -> &Value {
        &self.specification
    }

    pub fn get_operation(
        &self,
        request_path: &str,
        request_method: &str,
    ) -> Result<Arc<Operation>, ValidationErrorType> {
        let binding = request_method.to_lowercase();
        let request_method = binding.as_str();
        log::debug!("Looking for path '{request_path}' and method '{request_method}'");

        let entry = self
            .resolved_operations
            .entry((String::from(request_path), String::from(request_method)));

        match entry {
            Entry::Occupied(e) => Ok(e.get().clone()),
            Entry::Vacant(e) => {
                // Grab all paths from the spec
                if let Ok(spec_paths) = self.get_required(&self.specification, PATHS_FIELD) {
                    let spec_paths = Self::require_object(spec_paths.value())?;
                    for (spec_path, spec_path_methods) in spec_paths {
                        let operations = Self::require_object(spec_path_methods)?;

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
                                    .add(request_method);

                                let operation = Arc::new(Operation {
                                    data: operation.clone(),
                                    path: json_path,
                                });

                                if !Self::path_has_parameter(spec_path) {
                                    e.insert(operation.clone());
                                }

                                return Ok(operation);
                            }
                        }
                    }
                }
                Err(ValidationErrorType::FieldExpected(
                    format!("{}@{}", request_path, request_method),
                    Section::Specification(SpecificationSection::Paths(OperationSection::Other)),
                ))
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
    pub(crate) fn get_optional<'a>(
        &'a self,
        node: &'a Value,
        field: &str,
    ) -> Result<Option<SearchResult<'a>>, ValidationErrorType>
    where
        Self: 'a,
    {
        match self.get_required(node, field) {
            Ok(security) => Ok(Some(security)),
            Err(e) => match e {
                ValidationErrorType::FieldExpected(_, _) => Ok(None),
                _ => Err(e),
            },
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
    pub(crate) fn get_required<'a>(
        &'a self,
        node: &'a Value,
        field: &str,
    ) -> Result<SearchResult<'a>, ValidationErrorType> {
        log::trace!(
            "Attempting to find required field '{}' from '{}'",
            field,
            node.to_string()
        );
        let ref_result = self.resolve_possible_ref(node)?;
        match ref_result {
            SearchResult::Arc(val) => match val.get(field) {
                None => Err(ValidationErrorType::FieldExpected(
                    field.to_string(),
                    Section::Specification(SpecificationSection::Components(
                        ComponentSection::Schemas,
                    )),
                )),
                Some(v) => Ok(SearchResult::Arc(Arc::new(v.clone()))),
            },
            SearchResult::Ref(val) => match val.get(field) {
                None => Err(ValidationErrorType::FieldExpected(
                    field.to_string(),
                    Section::Specification(SpecificationSection::Components(
                        ComponentSection::Schemas,
                    )),
                )),
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
        if let Ok(ref_string) = Self::get_as_str(node, REF_FIELD) {
            let entry = self.resolved_references.entry(String::from(ref_string));
            return match entry {
                Entry::Occupied(entry) => Ok(SearchResult::Arc(entry.get().clone())),
                Entry::Vacant(entry) => {
                    let mut seen_references = HashSet::new();
                    let res = self.get_reference_path(ref_string, &mut seen_references)?;
                    let res = match res {
                        SearchResult::Arc(val) => {
                            let ret = val;
                            entry.insert(ret.clone());
                            ret
                        }
                        SearchResult::Ref(val) => {
                            let res = Arc::new(val.clone());
                            entry.insert(res.clone());
                            res
                        }
                    };
                    return Ok(SearchResult::Arc(res));
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
        ref_string: &'a str,
        seen_references: &mut HashSet<&'a str>,
    ) -> TraverseResult<'b>
    where
        'a: 'b,
    {
        if seen_references.contains(ref_string) {
            return Err(ValidationErrorType::CircularReference(
                ref_string.to_string(),
                Section::Specification(SpecificationSection::Components(ComponentSection::Schemas)),
            ));
        }
        seen_references.insert(ref_string);
        let mut complete_path = String::from("/");
        let path = ref_string
            .split(PATH_SEPARATOR)
            .filter(|node| !(*node).is_empty() && (*node != "#"))
            .collect::<Vec<&str>>()
            .join("/");
        complete_path.push_str(&path);

        let current_schema = match &self.specification.pointer(&complete_path) {
            None => {
                return Err(ValidationErrorType::FieldExpected(
                    complete_path,
                    Section::Specification(SpecificationSection::Components(
                        ComponentSection::Schemas,
                    )),
                ));
            }
            Some(v) => self.resolve_possible_ref(v)?,
        };
        Ok(current_schema)
    }

    fn get_as_str<'a, 'b>(node: &'a Value, field: &str) -> Result<&'b str, ValidationErrorType>
    where
        'a: 'b,
    {
        match node.get(field) {
            None => Err(ValidationErrorType::FieldExpected(
                field.to_string(),
                Section::Specification(SpecificationSection::Components(ComponentSection::Schemas)),
            )),
            Some(found) => Self::require_str(found),
        }
    }

    /// Attempts to convert the provided JSON value into a boolean.
    ///
    /// # Arguments
    ///
    /// * `node` - A reference to a JSON `Value` from which the function will attempt to extract a `boolean`.
    ///
    /// # Returns
    ///
    /// * `Ok(bool)` - If the `Value` provided is a boolean
    /// * `Err(ValidationError::UnexpectedType)` - If the `Value` provided is not a boolean
    pub(crate) fn require_bool<'a, 'b>(node: &'a Value) -> Result<bool, ValidationErrorType>
    where
        'a: 'b,
    {
        match node.as_bool() {
            None => Err(ValidationErrorType::UnexpectedType {
                expected: OpenApiPrimitives::Bool,
                found: node.clone(),
                section: Section::Specification(SpecificationSection::Components(
                    ComponentSection::Schemas,
                )),
            }),
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
    pub(crate) fn require_str<'a, 'b>(node: &'a Value) -> Result<&'b str, ValidationErrorType>
    where
        'a: 'b,
    {
        match node.as_str() {
            None => Err(ValidationErrorType::UnexpectedType {
                expected: OpenApiPrimitives::String,
                found: node.clone(),
                section: Section::Specification(SpecificationSection::Components(
                    ComponentSection::Schemas,
                )),
            }),
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
    ) -> Result<&'b Map<String, Value>, ValidationErrorType>
    where
        'a: 'b,
    {
        match node.as_object() {
            None => Err(ValidationErrorType::UnexpectedType {
                expected: OpenApiPrimitives::Object,
                found: node.clone(),
                section: Section::Specification(SpecificationSection::Components(
                    ComponentSection::Schemas,
                )),
            }),
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
    pub(crate) fn require_array<'a, 'b>(
        node: &'a Value,
    ) -> Result<&'b Vec<Value>, ValidationErrorType>
    where
        'a: 'b,
    {
        match node.as_array() {
            None => Err(ValidationErrorType::UnexpectedType {
                expected: OpenApiPrimitives::Array,
                found: node.clone(),
                section: Section::Specification(SpecificationSection::Components(
                    ComponentSection::Schemas,
                )),
            }),
            Some(array) => Ok(array),
        }
    }
}
