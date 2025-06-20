use crate::types::json_path::JsonPath;
use crate::types::primitive::OpenApiPrimitives;
use crate::types::Operation;
use crate::{NAME_FIELD, PARAMETERS_FIELD, PATHS_FIELD, PATH_SEPARATOR, REF_FIELD, SCHEMA_FIELD};
use dashmap::{DashMap, Entry};
use serde_json::{Map, Value};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::fmt::{Display, Formatter};
use std::hash::Hash;
use std::sync::Arc;

type TraverseSearchResult<'a> = Result<SearchResult<'a>, TraverserError<'a>>;
type TraverseOptionalSearchResult<'a> = Result<Option<SearchResult<'a>>, TraverserError<'a>>;
type TraverseTypeResult<'a, T> = Result<&'a T, TraverserError<'a>>;
type TraverseResult<'a> = Result<(), TraverserError<'a>>;
type FindOperationResult<'a> = Result<Arc<Operation>, TraverserError<'a>>;

/// Error types that can occur during OpenAPI specification traversal.
///
/// This enum represents various error conditions that may arise when parsing,
/// validating, or navigating through an OpenAPI specification document.
#[derive(Debug)]
pub enum TraverserError<'a> {
    /// A required field was not found in the specification.
    MissingField(Cow<'a, str>),

    /// The found type does not match the expected type.
    TypeMismatch {
        expected: Cow<'a, str>,
        found: Cow<'a, str>,
    },

    /// The structure of the specification is invalid or contains logical errors.
    InvalidStructure(Cow<'a, str>),

    /// A circular reference was detected in the specification.
    CyclicReference(Cow<'a, str>),

    /// A specified path does not exist in the specification.
    PathNotFound(Cow<'a, str>),
}

impl<'a> TraverserError<'a> {
    /// Creates a new `MissingField` error.
    ///
    /// # Parameters
    /// - `message`: The field name or description that was missing
    #[inline]
    pub(crate) fn missing_field(message: impl Into<Cow<'a, str>>) -> Self {
        Self::MissingField(message.into())
    }

    /// Creates a new `TypeMismatch` error.
    ///
    /// # Parameters
    /// - `expected`: The expected type description
    /// - `found`: The actual type that was found
    #[inline]
    pub(crate) fn type_mismatch(
        expected: impl Into<Cow<'a, str>>,
        found: impl Into<Cow<'a, str>>,
    ) -> Self {
        Self::TypeMismatch {
            expected: expected.into(),
            found: found.into(),
        }
    }

    /// Creates a new `InvalidStructure` error.
    ///
    /// # Parameters
    /// - `message`: Description of the structural issue
    #[inline]
    pub(crate) fn invalid_structure(message: impl Into<Cow<'a, str>>) -> Self {
        Self::InvalidStructure(message.into())
    }

    /// Creates a new `CyclicReference` error.
    ///
    /// # Parameters
    /// - `message`: Description of the circular reference
    #[inline]
    pub(crate) fn cyclic_reference(message: impl Into<Cow<'a, str>>) -> Self {
        Self::CyclicReference(message.into())
    }

    /// Creates a new `PathNotFound` error.
    ///
    /// # Parameters
    /// - `message`: The path that could not be found
    #[inline]
    pub(crate) fn path_not_found(message: impl Into<Cow<'a, str>>) -> Self {
        Self::PathNotFound(message.into())
    }
}

impl Display for TraverserError<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            TraverserError::MissingField(field) => {
                write!(f, "Missing field: {}", field)
            }
            TraverserError::TypeMismatch { expected, found } => {
                write!(f, "Type mismatch: expected {}, found {}", expected, found)
            }
            TraverserError::InvalidStructure(field) => {
                write!(f, "Invalid structure: {}", field)
            }
            TraverserError::CyclicReference(field) => {
                write!(f, "Cyclic reference: {}", field)
            }
            TraverserError::PathNotFound(field) => {
                write!(f, "Path not found: {}", field)
            }
        }
    }
}

impl std::error::Error for TraverserError<'_> {}

/// Represents the result of a search operation within the OpenAPI specification.
///
/// This enum encapsulates different types of references that can be returned
/// from traversal operations, either as cached Arc references or direct references
#[derive(Debug)]
pub enum SearchResult<'a> {
    /// A search yielding a cached reference.
    Arc(Arc<Value>),

    /// A search result yielding a sub-node (no reference string)
    Ref(&'a Value),
}

impl<'a> SearchResult<'a> {
    /// Returns a reference to the underlying JSON value.
    ///
    /// # Returns
    /// A reference to the `Value` contained within this search result.
    pub fn value(&'a self) -> &'a Value {
        match self {
            SearchResult::Arc(arc_val) => arc_val,
            SearchResult::Ref(val) => val,
        }
    }
}

/// Represents a segment in an OpenAPI path specification.
///
/// Path segments can be either static (literal strings) or parameterized
/// (placeholders for dynamic values with associated schemas).
#[derive(Debug, Eq, Hash, PartialEq)]
enum PathSegment {
    /// A static path segment in the path, e.g., "/users"
    Static(String),

    /// A parameter segment in the path, e.g., "/users/{id}"
    Parameter { name: String, schema: Arc<Value> },
}

/// A node in the path routing tree structure.
///
/// Each node represents a segment in the API path hierarchy and can contain
/// child nodes and associated HTTP operations.
struct PathNode {
    /// Child nodes keyed by their path segments
    children: HashMap<PathSegment, PathNode>,

    /// HTTP operations available at this path, keyed by method name
    operations: HashMap<String, Arc<Operation>>,
}

impl PathNode {
    /// Creates a new empty path node.
    ///
    /// # Returns
    /// A new `PathNode` with empty children and operations collections.
    fn new() -> Self {
        Self {
            children: HashMap::new(),
            operations: HashMap::new(),
        }
    }
}

/// Main traverser for OpenAPI specifications.
///
/// Provides functionality to parse, validate, and navigate OpenAPI specification
/// documents with caching support for resolved references and operations.
pub struct OpenApiTraverser {
    specification: Value,
    resolved_references: DashMap<String, Arc<Value>>,
    resolved_operations: DashMap<(String, String), Arc<Operation>>,
    path_router: PathNode,
}

impl OpenApiTraverser {
    /// Creates a new OpenAPI traverser from a specification document.
    ///
    /// # Parameters
    /// - `specification`: The complete OpenAPI specification as a JSON Value
    ///
    /// # Returns
    /// A new `OpenApiTraverser` instance ready for navigation operations.
    ///
    /// # Examples
    /// ```rust
    /// use serde_json::json;
    /// use oasert::traverser::OpenApiTraverser;
    ///
    /// let spec = json!({
    ///     "openapi": "3.0.0",
    ///     "info": {"title": "API", "version": "1.0.0"},
    ///     "paths": {
    ///         "/users": {
    ///             "get": {"summary": "Get users"}
    ///         }
    ///     }
    /// });
    ///
    /// let traverser = OpenApiTraverser::new(spec)?;
    /// ```
    ///
    /// # Behavior
    /// This constructor automatically crawls all paths in the specification to build
    /// an internal routing tree for efficient operation lookup.
    pub fn new<'a>(specification: Value) -> Result<Self, TraverserError<'a>> {
        let mut traverser = Self {
            specification,
            resolved_references: DashMap::new(),
            resolved_operations: DashMap::new(),
            path_router: PathNode::new(),
        };
        match traverser.crawl_paths() {
            Ok(_) => {}
            Err(_) => todo!(),
        };
        Ok(traverser)
    }

    /// Crawls all paths in the specification to build the internal routing tree.
    ///
    /// # Returns
    /// `Ok(())` on successful crawling, or a `TraverserError` if parsing fails.
    fn crawl_paths(&mut self) -> TraverseResult {
        let spec_paths = Self::get_as_object(&self.specification, PATHS_FIELD)?;

        for (spec_path, spec_methods) in spec_paths {
            let operations = Self::require_object(spec_methods)?;
            for (spec_method, spec_operation) in operations {
                let spec_path_segments = Self::split_path_segments(spec_path);

                // Start at the root of the router
                let mut current_node = &mut self.path_router;

                // Build the path tree
                for segment in spec_path_segments {
                    let resolved_segment = if Self::is_parameter_segment(segment) {
                        let operation_params =
                            Self::get_as_array(spec_operation, PARAMETERS_FIELD)?;
                        let param_name = Self::extract_parameter_name(segment);

                        let param_schema = operation_params.iter().find(|param| {
                            if let Ok(name) = Self::get_as_str(param, NAME_FIELD) {
                                return name == param_name;
                            }
                            false
                        });
                        let param_schema = match param_schema {
                            None => continue,
                            Some(found) => match found.get(SCHEMA_FIELD) {
                                None => continue,
                                Some(schema) => schema,
                            },
                        };
                        let param_schema = Arc::new(param_schema.clone());
                        PathSegment::Parameter {
                            name: param_name.to_string(),
                            schema: param_schema,
                        }
                    } else {
                        PathSegment::Static(segment.to_string())
                    };
                    current_node = current_node
                        .children
                        .entry(resolved_segment)
                        .or_insert_with(PathNode::new);
                }

                // Store the operation at this path node
                let mut json_path = JsonPath::new();
                json_path.add(PATHS_FIELD).add(spec_path).add(spec_method);
                let operation = Arc::new(Operation {
                    data: spec_operation.clone(),
                    path: json_path,
                });
                current_node
                    .operations
                    .insert(spec_method.to_string(), operation);
            }
        }
        Ok(())
    }

    /// Filter function for removing empty path segments.
    const EMPTY_SEGMENT_FILTER: fn(&&str) -> bool = |s| !s.is_empty();

    /// Splits a path string into individual segments.
    ///
    /// # Parameters
    /// - `path`: The path string to split (e.g., "/users/{id}/posts")
    ///
    /// # Returns
    /// A vector of path segments with empty segments filtered out.
    fn split_path_segments(path: &str) -> Vec<&str> {
        path.split(PATH_SEPARATOR)
            .filter(Self::EMPTY_SEGMENT_FILTER)
            .collect()
    }

    /// Resolves a value using caching to avoid redundant computations.
    ///
    /// # Parameters
    /// - `cache`: The cache to use for storing/retrieving values
    /// - `key`: The key to cache the result under
    /// - `resolver`: Function to compute the value if not cached
    ///
    /// # Returns
    /// The resolved value wrapped in Arc, either from cache or newly computed.
    fn resolve_with_cache<'a, K, V, F>(
        cache: &DashMap<K, Arc<V>>,
        key: K,
        resolver: F,
    ) -> Result<Arc<V>, TraverserError<'a>>
    where
        K: Eq + Hash,
        F: FnOnce() -> Result<Arc<V>, TraverserError<'a>>,
    {
        let entry = cache.entry(key);
        match entry {
            Entry::Occupied(e) => Ok(e.get().clone()),
            Entry::Vacant(e) => {
                let result = resolver()?;
                e.insert(result.clone());
                Ok(result)
            }
        }
    }

    /// Checks if a path segment is a parameter placeholder.
    ///
    /// # Parameters
    /// - `segment`: The path segment to check
    ///
    /// # Returns
    /// `true` if the segment is surrounded by curly braces, `false` otherwise.
    fn is_parameter_segment(segment: &str) -> bool {
        segment.starts_with('{') && segment.ends_with('}')
    }

    /// Extracts the parameter name from a parameter segment.
    ///
    /// # Parameters
    /// - `segment`: The parameter segment (e.g., "{id}")
    ///
    /// # Returns
    /// The parameter name without the curly braces (e.g., "id").
    fn extract_parameter_name(segment: &str) -> &str {
        &segment[1..segment.len() - 1]
    }

    /// Returns a reference to the underlying OpenAPI specification.
    ///
    /// # Returns
    /// A reference to the complete specification JSON value.
    pub fn specification(&self) -> &Value {
        &self.specification
    }

    /// Finds and returns an operation matching the given path and HTTP method.
    ///
    /// # Parameters
    /// - `request_path`: The API path to match (e.g., "/users/123/posts")
    /// - `request_method`: The HTTP method (case-insensitive, e.g., "GET", "post")
    ///
    /// # Returns
    /// An `Arc<Operation>` containing the matching operation definition.
    ///
    /// # Examples
    /// ```rust
    /// use serde_json::json;
    /// use oasert::traverser::OpenApiTraverser;
    ///
    /// let spec = json!({
    ///     "openapi": "3.0.0",
    ///     "info": {"title": "API", "version": "1.0.0"},
    ///     "paths": {
    ///         "/users/{id}": {
    ///             "get": {
    ///                 "summary": "Get user by ID",
    ///                 "parameters": [
    ///                     {
    ///                         "name": "id",
    ///                         "in": "path",
    ///                         "required": true,
    ///                         "schema": {
    ///                             "type": "integer"
    ///                         }
    ///                     }
    ///                 ]
    ///             }
    ///         }
    ///     }
    /// });
    /// let traverser = OpenApiTraverser::new(spec)?;
    /// let operation = traverser.get_operation_from_path_and_method("/users/123", "GET")?;
    /// println!("Found operation: {:?}", operation);
    /// ```
    ///
    /// # Behavior
    /// This method performs path matching with parameter validation, including type
    /// conversion and schema validation for path parameters. Results are cached for
    /// performance. Returns `PathNotFound` error if no matching operation exists.
    pub fn get_operation_from_path_and_method<'a>(
        &self,
        request_path: &'a str,
        request_method: &str,
    ) -> FindOperationResult<'a> {
        let binding = request_method.to_lowercase();
        let request_method = binding.as_str();
        let segments = Self::split_path_segments(request_path);

        Self::resolve_with_cache(
            &self.resolved_operations,
            (String::from(request_path), String::from(request_method)),
            || {
                let result =
                    self.find_matching_operation(&segments, 0, &self.path_router, request_method);
                match result {
                    Some(operation) => Ok(operation),
                    None => Err(TraverserError::path_not_found(request_path)),
                }
            },
        )
    }

    /// Recursively searches for a matching operation in the path tree.
    ///
    /// # Parameters
    /// - `segments`: Array of path segments to match
    /// - `current_index`: Current position in the path segments array
    /// - `current_node`: Current node in the path tree
    /// - `method`: HTTP method to find
    ///
    /// # Returns
    /// `Some(Arc<Operation>)` if a match is found, `None` otherwise.
    fn find_matching_operation(
        &self,
        segments: &[&str],
        current_index: usize,
        current_node: &PathNode,
        method: &str,
    ) -> Option<Arc<Operation>> {
        // If we've processed all segments, check for an operation matching the method
        if current_index >= segments.len() {
            current_node.operations.get(method).cloned()
        } else {
            let current_segment = segments[current_index];
            for (segment, child) in &current_node.children {
                match segment {
                    PathSegment::Static(s) if s == current_segment => {
                        return self.find_matching_operation(
                            segments,
                            current_index + 1,
                            child,
                            method,
                        );
                    }
                    PathSegment::Parameter { name, schema } => {
                        if let Ok(converted_value) =
                            OpenApiPrimitives::convert_string_to_schema_type(
                                schema,
                                current_segment,
                            )
                        {
                            if jsonschema::validate(schema, &converted_value).is_ok() {
                                return self.find_matching_operation(
                                    segments,
                                    current_index + 1,
                                    child,
                                    method,
                                );
                            }
                        }
                    }
                    _ => continue,
                }
            }
            None
        }
    }

    /// Retrieves an optional field from a JSON node, returning None if missing.
    ///
    /// # Parameters
    /// - `node`: The JSON node to search in
    /// - `field`: The field name to look for
    ///
    /// # Returns
    /// `Some(SearchResult)` if the field exists, `None` if missing, or an error for other issues.
    ///
    /// # Examples
    /// ```rust
    /// use oasert::traverser::OpenApiTraverser;
    ///
    /// let spec = serde_json::json!({});
    ///
    /// let traverser = OpenApiTraverser::new(spec)?;
    /// let node = &serde_json::json!({"optional_field": "value"});
    /// let result = traverser.get_optional(node, "optional_field")?;
    /// assert!(result.is_some());
    ///
    /// let missing = traverser.get_optional(node, "missing_field")?;
    /// assert!(missing.is_none());
    /// ```
    ///
    /// # Behavior
    /// This method resolves references automatically and distinguishes between
    /// missing fields (returns None) and other errors (propagates the error).
    pub fn get_optional<'node>(
        &'node self,
        node: &'node Value,
        field: &'node str,
    ) -> TraverseOptionalSearchResult<'node> {
        match self.get_required(node, field) {
            Ok(security) => Ok(Some(security)),
            Err(e) => match e {
                TraverserError::MissingField(_) => Ok(None),
                _ => Err(e),
            },
        }
    }

    /// Retrieves a required field from a JSON node, failing if missing.
    ///
    /// # Parameters
    /// - `node`: The JSON node to search in
    /// - `field`: The field name that must exist
    ///
    /// # Returns
    /// A `SearchResult` containing the field value, or an error if missing or invalid.
    ///
    /// # Examples
    /// ```rust
    /// use oasert::traverser::OpenApiTraverser;
    /// let spec = serde_json::json!({});
    /// let traverser = OpenApiTraverser::new(spec)?;
    /// let node = &serde_json::json!({"required_field": "value"});
    /// let result = traverser.get_required(node, "required_field")?;
    /// assert_eq!(result.value().as_str().unwrap(), "value");
    /// ```
    ///
    /// # Behavior
    /// Automatically resolves JSON references ($ref) before field lookup. Returns
    /// `MissingField` error if the field doesn't exist after reference resolution.
    pub fn get_required<'node>(
        &'node self,
        node: &'node Value,
        field: &'node str,
    ) -> TraverseSearchResult<'node> {
        let ref_result = self.resolve_possible_ref(node)?;
        match ref_result {
            SearchResult::Arc(val) => match val.get(field) {
                None => Err(TraverserError::missing_field(field)),
                Some(v) => Ok(SearchResult::Arc(Arc::new(v.clone()))),
            },
            SearchResult::Ref(val) => match val.get(field) {
                None => Err(TraverserError::missing_field(field)),
                Some(v) => Ok(SearchResult::Ref(v)),
            },
        }
    }

    /// Resolves a JSON node that might contain a $ref reference.
    ///
    /// # Parameters
    /// - `node`: The JSON node to potentially resolve
    ///
    /// # Returns
    /// A `SearchResult` containing either the original node or the resolved reference.
    fn resolve_possible_ref<'node>(&'node self, node: &'node Value) -> TraverseSearchResult<'node> {
        if let Ok(ref_string) = Self::get_as_str(node, REF_FIELD) {
            let result = Self::resolve_with_cache(
                &self.resolved_references,
                String::from(ref_string),
                || {
                    let mut seen_references = HashSet::new();
                    let result = self.get_reference_path(ref_string, &mut seen_references)?;
                    Ok(match result {
                        SearchResult::Arc(val) => val,
                        SearchResult::Ref(val) => Arc::new(val.clone()),
                    })
                },
            )?;
            return Ok(SearchResult::Arc(result));
        }
        Ok(SearchResult::Ref(node))
    }

    /// Resolves a reference path with circular reference detection.
    ///
    /// # Parameters
    /// - `ref_string`: The reference string to resolve (e.g., "#/components/schemas/User")
    /// - `seen_references`: Set of already seen references for cycle detection
    ///
    /// # Returns
    /// The resolved JSON node or an error if the reference is invalid or circular.
    fn get_reference_path<'node, 'sub_node>(
        &'node self,
        ref_string: &'node str,
        seen_references: &mut HashSet<&'node str>,
    ) -> TraverseSearchResult<'sub_node>
    where
        'node: 'sub_node,
    {
        if seen_references.contains(ref_string) {
            return Err(TraverserError::cyclic_reference(ref_string));
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
            None => return Err(TraverserError::missing_field(ref_string)),
            Some(v) => self.resolve_possible_ref(v)?,
        };
        Ok(current_schema)
    }

    /// Generic helper for extracting typed values from JSON nodes.
    ///
    /// # Parameters
    /// - `node`: The JSON node containing the field
    /// - `field`: The field name to extract
    /// - `converter`: Function to convert the raw Value to the desired type
    ///
    /// # Returns
    /// A reference to the converted type or a TraverserError.
    fn get_as_type<'n, 's, T, F>(
        node: &'n Value,
        field: &'n str,
        converter: F,
    ) -> TraverseTypeResult<'s, T>
    where
        'n: 's,
        T: ?Sized,
        F: Fn(&'n Value) -> TraverseTypeResult<'s, T>,
    {
        match node.get(field) {
            None => Err(TraverserError::missing_field(field)),
            Some(found) => converter(found),
        }
    }

    /// Generic helper for requiring specific types with proper error messages.
    ///
    /// # Parameters
    /// - `node`: The JSON node to convert
    /// - `converter`: Function that attempts the type conversion
    /// - `type_name`: Name of the expected type for error reporting
    ///
    /// # Returns
    /// The converted value or a type mismatch error.
    fn require_type<'n, 's, T, F>(
        node: &'n Value,
        converter: F,
        type_name: &'static str,
    ) -> Result<T, TraverserError<'s>>
    where
        'n: 's,
        F: Fn(&'n Value) -> Option<T>,
    {
        converter(node).ok_or(TraverserError::type_mismatch(
            type_name,
            format!("{}", node),
        ))
    }

    /// Extracts a string field from a JSON node.
    ///
    /// # Parameters
    /// - `node`: The JSON node containing the field
    /// - `field`: The field name to extract as a string
    ///
    /// # Returns
    /// A reference to the string value or a TraverserError.
    pub(crate) fn get_as_str<'n, 's>(node: &'n Value, field: &'n str) -> TraverseTypeResult<'s, str>
    where
        'n: 's,
    {
        Self::get_as_type(node, field, Self::require_str)
    }

    /// Extracts an object field from a JSON node.
    ///
    /// # Parameters
    /// - `node`: The JSON node containing the field
    /// - `field`: The field name to extract as an object
    ///
    /// # Returns
    /// A reference to the object (Map) or a TraverserError.
    pub(crate) fn get_as_object<'n, 's>(
        node: &'n Value,
        field: &'n str,
    ) -> TraverseTypeResult<'s, Map<String, Value>>
    where
        'n: 's,
    {
        Self::get_as_type(node, field, Self::require_object)
    }

    /// Extracts an array field from a JSON node.
    ///
    /// # Parameters
    /// - `node`: The JSON node containing the field
    /// - `field`: The field name to extract as an array
    ///
    /// # Returns
    /// A reference to the array (Vec) or a TraverserError.
    pub(crate) fn get_as_array<'n, 's>(
        node: &'n Value,
        field: &'n str,
    ) -> TraverseTypeResult<'s, Vec<Value>>
    where
        'n: 's,
    {
        Self::get_as_type(node, field, Self::require_array)
    }

    /// Requires a JSON value to be a boolean.
    ///
    /// # Parameters
    /// - `node`: The JSON value to convert
    ///
    /// # Returns
    /// The boolean value or a type mismatch error.
    pub(crate) fn require_bool<'n, 's>(node: &'n Value) -> Result<bool, TraverserError<'s>>
    where
        'n: 's,
    {
        Self::require_type(node, Value::as_bool, "bool")
    }

    /// Requires a JSON value to be a string.
    ///
    /// # Parameters
    /// - `node`: The JSON value to convert
    ///
    /// # Returns
    /// A reference to the string or a type mismatch error.
    pub(crate) fn require_str<'n, 's>(node: &'n Value) -> TraverseTypeResult<'s, str>
    where
        'n: 's,
    {
        Self::require_type(node, Value::as_str, "string")
    }

    /// Requires a JSON value to be an object.
    ///
    /// # Parameters
    /// - `node`: The JSON value to convert
    ///
    /// # Returns
    /// A reference to the object map or a type mismatch error.
    pub(crate) fn require_object<'n, 's>(
        node: &'n Value,
    ) -> TraverseTypeResult<'s, Map<String, Value>>
    where
        'n: 's,
    {
        Self::require_type(node, Value::as_object, "object")
    }

    /// Requires a JSON value to be an array.
    ///
    /// # Parameters
    /// - `node`: The JSON value to convert
    ///
    /// # Returns
    /// A reference to the array vector or a type mismatch error.
    pub(crate) fn require_array<'n, 's>(node: &'n Value) -> TraverseTypeResult<'s, Vec<Value>>
    where
        'n: 's,
    {
        Self::require_type(node, Value::as_array, "array")
    }
}

#[cfg(test)]
mod tests {
    use crate::traverser::{OpenApiTraverser, TraverserError};
    use serde_json::json;
    use std::sync::Arc;

    #[test]
    fn test_get_operation_from_path_and_method_with_valid_path_and_method() {
        let spec = json!({
            "paths": {
                "/pets": {
                    "get": {
                        "operationId": "getPets",
                        "responses": {
                            "200": {
                                "description": "successful operation"
                            }
                        }
                    }
                }
            }
        });
        let traverser = OpenApiTraverser::new(spec).unwrap();
        let result = traverser.get_operation_from_path_and_method("/pets", "GET");
        assert!(result.is_ok());
        let operation = result.unwrap();
        assert_eq!(operation.data["operationId"], "getPets");
    }

    #[test]
    fn test_get_operation_from_path_and_method_with_invalid_path() {
        let spec = json!({
            "paths": {
                "/pets": {
                    "get": {
                        "operationId": "getPets",
                        "responses": {
                            "200": {
                                "description": "successful operation"
                            }
                        }
                    }
                }
            }
        });
        let traverser = OpenApiTraverser::new(spec).unwrap();
        let result = traverser.get_operation_from_path_and_method("/invalid_path", "GET");
        assert!(result.is_err());
    }

    #[test]
    fn test_get_operation_from_path_and_method_with_invalid_method() {
        let spec = json!({
            "paths": {
                "/pets": {
                    "get": {
                        "operationId": "getPets",
                        "responses": {
                            "200": {
                                "description": "successful operation"
                            }
                        }
                    }
                }
            }
        });

        let traverser = OpenApiTraverser::new(spec).unwrap();
        let result = traverser.get_operation_from_path_and_method("/pets", "POST");
        assert!(result.is_err());
    }

    #[test]
    fn test_get_operation_from_path_and_method_with_cached_result() {
        let spec = json!({
            "paths": {
                "/pets": {
                    "get": {
                        "operationId": "getPets",
                        "responses": {
                            "200": {
                                "description": "successful operation"
                            }
                        }
                    }
                }
            }
        });

        let traverser = OpenApiTraverser::new(spec).unwrap();

        // Call the function the first time (caches the result)
        let first_result = traverser.get_operation_from_path_and_method("/pets", "GET");
        assert!(first_result.is_ok());
        let second_result = traverser.get_operation_from_path_and_method("/pets", "GET");
        assert!(second_result.is_ok());
        let first_result = first_result.unwrap();
        let second_result = second_result.unwrap();
        assert!(Arc::ptr_eq(&first_result, &second_result));
    }

    #[test]
    fn test_get_operation_from_path_and_method_with_parametrized_path() {
        let spec = json!({
            "paths": {
                "/pets/{id}": {
                    "get": {
                        "operationId": "getPetById",
                        "parameters": [
                            {
                                "name": "id",
                                "in": "path",
                                "required": true,
                                "schema": { "type": "string" }
                            }
                        ],
                        "responses": {
                            "200": {
                                "description": "successful operation"
                            }
                        }
                    }
                }
            }
        });
        let traverser = OpenApiTraverser::new(spec).unwrap();
        let result = traverser.get_operation_from_path_and_method("/pets/123", "GET");
        assert!(result.is_ok());
        let operation = result.unwrap();
        assert_eq!(operation.data["operationId"], "getPetById");
    }

    #[test]
    fn test_get_operation_from_path_and_method_with_non_matching_param() {
        let spec = json!({
            "paths": {
                "/pets/{id}": {
                    "get": {
                        "operationId": "getPetById",
                        "parameters": [
                            {
                                "name": "id",
                                "in": "path",
                                "required": true,
                                "schema": { "type": "integer" }
                            }
                        ],
                        "responses": {
                            "200": {
                                "description": "successful operation"
                            }
                        }
                    }
                }
            }
        });
        let traverser = OpenApiTraverser::new(spec).unwrap();
        let result = traverser.get_operation_from_path_and_method("/pets/id_as_string", "GET");
        assert!(result.is_err());
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
            Err(TraverserError::TypeMismatch { expected, .. }) => {
                assert_eq!(expected, "object");
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
            Err(TraverserError::TypeMismatch { expected, .. }) => {
                assert_eq!(expected, "array");
            }
            _ => panic!("Expected UnexpectedType error"),
        }
    }
}
