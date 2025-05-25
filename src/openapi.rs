use std::collections::BTreeMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A complete OpenAPI specification supporting both 3.0.x and 3.1.x
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct OpenApiSpec {
    /// This string MUST be the semantic version number of the OpenAPI Specification version
    /// that the OpenAPI document uses. For OpenAPI 3.0.x this would be "3.0.X", and
    /// for 3.1.x it would be "3.1.X"
    pub openapi: String,

    /// Provides metadata about the API.
    pub info: Info,

    /// JSON Schema dialect used by the specification.
    /// Only available in OpenAPI 3.1.x.
    #[serde(skip_serializing_if = "Option::is_none", rename = "jsonSchemaDialect")]
    pub json_schema_dialect: Option<String>,

    /// Connectivity information to target servers.
    /// If not provided, defaults to a Server Object with a URL value of /.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub servers: Vec<Server>,

    /// The available paths and operations for the API.
    /// Required in 3.0.x, optional in 3.1.x.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paths: Option<BTreeMap<String, PathItem>>,

    /// Incoming webhooks that may be received as part of this API.
    /// Only available in OpenAPI 3.1.x.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub webhooks: BTreeMap<String, PathItem>,

    /// An element to hold various schemas for the specification.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub components: Option<Components>,

    /// A declaration of which security mechanisms can be used across the API.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub security: Vec<SecurityRequirement>,

    /// A list of tags used by the specification with additional metadata.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<Tag>,

    /// Additional external documentation.
    #[serde(skip_serializing_if = "Option::is_none", rename = "externalDocs")]
    pub external_docs: Option<ExternalDoc>,

    /// Specification extensions.
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

/// Metadata about the API
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Info {
    /// The title of the API.
    pub title: String,

    /// A short summary of the API.
    /// Only available in OpenAPI 3.1.x.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,

    /// A description of the API.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// A URL to the Terms of Service for the API.
    #[serde(skip_serializing_if = "Option::is_none", rename = "termsOfService")]
    pub terms_of_service: Option<String>,

    /// The contact information for the API.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contact: Option<Contact>,

    /// The license information for the API.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<License>,

    /// The version of the API.
    pub version: String,

    /// Specification extensions.
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

/// Contact information for the API.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Contact {
    /// The identifying name of the contact person/organization.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// The URL pointing to the contact information.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,

    /// The email address of the contact person/organization.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,

    /// Specification extensions.
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

/// License information for the API.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct License {
    /// The license name.
    pub name: String,

    /// License identifier.
    /// Only available in OpenAPI 3.1.x.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identifier: Option<String>,

    /// URL to the license.
    /// In 3.1.x, cannot be used with identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,

    /// Specification extensions.
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

/// Server information
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Server {
    /// A URL to the target host.
    pub url: String,

    /// An optional string describing the host.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// A map between variable names and their values.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variables: Option<BTreeMap<String, ServerVariable>>,

    /// Specification extensions.
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

/// Variable for server URL template substitution.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct ServerVariable {
    /// An enumeration of string values to be used if the substitution options are from a limited set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enum_: Option<Vec<String>>,

    /// The default value to use for substitution.
    pub default: String,

    /// An optional description for the server variable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Specification extensions.
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

/// A Schema represents a type definition, supporting both OpenAPI 3.0.x and 3.1.x formats
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Schema {
    /// Schema represented as a boolean (3.1.x feature)
    Boolean(bool),
    /// Schema represented as an object (both 3.0.x and 3.1.x)
    Object(SchemaObject),
}

/// Schema type that supports both single string (3.0.x) and array of types (3.1.x)
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum SchemaType {
    /// Single type string (3.0.x style)
    Single(String),
    /// Array of types (3.1.x style)
    Multiple(Vec<String>),
}

/// Schema object supporting both 3.0.x and 3.1.x features
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct SchemaObject {
    /// Title of the schema
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// Description of the schema
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Schema type - can be single string (3.0.x) or array of strings (3.1.x)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#type: Option<SchemaType>,

    /// Format of the schema
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,

    /// Whether the schema can be null (3.0.x feature)
    /// In 3.1.x, null should be included in the type array instead
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nullable: Option<bool>,

    /// Properties for object schemas
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<BTreeMap<String, Schema>>,

    /// Required properties for object schemas
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required: Vec<String>,

    /// Items for array schemas
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Box<Schema>>,

    /// Additional properties for object schemas
    #[serde(skip_serializing_if = "Option::is_none", rename = "additionalProperties")]
    pub additional_properties: Option<Box<Schema>>,

    /// Minimum value for numeric schemas
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minimum: Option<f64>,

    /// Maximum value for numeric schemas
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maximum: Option<f64>,

    /// Exclusive minimum flag (3.0.x) or value (3.1.x)
    #[serde(skip_serializing_if = "Option::is_none", rename = "exclusiveMinimum")]
    pub exclusive_minimum: Option<Value>,

    /// Exclusive maximum flag (3.0.x) or value (3.1.x)
    #[serde(skip_serializing_if = "Option::is_none", rename = "exclusiveMaximum")]
    pub exclusive_maximum: Option<Value>,

    /// Pattern for string schemas
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,

    /// Enum values
    #[serde(skip_serializing_if = "Option::is_none", rename = "enum")]
    pub enum_values: Option<Vec<Value>>,

    /// All of schemas
    #[serde(skip_serializing_if = "Option::is_none", rename = "allOf")]
    pub all_of: Option<Vec<Schema>>,

    /// Any of schemas
    #[serde(skip_serializing_if = "Option::is_none", rename = "anyOf")]
    pub any_of: Option<Vec<Schema>>,

    /// One of schemas
    #[serde(skip_serializing_if = "Option::is_none", rename = "oneOf")]
    pub one_of: Option<Vec<Schema>>,

    /// Not schema
    #[serde(skip_serializing_if = "Option::is_none")]
    pub not: Option<Box<Schema>>,

    /// Schema reference ($ref)
    #[serde(skip_serializing_if = "Option::is_none", rename = "$ref")]
    pub ref_path: Option<String>,

    /// Schema dynamic reference ($dynamicRef) - 3.1.x feature
    #[serde(skip_serializing_if = "Option::is_none", rename = "$dynamicRef")]
    pub dynamic_ref: Option<String>,

    /// Schema anchor ($anchor) - 3.1.x feature
    #[serde(skip_serializing_if = "Option::is_none", rename = "$anchor")]
    pub anchor: Option<String>,

    /// Specification extensions (x- prefixed fields)
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

/// Components Object
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Components {
    /// An object to hold reusable Schema Objects.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub schemas: BTreeMap<String, Schema>,

    /// An object to hold reusable Response Objects.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub responses: BTreeMap<String, ReferenceOr<Response>>,

    /// An object to hold reusable Parameter Objects.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub parameters: BTreeMap<String, ReferenceOr<Parameter>>,

    /// An object to hold reusable Example Objects.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub examples: BTreeMap<String, ReferenceOr<Example>>,

    /// An object to hold reusable Request Body Objects.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", rename = "requestBodies")]
    pub request_bodies: BTreeMap<String, ReferenceOr<RequestBody>>,

    /// An object to hold reusable Header Objects.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, ReferenceOr<Header>>,

    /// An object to hold reusable Security Scheme Objects.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", rename = "securitySchemes")]
    pub security_schemes: BTreeMap<String, ReferenceOr<SecurityScheme>>,

    /// An object to hold reusable Link Objects.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub links: BTreeMap<String, ReferenceOr<Link>>,

    /// An object to hold reusable Callback Objects.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub callbacks: BTreeMap<String, ReferenceOr<Callback>>,

    /// An object to hold reusable Path Item Objects.
    /// Only available in OpenAPI 3.1.x.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", rename = "pathItems")]
    pub path_items: BTreeMap<String, ReferenceOr<PathItem>>,

    /// Specification extensions.
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

/// Reference Object or another type T
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum ReferenceOr<T> {
    /// Reference to another component
    Reference(Reference),
    /// Inline component
    Item(T),
}

/// Reference Object
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Reference {
    /// The reference string.
    #[serde(rename = "$ref")]
    pub ref_path: String,

    /// A brief summary of the referenced target.
    /// Only available in OpenAPI 3.1.x.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,

    /// A description of the referenced target.
    /// Only available in OpenAPI 3.1.x.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Path Item Object
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct PathItem {
    /// Allows for a referenced definition of this path item.
    #[serde(skip_serializing_if = "Option::is_none", rename = "$ref")]
    pub ref_path: Option<String>,

    /// An optional, string summary, intended to apply to all operations in this path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,

    /// An optional, string description, intended to apply to all operations in this path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// A definition of a GET operation on this path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub get: Option<Operation>,

    /// A definition of a PUT operation on this path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub put: Option<Operation>,

    /// A definition of a POST operation on this path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post: Option<Operation>,

    /// A definition of a DELETE operation on this path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delete: Option<Operation>,

    /// A definition of a OPTIONS operation on this path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Operation>,

    /// A definition of a HEAD operation on this path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub head: Option<Operation>,

    /// A definition of a PATCH operation on this path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch: Option<Operation>,

    /// A definition of a TRACE operation on this path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace: Option<Operation>,

    /// An alternative server array to service all operations in this path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub servers: Option<Vec<Server>>,

    /// A list of parameters that are applicable for all the operations described under this path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Vec<ReferenceOr<Parameter>>>,

    /// Specification extensions.
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

/// Operation Object
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Operation {
    /// A list of tags for API documentation control.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,

    /// A short summary of what the operation does.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,

    /// A verbose explanation of the operation behavior.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Additional external documentation for this operation.
    #[serde(skip_serializing_if = "Option::is_none", rename = "externalDocs")]
    pub external_docs: Option<ExternalDoc>,

    /// Unique string used to identify the operation.
    #[serde(skip_serializing_if = "Option::is_none", rename = "operationId")]
    pub operation_id: Option<String>,

    /// A list of parameters that are applicable for this operation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Vec<ReferenceOr<Parameter>>>,

    /// The request body applicable for this operation.
    #[serde(skip_serializing_if = "Option::is_none", rename = "requestBody")]
    pub request_body: Option<ReferenceOr<RequestBody>>,

    /// The list of possible responses as they are returned from executing this operation.
    pub responses: Responses,

    /// A map of possible out-of-band callbacks related to the parent operation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub callbacks: Option<BTreeMap<String, ReferenceOr<Callback>>>,

    /// Declares this operation to be deprecated.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<bool>,

    /// A declaration of which security mechanisms can be used for this operation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub security: Option<Vec<SecurityRequirement>>,

    /// An alternative server array to service this operation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub servers: Option<Vec<Server>>,

    /// Specification extensions.
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

/// External Documentation Object
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct ExternalDoc {
    /// A description of the target documentation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// The URL for the target documentation.
    pub url: String,

    /// Specification extensions.
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

/// Parameter Object
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Parameter {
    /// The name of the parameter.
    pub name: String,

    /// The location of the parameter.
    pub r#in: ParameterLocation,

    /// A brief description of the parameter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Determines whether this parameter is mandatory.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,

    /// Specifies that a parameter is deprecated.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<bool>,

    /// Sets the ability to pass empty-valued parameters.
    /// This is valid only for query parameters and allows sending a parameter with an empty value.
    #[serde(skip_serializing_if = "Option::is_none", rename = "allowEmptyValue")]
    pub allow_empty_value: Option<bool>,

    /// Describes how the parameter value will be serialized.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,

    /// When this is true, parameter values of type array or object generate separate parameters
    /// for each value of the array or key-value pair of the map.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explode: Option<bool>,

    /// Determines whether the parameter value SHOULD allow reserved characters.
    #[serde(skip_serializing_if = "Option::is_none", rename = "allowReserved")]
    pub allow_reserved: Option<bool>,

    /// The schema defining the type used for the parameter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<Schema>,

    /// Example of the parameter's potential value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub example: Option<Value>,

    /// Examples of the parameter's potential value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub examples: Option<BTreeMap<String, ReferenceOr<Example>>>,

    /// A map containing the representations for the parameter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<BTreeMap<String, MediaType>>,

    /// Specification extensions.
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

/// Parameter Location
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ParameterLocation {
    /// Parameter in query string
    Query,
    /// Parameter in header
    Header,
    /// Parameter in path
    Path,
    /// Parameter in cookie
    Cookie,
}

/// Request Body Object
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct RequestBody {
    /// A brief description of the request body.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// The content of the request body.
    pub content: BTreeMap<String, MediaType>,

    /// Determines if the request body is required in the request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,

    /// Specification extensions.
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

/// Media Type Object
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct MediaType {
    /// The schema defining the content of the request, response, or parameter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<Schema>,

    /// Example of the media type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub example: Option<Value>,

    /// Examples of the media type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub examples: Option<BTreeMap<String, ReferenceOr<Example>>>,

    /// A map between a property name and its encoding information.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoding: Option<BTreeMap<String, Encoding>>,

    /// Specification extensions.
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

/// Encoding Object
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Encoding {
    /// The Content-Type for encoding a specific property.
    #[serde(skip_serializing_if = "Option::is_none", rename = "contentType")]
    pub content_type: Option<String>,

    /// A map allowing additional information to be provided as headers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<BTreeMap<String, ReferenceOr<Header>>>,

    /// Describes how a specific property value will be serialized.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,

    /// When this is true, property values of type array or object generate separate parameters
    /// for each value of the array, or key-value-pair of the map.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explode: Option<bool>,

    /// Determines whether the parameter value SHOULD allow reserved characters.
    #[serde(skip_serializing_if = "Option::is_none", rename = "allowReserved")]
    pub allow_reserved: Option<bool>,

    /// Specification extensions.
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

/// Responses Object
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Responses {
    /// The documentation of responses other than the ones declared for specific HTTP response codes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<ReferenceOr<Response>>,

    /// Any HTTP status code can be used as the property name, but only one property per code.
    #[serde(flatten)]
    pub responses: BTreeMap<String, ReferenceOr<Response>>,

    /// Specification extensions.
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

/// Response Object
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Response {
    /// A description of the response.
    pub description: String,

    /// Maps a header name to its definition.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<BTreeMap<String, ReferenceOr<Header>>>,

    /// A map containing descriptions of potential response payloads.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<BTreeMap<String, MediaType>>,

    /// A map of operations links that can be followed from the response.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub links: Option<BTreeMap<String, ReferenceOr<Link>>>,

    /// Specification extensions.
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

/// Header Object
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Header {
    /// A brief description of the header.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Determines whether this parameter is mandatory.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,

    /// Specifies that a header is deprecated.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<bool>,

    /// Describes how the parameter value will be serialized.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,

    /// When this is true, parameter values of type array or object generate separate parameters
    /// for each value of the array or key-value pair of the map.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explode: Option<bool>,

    /// The schema defining the type used for the header.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<Schema>,

    /// Example of the header's potential value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub example: Option<Value>,

    /// Examples of the header's potential value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub examples: Option<BTreeMap<String, ReferenceOr<Example>>>,

    /// A map containing the representations for the parameter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<BTreeMap<String, MediaType>>,

    /// Specification extensions.
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

/// Example Object
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Example {
    /// Short description for the example.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,

    /// Long description for the example.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Embedded literal example.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,

    /// A URL that points to the literal example.
    #[serde(skip_serializing_if = "Option::is_none", rename = "externalValue")]
    pub external_value: Option<String>,

    /// Specification extensions.
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

/// Link Object
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Link {
    /// A relative or absolute URI reference to an OAS operation.
    #[serde(skip_serializing_if = "Option::is_none", rename = "operationRef")]
    pub operation_ref: Option<String>,

    /// The name of an existing, resolvable OAS operation.
    #[serde(skip_serializing_if = "Option::is_none", rename = "operationId")]
    pub operation_id: Option<String>,

    /// A map representing parameters to pass to an operation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<BTreeMap<String, Value>>,

    /// A literal value or expression to use as a request body when calling the target operation.
    #[serde(skip_serializing_if = "Option::is_none", rename = "requestBody")]
    pub request_body: Option<Value>,

    /// A description of the link.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// A server object to be used by the target operation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server: Option<Server>,

    /// Specification extensions.
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

/// Callback Object
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Callback {
    /// A Path Item Object used to define a callback request and expected responses.
    #[serde(flatten)]
    pub callback_paths: BTreeMap<String, PathItem>,

    /// Specification extensions.
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

/// Security Scheme Object
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct SecurityScheme {
    /// The type of the security scheme.
    pub r#type: SecuritySchemeType,

    /// A description for security scheme.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// The name of the header, query, or cookie parameter to be used.
    /// Only for apiKey type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// The location of the API key.
    /// Only for apiKey type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#in: Option<ApiKeyLocation>,

    /// The name of the HTTP Authorization scheme to be used.
    /// Only for http type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheme: Option<String>,

    /// A hint to the client to identify how the bearer token is formatted.
    /// Only for http ("bearer") type.
    #[serde(skip_serializing_if = "Option::is_none", rename = "bearerFormat")]
    pub bearer_format: Option<String>,

    /// An object containing configuration information for the flow types supported.
    /// Only for oauth2 type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flows: Option<OAuthFlows>,

    /// OpenId Connect URL to discover OAuth2 configuration values.
    /// Only for openIdConnect type.
    #[serde(skip_serializing_if = "Option::is_none", rename = "openIdConnectUrl")]
    pub open_id_connect_url: Option<String>,

    /// Specification extensions.
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

/// Security Scheme Type
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SecuritySchemeType {
    /// API key
    ApiKey,
    /// HTTP authentication
    Http,
    /// OAuth2
    OAuth2,
    /// OpenID Connect
    OpenIdConnect,
    /// Mutual TLS (3.1.x feature)
    MutualTLS,
}

/// API Key Location
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ApiKeyLocation {
    /// In query
    Query,
    /// In header
    Header,
    /// In cookie
    Cookie,
}

/// OAuth Flows Object
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct OAuthFlows {
    /// Configuration for the OAuth Implicit flow.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub implicit: Option<OAuthFlow>,

    /// Configuration for the OAuth Resource Owner Password flow.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<OAuthFlow>,

    /// Configuration for the OAuth Client Credentials flow.
    #[serde(skip_serializing_if = "Option::is_none", rename = "clientCredentials")]
    pub client_credentials: Option<OAuthFlow>,

    /// Configuration for the OAuth Authorization Code flow.
    #[serde(skip_serializing_if = "Option::is_none", rename = "authorizationCode")]
    pub authorization_code: Option<OAuthFlow>,

    /// Specification extensions.
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

/// OAuth Flow Object
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct OAuthFlow {
    /// The authorization URL to be used for this flow.
    /// Only for implicit and authorizationCode flows.
    #[serde(skip_serializing_if = "Option::is_none", rename = "authorizationUrl")]
    pub authorization_url: Option<String>,

    /// The token URL to be used for this flow.
    /// Only for password, clientCredentials, and authorizationCode flows.
    #[serde(skip_serializing_if = "Option::is_none", rename = "tokenUrl")]
    pub token_url: Option<String>,

    /// The URL to be used for obtaining refresh tokens.
    #[serde(skip_serializing_if = "Option::is_none", rename = "refreshUrl")]
    pub refresh_url: Option<String>,

    /// The available scopes for the OAuth2 security scheme.
    pub scopes: BTreeMap<String, String>,

    /// Specification extensions.
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

/// Security Requirement Object
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct SecurityRequirement(pub BTreeMap<String, Vec<String>>);

/// Tag Object
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Tag {
    /// The name of the tag.
    pub name: String,

    /// A description for the tag.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Additional external documentation for this tag.
    #[serde(skip_serializing_if = "Option::is_none", rename = "externalDocs")]
    pub external_docs: Option<ExternalDoc>,

    /// Specification extensions.
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

impl OpenApiSpec {
    /// Validates if the spec version is supported (either 3.0.x or 3.1.x)
    pub fn validate_version(&self) -> Result<semver::Version, String> {
        let spec_version = &self.openapi;
        match semver::Version::parse(spec_version) {
            Ok(sem_ver) => {
                // Check if version is 3.0.x or 3.1.x
                if (sem_ver.major == 3 && sem_ver.minor == 0) ||
                    (sem_ver.major == 3 && sem_ver.minor == 1) {
                    Ok(sem_ver)
                } else {
                    Err(format!("Unsupported OpenAPI version: {}", spec_version))
                }
            },
            Err(e) => Err(format!("Invalid version format: {}", e)),
        }
    }

    /// Determines if this spec is OpenAPI 3.1.x
    pub fn is_3_1(&self) -> bool {
        if let Ok(version) = semver::Version::parse(&self.openapi) {
            version.major == 3 && version.minor == 1
        } else {
            false
        }
    }

    /// Determines if this spec is OpenAPI 3.0.x
    pub fn is_3_0(&self) -> bool {
        if let Ok(version) = semver::Version::parse(&self.openapi) {
            version.major == 3 && version.minor == 0
        } else {
            false
        }
    }
}




