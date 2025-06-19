use crate::error::{
    ComponentSection, OperationSection, Section, SpecificationSection, ValidationErrorType,
};
use crate::types::json_path::JsonPath;
use crate::types::primitive::OpenApiPrimitives;
use crate::types::Operation;
use crate::{NAME_FIELD, PARAMETERS_FIELD, PATHS_FIELD, PATH_SEPARATOR, REF_FIELD, SCHEMA_FIELD};
use dashmap::{DashMap, Entry};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

type TraverseResult<'a> = Result<SearchResult<'a>, ValidationErrorType>;

#[derive(Debug)]
pub enum SearchResult<'a> {
    /// A search yielding a cached reference.
    Arc(Arc<Value>),
    /// A search result yielding a sub-node (no reference string)
    Ref(&'a Value),
}

impl<'a> SearchResult<'a> {
    pub fn value(&'a self) -> &'a Value {
        match self {
            SearchResult::Arc(arc_val) => arc_val,
            SearchResult::Ref(val) => val,
        }
    }
}

#[derive(Debug, Eq, Hash, PartialEq)]
enum PathSegment {
    Static(String),
    Parameter { name: String, schema: Arc<Value> },
}

struct PathNode {
    children: HashMap<PathSegment, PathNode>,
    operations: HashMap<String, Arc<Operation>>,
}

impl PathNode {
    fn new() -> Self {
        Self {
            children: HashMap::new(),
            operations: HashMap::new(),
        }
    }
}

pub struct OpenApiTraverser {
    specification: Value,
    resolved_references: DashMap<String, Arc<Value>>,
    resolved_operations: DashMap<(String, String), Arc<Operation>>,
    path_router: PathNode,
}

impl OpenApiTraverser {
    pub fn new(specification: Value) -> Result<Self, ValidationErrorType> {
        let mut traverser = Self {
            specification,
            resolved_references: DashMap::new(),
            resolved_operations: DashMap::new(),
            path_router: PathNode::new(),
        };
        traverser.crawl_paths()?;
        Ok(traverser)
    }

    // TODO test to see if this works!
    fn crawl_paths(&mut self) -> Result<(), ValidationErrorType> {
        let spec_paths = Self::get_as_object(&self.specification, PATHS_FIELD)?;
        for (path, method) in spec_paths {
            let operations = Self::require_object(method)?;
            for (method, operation) in operations {
                let segments = path
                    .split(PATH_SEPARATOR)
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>();

                let mut current_node = &mut self.path_router;

                // Build the path tree
                for segment in segments {
                    let path_segment = if Self::is_parameter_segment(segment) {
                        let param_name = Self::extract_parameter_name(segment);

                        // Find parameter schema from operation parameters
                        let parameters = Self::get_as_array(operation, PARAMETERS_FIELD)?;
                        let param_schema = parameters.iter().find(|param| {
                            if let Ok(name) = Self::get_as_str(param, NAME_FIELD) {
                                return name == param_name;
                            }
                            false
                        });
                        let schema = match param_schema {
                            None => continue,
                            Some(found) => match found.get(SCHEMA_FIELD) {
                                None => continue,
                                Some(schema) => schema,
                            },
                        };
                        let schema = Arc::new(schema.clone());
                        PathSegment::Parameter {
                            name: param_name.to_string(),
                            schema,
                        }
                    } else {
                        PathSegment::Static(segment.to_string())
                    };

                    // Create a path node if it doesn't exist
                    current_node = current_node
                        .children
                        .entry(path_segment)
                        .or_insert_with(PathNode::new);
                }

                // Store the operation at this path node
                let mut json_path = JsonPath::new();
                json_path.add(PATHS_FIELD).add(path).add(method);

                let operation = Arc::new(Operation {
                    data: operation.clone(),
                    path: json_path,
                });
                current_node
                    .operations
                    .insert(method.to_string(), operation);
            }
        }
        Ok(())
    }

    fn is_parameter_segment(segment: &str) -> bool {
        segment.starts_with('{') && segment.ends_with('}')
    }

    fn extract_parameter_name(segment: &str) -> &str {
        &segment[1..segment.len() - 1]
    }

    pub fn specification(&self) -> &Value {
        &self.specification
    }

    /// # get_operation_from_path_and_method
    ///
    /// Retrieves an OpenAPI Operation object that matches the provided request path and method.
    ///
    /// ## Arguments
    ///
    /// * `request_path` - The path of the HTTP request (e.g., "/users/{id}")
    /// * `request_method` - The HTTP method of the request (e.g., "get", "post")
    ///
    /// ## Returns
    ///
    /// * `Ok(Arc<Operation>)` - A thread-safe reference to the matching Operation object if found
    /// * `Err(ValidationErrorType::FieldExpected)` - If no matching operation is found in the specification
    ///
    /// ## Example
    ///
    /// ```
    /// use serde_json::json;
    /// use oasert::traverser::OpenApiTraverser;
    ///
    /// // Mini-spec for testing
    /// let specification = json!({
    ///     "openapi": "3.0.0",
    ///     "info": {
    ///         "title": "Example API",
    ///         "version": "1.0.0"
    ///     },
    ///     "paths": {
    ///         "/users/{id}": {
    ///             "get": {
    ///                 "summary": "Get a user by ID",
    ///                 "parameters": [
    ///                     {
    ///                       "name": "id",
    ///                       "in": "path",
    ///                       "description": "ID of user to get",
    ///                       "required": true,
    ///                       "schema": {
    ///                         "type": "integer",
    ///                         "format": "int64"
    ///                        }
    ///                     }
    ///                 ]
    ///             }
    ///         }
    ///     }
    /// });
    ///
    /// let traverser = OpenApiTraverser::new(specification).unwrap();
    /// match traverser.get_operation_from_path_and_method("/users/123", "get") {
    ///     Ok(operation) => {
    ///         // Use the operation object
    ///         println!("Found operation: {:?}", operation);
    ///     },
    ///     Err(err) => {
    ///         println!("No matching operation found: {:?}", err);
    ///     }
    /// }
    /// ```
    pub fn get_operation_from_path_and_method(
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
            Entry::Occupied(e) => Ok(e.get().to_owned()),
            Entry::Vacant(e) => {
                // Grab all paths from the spec
                if let Ok(spec_paths) = self.get_required(&self.specification, PATHS_FIELD) {
                    let spec_paths = Self::require_object(spec_paths.value())?;
                    for (spec_path, spec_path_methods) in spec_paths {
                        let operations = Self::require_object(spec_path_methods)?;

                        // Grab the operation matching our request method and test to see if the path matches our request path.
                        // If both method and path match, then we've found the operation associated with the request.
                        if let Some(operation) = operations.get(request_method) {
                            if self.matches_spec_path(operation, request_path, spec_path) {
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
    /// * `operation` - The operation object from the OpenAPI specification to check against.
    /// * `path_to_match` - The actual request path to check against the `specification`.
    /// * `spec_path` - The `specification` path pattern that may contain path parameters in the format "{param_name}".
    ///
    /// # Returns
    ///
    /// * `true` if the path matches the `specification` pattern, accounting for path parameters.
    /// * `false` if the path does not match the pattern or has a different number of segments.
    fn matches_spec_path(&self, operation: &Value, path_to_match: &str, spec_path: &str) -> bool {
        // If the spec path we are checking contains no path parameters,
        // then we can simply compare path strings.
        if !Self::path_has_parameter(spec_path) {
            spec_path == path_to_match

        // if the request path contains path parameters, we need to compare each segment
        // When we reach a segment that is a parameter, compare the value in the path to the value in the spec.
        } else {
            let target_segments = path_to_match.split(PATH_SEPARATOR).collect::<Vec<&str>>();
            let spec_segments = spec_path.split(PATH_SEPARATOR).collect::<Vec<&str>>();

            // If the spec path and request path have different numbers of segments, then they don't match.
            if spec_segments.len() != target_segments.len() {
                return false;
            }

            let parameters = match Self::get_as_array(operation, PARAMETERS_FIELD) {
                Ok(found) => found,
                Err(_) => return false,
            };

            let (matching_segments, segment_count) =
                spec_segments.iter().zip(target_segments.iter()).fold(
                    (0, 0),
                    |(mut matches, mut count), (spec_segment, target_segment)| {
                        count += 1;
                        if let Some(param_name) = spec_segment.find("{").and_then(|start| {
                            spec_segment
                                .find("}")
                                .map(|end| &spec_segment[start + 1..end])
                        }) {
                            // If the spec segment is a parameter, we need to check if the path parameter matches the spec parameter.
                            for param in parameters {
                                // get the name of the parameter.
                                let current_param_name = match Self::get_as_str(param, NAME_FIELD) {
                                    Ok(val) => val,
                                    Err(_) => continue,
                                };

                                if current_param_name == param_name {
                                    let schema = match self.get_required(param, SCHEMA_FIELD) {
                                        Ok(found) => found,
                                        Err(_) => continue,
                                    };
                                    let schema = match self.resolve_possible_ref(schema.value()) {
                                        Ok(val) => val,
                                        Err(_) => continue,
                                    };
                                    let converted_value =
                                        match OpenApiPrimitives::convert_string_to_schema_type(
                                            schema.value(),
                                            target_segment,
                                        ) {
                                            Ok(val) => val,
                                            Err(_) => continue,
                                        };
                                    match jsonschema::validate(schema.value(), &converted_value) {
                                        Ok(_) => {
                                            matches += 1;
                                            break;
                                        }
                                        Err(_) => continue,
                                    }
                                }
                            }
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
    pub fn get_optional<'node>(
        &'node self,
        node: &'node Value,
        field: &str,
    ) -> Result<Option<SearchResult<'node>>, ValidationErrorType>
    where
        Self: 'node,
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
    pub fn get_required<'node>(
        &'node self,
        node: &'node Value,
        field: &str,
    ) -> Result<SearchResult<'node>, ValidationErrorType>
    where
        Self: 'node,
    {
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
    fn resolve_possible_ref<'node>(&'node self, node: &'node Value) -> TraverseResult<'node> {
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
    fn get_reference_path<'node, 'sub_node>(
        &'node self,
        ref_string: &'node str,
        seen_references: &mut HashSet<&'node str>,
    ) -> TraverseResult<'sub_node>
    where
        'node: 'sub_node,
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

    /// Retrieves a string value from a specified field in a JSON Value.
    ///
    /// # Arguments
    /// * `node` - A reference to a JSON `Value` from which to extract the field
    /// * `field` - The name of the field to extract from the JSON `Value`
    ///
    /// # Returns
    /// * `Ok(&str)` - If the field exists and contains a string value
    /// * `Err(ValidationErrorType::FieldExpected)` - If the field doesn't exist in the node
    /// * `Err(ValidationErrorType::UnexpectedType)` - If the field exists but doesn't contain a string value
    fn get_as_str<'node, 'sub_node>(
        node: &'node Value,
        field: &str,
    ) -> Result<&'sub_node str, ValidationErrorType>
    where
        'node: 'sub_node,
    {
        match node.get(field) {
            None => Err(ValidationErrorType::FieldExpected(
                field.to_string(),
                Section::Specification(SpecificationSection::Components(ComponentSection::Schemas)),
            )),
            Some(found) => Self::require_str(found),
        }
    }

    fn get_as_object<'node, 'sub_node>(
        node: &'node Value,
        field: &str,
    ) -> Result<&'sub_node Map<String, Value>, ValidationErrorType>
    where
        'node: 'sub_node,
    {
        match node.get(field) {
            None => Err(ValidationErrorType::FieldExpected(
                field.to_string(),
                Section::Specification(SpecificationSection::Components(ComponentSection::Schemas)),
            )),
            Some(val) => Self::require_object(val),
        }
    }

    fn get_as_array<'node, 'sub_node>(
        node: &'node Value,
        field: &str,
    ) -> Result<&'sub_node Vec<Value>, ValidationErrorType>
    where
        'node: 'sub_node,
    {
        match node.get(field) {
            None => Err(ValidationErrorType::FieldExpected(
                field.to_string(),
                Section::Specification(SpecificationSection::Components(ComponentSection::Schemas)),
            )),
            Some(found) => Self::require_array(found),
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
    pub(crate) fn require_bool(node: &Value) -> Result<bool, ValidationErrorType> {
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
    pub(crate) fn require_str<'node, 'sub_node>(
        node: &'node Value,
    ) -> Result<&'sub_node str, ValidationErrorType>
    where
        'node: 'sub_node,
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
    pub(crate) fn require_object<'node, 'sub_node>(
        node: &'node Value,
    ) -> Result<&'sub_node Map<String, Value>, ValidationErrorType>
    where
        'node: 'sub_node,
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
    pub(crate) fn require_array<'node, 'sub_node>(
        node: &'node Value,
    ) -> Result<&'sub_node Vec<Value>, ValidationErrorType>
    where
        'node: 'sub_node,
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

#[cfg(test)]
mod tests {
    use crate::error::ValidationErrorType;
    use crate::traverser::OpenApiTraverser;
    use crate::types::primitive::OpenApiPrimitives;
    use serde_json::json;

    #[test]
    fn test_matches_spec_path_exact_match() {
        let traverser = OpenApiTraverser::new(json!({})).unwrap();
        let operation = json!({
            "parameters": []
        });
        let spec_path = "/api/users";
        let path_to_match = "/api/users";
        let result = traverser.matches_spec_path(&operation, path_to_match, spec_path);
        assert!(result, "Paths should match exactly");
    }

    #[test]
    fn test_matches_spec_path_with_param_reference() {
        let traverser = OpenApiTraverser::new(json!({
            "components": {
                "schemas": {
                    "userId": {
                        "type": "string",
                        "maxLength": 16
                    }
                }
            }
        }))
        .unwrap();
        let operation = json!({
            "parameters": [
                {
                    "name": "userId",
                    "in": "path",
                    "schema": { "$ref": "#/components/schemas/userId" }
                }
            ]
        });
        let spec_path = "/api/users/{userId}";
        let path_to_match = "/api/users/12345";
        let result = traverser.matches_spec_path(&operation, path_to_match, spec_path);
        assert!(result, "Path with parameter reference should match");
    }

    #[test]
    fn test_matches_spec_path_exact_mismatch() {
        let traverser = OpenApiTraverser::new(json!({})).unwrap();
        let operation = json!({
            "parameters": []
        });
        let spec_path = "/api/users";
        let path_to_match = "/api/products";
        let result = traverser.matches_spec_path(&operation, path_to_match, spec_path);
        assert!(!result, "Different paths should not match");
    }

    #[test]
    fn test_matches_spec_path_with_path_parameter() {
        let traverser = OpenApiTraverser::new(json!({})).unwrap();
        let operation = json!({
            "parameters": [
                {
                    "name": "id",
                    "in": "path",
                    "schema": { "type": "string" }
                }
            ]
        });
        let spec_path = "/api/users/{id}";
        let path_to_match = "/api/users/12345";
        let result = traverser.matches_spec_path(&operation, path_to_match, spec_path);
        assert!(result, "Path with parameter should match");
    }

    #[test]
    fn test_matches_spec_path_with_multiple_path_parameters() {
        let traverser = OpenApiTraverser::new(json!({})).unwrap();
        let operation = json!({
            "parameters": [
                {
                    "name": "userId",
                    "in": "path",
                    "schema": { "type": "string" }
                },
                {
                    "name": "postId",
                    "in": "path",
                    "schema": { "type": "string" }
                }
            ]
        });
        let spec_path = "/api/users/{userId}/posts/{postId}";
        let path_to_match = "/api/users/123/posts/456";
        let result = traverser.matches_spec_path(&operation, path_to_match, spec_path);
        assert!(result, "Path with multiple parameters should match");
    }

    #[test]
    fn test_require_object_with_valid_object() {
        let object_value = json!({"name": "test", "age": 30});
        let result = OpenApiTraverser::require_object(&object_value);
        assert!(result.is_ok());
        let obj = result.unwrap();
        assert_eq!(obj.len(), 2);
        assert_eq!(obj.get("name").unwrap(), &json!("test"));
        assert_eq!(obj.get("age").unwrap(), &json!(30));
    }

    #[test]
    fn test_require_object_with_non_object_type() {
        let string_value = json!("not an object");
        let result = OpenApiTraverser::require_object(&string_value);
        assert!(result.is_err());
        match result {
            Err(ValidationErrorType::UnexpectedType {
                expected, found, ..
            }) => {
                assert_eq!(expected, OpenApiPrimitives::Object);
                assert_eq!(found, string_value);
            }
            _ => panic!("Expected UnexpectedType error"),
        }
    }

    #[test]
    fn test_require_array_with_valid_array() {
        let array_value = json!([1, 2, 3, "test"]);
        let result = OpenApiTraverser::require_array(&array_value);
        assert!(result.is_ok());
        let array = result.unwrap();
        assert_eq!(array.len(), 4);
        assert_eq!(array[0], json!(1));
        assert_eq!(array[1], json!(2));
        assert_eq!(array[2], json!(3));
        assert_eq!(array[3], json!("test"));
    }

    #[test]
    fn test_require_array_with_non_array_type() {
        let object_value = json!({"key": "value"});
        let result = OpenApiTraverser::require_array(&object_value);
        assert!(result.is_err());
        match result {
            Err(ValidationErrorType::UnexpectedType {
                expected, found, ..
            }) => {
                assert_eq!(expected, OpenApiPrimitives::Array);
                assert_eq!(found, object_value);
            }
            _ => panic!("Expected UnexpectedType error"),
        }
    }
}
