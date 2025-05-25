use std::collections::HashMap;
use serde::{Serialize, Deserialize};
use serde_json::Value;

/// OpenAPI 3.1.x Document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenApiDocument {
    /// The semantic version number of the OpenAPI Specification version
    pub openapi: String,
    /// Metadata about the API
    pub info: Info,
    /// The default value is the OAS dialect schema
    #[serde(skip_serializing_if = "Option::is_none")]
    pub json_schema_dialect: Option<String>,
    /// The available paths and operations for the API
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paths: Option<Paths>,
    /// The incoming webhooks that may be received as part of this API
    /// Additional external documentation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_docs: Option<ExternalDocumentation>,
    /// An array of Server Objects, which provide connectivity information to a target server
    #[serde(skip_serializing_if = "Option::is_none")]
    pub servers: Option<Vec<Server>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub webhooks: Option<HashMap<String, PathItem>>,
    /// An element to hold various schemas for the document
    #[serde(skip_serializing_if = "Option::is_none")]
    pub components: Option<Components>,
    /// A declaration of which security mechanisms can be used across the API
    #[serde(skip_serializing_if = "Option::is_none")]
    pub security: Option<Vec<SecurityRequirement>>,
    /// A list of tags used by the document with additional metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<Tag>>,
    /// Extension fields
    #[serde(flatten)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    #[serde(with = "crate::openapi_common::extensions")]
    pub extensions: HashMap<String, Value>,
}

/// Info Object: provides metadata about the API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Info {
    /// The title of the API
    pub title: String,
    /// The version of the API
    pub version: String,
    /// A short summary of the API
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// A description of the API
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// A URL to the Terms of Service for the API
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terms_of_service: Option<String>,
    /// Contact information for the exposed API
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contact: Option<Contact>,
    /// License information for the exposed API
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<License>,
    /// Extension fields
    #[serde(flatten)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    #[serde(with = "crate::openapi_common::extensions")]
    pub extensions: HashMap<String, Value>,
}

/// Contact Object: contact information for the exposed API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contact {
    /// The identifying name of the contact person/organization
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The URL pointing to the contact information
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// The email address of the contact person/organization
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    /// Extension fields
    #[serde(flatten)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    #[serde(with = "crate::openapi_common::extensions")]
    pub extensions: HashMap<String, Value>,
}

/// License Object: license information for the exposed API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct License {
    /// The license name used for the API
    pub name: String,
    /// An SPDX license expression for the API
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identifier: Option<String>,
    /// A URL to the license used for the API
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Extension fields
    #[serde(flatten)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    #[serde(with = "crate::openapi_common::extensions")]
    pub extensions: HashMap<String, Value>,
}

/// Server Object: object representing a server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Server {
    /// A URL to the target host
    pub url: String,
    /// An optional string describing the host
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// A map between variable names and their values
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variables: Option<HashMap<String, ServerVariable>>,
    /// Extension fields
    #[serde(flatten)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    #[serde(with = "crate::openapi_common::extensions")]
    pub extensions: HashMap<String, Value>,
}

/// Server Variable Object: a server variable for server URL template substitution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerVariable {
    /// The default value to use for substitution
    pub default: String,
    /// An enumeration of string values to be used if the substitution options are from a limited set
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enum_values: Option<Vec<String>>,
    /// A description for the server variable
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Extension fields
    #[serde(flatten)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    #[serde(with = "crate::openapi_common::extensions")]
    pub extensions: HashMap<String, Value>,
}

/// Components Object: holds a set of reusable objects
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Components {
    /// An object to hold reusable Schema Objects
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schemas: Option<HashMap<String, Schema>>,
    /// An object to hold reusable Response Objects
    #[serde(skip_serializing_if = "Option::is_none")]
    pub responses: Option<HashMap<String, ReferenceOr<Response>>>,
    /// An object to hold reusable Parameter Objects
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<HashMap<String, ReferenceOr<Parameter>>>,
    /// An object to hold reusable Example Objects
    #[serde(skip_serializing_if = "Option::is_none")]
    pub examples: Option<HashMap<String, ReferenceOr<Example>>>,
    /// An object to hold reusable Request Body Objects
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_bodies: Option<HashMap<String, ReferenceOr<RequestBody>>>,
    /// An object to hold reusable Header Objects
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, ReferenceOr<Header>>>,
    /// An object to hold reusable Security Scheme Objects
    #[serde(skip_serializing_if = "Option::is_none")]
    pub security_schemes: Option<HashMap<String, ReferenceOr<SecurityScheme>>>,
    /// An object to hold reusable Link Objects
    #[serde(skip_serializing_if = "Option::is_none")]
    pub links: Option<HashMap<String, ReferenceOr<Link>>>,
    /// An object to hold reusable Callback Objects
    #[serde(skip_serializing_if = "Option::is_none")]
    pub callbacks: Option<HashMap<String, ReferenceOr<Callbacks>>>,
    /// An object to hold reusable Path Item Objects
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path_items: Option<HashMap<String, PathItem>>,
    /// Extension fields
    #[serde(flatten)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    #[serde(with = "crate::openapi_common::extensions")]
    pub extensions: HashMap<String, Value>,
}

/// Paths Object: holds the relative paths to the individual endpoints and their operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Paths {
    /// Extension fields
    #[serde(flatten)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    #[serde(with = "crate::openapi_common::extensions")]
    pub extensions: HashMap<String, Value>,

    /// Path items
    #[serde(flatten)]
    pub paths: HashMap<String, PathItem>,
}

/// Path Item Object: describes the operations available on a single path
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathItem {
    /// Reference to a Path Item Object in the components section
    #[serde(rename = "$ref", skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
    /// A summary for the path
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// A description for the path
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// A definition of a GET operation on this path
    #[serde(skip_serializing_if = "Option::is_none")]
    pub get: Option<Operation>,
    /// A definition of a PUT operation on this path
    #[serde(skip_serializing_if = "Option::is_none")]
    pub put: Option<Operation>,
    /// A definition of a POST operation on this path
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post: Option<Operation>,
    /// A definition of a DELETE operation on this path
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delete: Option<Operation>,
    /// A definition of a OPTIONS operation on this path
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Operation>,
    /// A definition of a HEAD operation on this path
    #[serde(skip_serializing_if = "Option::is_none")]
    pub head: Option<Operation>,
    /// A definition of a PATCH operation on this path
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch: Option<Operation>,
    /// A definition of a TRACE operation on this path
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace: Option<Operation>,
    /// An alternative server array to service all operations in this path
    #[serde(skip_serializing_if = "Option::is_none")]
    pub servers: Option<Vec<Server>>,
    /// A list of parameters that are applicable for all operations
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Vec<ReferenceOr<Parameter>>>,
    /// Extension fields
    #[serde(flatten)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    #[serde(with = "crate::openapi_common::extensions")]
    pub extensions: HashMap<String, Value>,
}

/// Operation Object: describes a single API operation on a path
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Operation {
    /// A list of tags for API documentation control
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    /// A short summary of what the operation does
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// A verbose explanation of the operation behavior
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Additional external documentation for this operation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_docs: Option<ExternalDocumentation>,
    /// Unique string used to identify the operation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,
    /// A list of parameters that are applicable for this operation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Vec<ReferenceOr<Parameter>>>,
    /// The request body applicable for this operation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_body: Option<ReferenceOr<RequestBody>>,
    /// The list of possible responses as they are returned from executing this operation
    pub responses: Responses,
    /// A map of possible out-of band callbacks related to the parent operation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub callbacks: Option<HashMap<String, ReferenceOr<Callbacks>>>,
    /// Declares this operation to be deprecated
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<bool>,
    /// A declaration of which security mechanisms can be used for this operation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub security: Option<Vec<SecurityRequirement>>,
    /// An alternative server array to service this operation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub servers: Option<Vec<Server>>,
    /// Extension fields
    #[serde(flatten)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    #[serde(with = "crate::openapi_common::extensions")]
    pub extensions: HashMap<String, Value>,
}

/// External Documentation Object: allows referencing an external resource for extended documentation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalDocumentation {
    /// A description of the target documentation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The URL for the target documentation
    pub url: String,
    /// Extension fields
    #[serde(flatten)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    #[serde(with = "crate::openapi_common::extensions")]
    pub extensions: HashMap<String, Value>,
}

/// Parameter Object: describes a single operation parameter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Parameter {
    /// The name of the parameter
    pub name: String,
    /// The location of the parameter
    #[serde(rename = "in")]
    pub in_: ParameterLocation,
    /// A brief description of the parameter
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Determines whether this parameter is mandatory
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    /// Specifies that a parameter is deprecated
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<bool>,
    /// Allows sending a parameter by name only or with an empty value
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_empty_value: Option<bool>,
    /// The schema defining the type used for the parameter
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<Schema>,
    /// Example of the parameter's potential value
    #[serde(skip_serializing_if = "Option::is_none")]
    pub example: Option<Value>,
    /// Examples of the parameter's potential value
    #[serde(skip_serializing_if = "Option::is_none")]
    pub examples: Option<HashMap<String, ReferenceOr<Example>>>,
    /// A map containing descriptions of potential content
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<HashMap<String, MediaType>>,
    /// Describes how the parameter value will be serialized
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
    /// When this is true, parameter values of type array or object generate separate parameters
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explode: Option<bool>,
    /// Reserved characters in parameter values will not be percent-encoded
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_reserved: Option<bool>,
    /// Extension fields
    #[serde(flatten)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    #[serde(with = "crate::openapi_common::extensions")]
    pub extensions: HashMap<String, Value>,
}

/// Enum for parameter location
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ParameterLocation {
    Query,
    Header,
    Path,
    Cookie,
}

/// A type that can either be a Reference or a specific type T
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ReferenceOr<T> {
    Reference(Reference),
    Item(T),
}

/// Reference Object: a simple object to allow referencing other components
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reference {
    /// The reference string
    #[serde(rename = "$ref")]
    pub reference: String,
    /// A short summary which by default SHOULD override that of the referenced component
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// A description which by default SHOULD override that of the referenced component
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Request Body Object: describes a single request body
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestBody {
    /// A brief description of the request body
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The content of the request body
    pub content: HashMap<String, MediaType>,
    /// Determines if the request body is required in the request
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    /// Extension fields
    #[serde(flatten)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    #[serde(with = "crate::openapi_common::extensions")]
    pub extensions: HashMap<String, Value>,
}

/// Media Type Object: provides schema for the media type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaType {
    /// The schema defining the content of the request, response, or parameter
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<Schema>,
    /// Example of the media type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub example: Option<Value>,
    /// Examples of the media type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub examples: Option<HashMap<String, ReferenceOr<Example>>>,
    /// A map between a property name and its encoding information
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoding: Option<HashMap<String, Encoding>>,
    /// Extension fields
    #[serde(flatten)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    #[serde(with = "crate::openapi_common::extensions")]
    pub extensions: HashMap<String, Value>,
}

/// Encoding Object: a single encoding definition applied to a single schema property
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Encoding {
    /// The Content-Type for encoding a specific property
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    /// A map allowing additional information to be provided
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, ReferenceOr<Header>>>,
    /// Describes how a specific property value will be serialized
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
    /// When this is true, property values of type array or object generate separate parameters
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explode: Option<bool>,
    /// Reserved characters in parameter values will not be percent-encoded
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_reserved: Option<bool>,
    /// Extension fields
    #[serde(flatten)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    #[serde(with = "crate::openapi_common::extensions")]
    pub extensions: HashMap<String, Value>,
}

/// Responses Object: container for the expected responses of an operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Responses {
    /// The documentation of responses other than the ones declared
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<ReferenceOr<Response>>,
    /// HTTP status codes with their corresponding Response Objects
    #[serde(flatten)]
    pub responses: HashMap<String, ReferenceOr<Response>>,
    /// Extension fields
    #[serde(flatten)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    #[serde(with = "crate::openapi_common::extensions")]
    pub extensions: HashMap<String, Value>,
}

/// Response Object: describes a single response from an API Operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    /// A short description of the response
    pub description: String,
    /// Maps a header name to its definition
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, ReferenceOr<Header>>>,
    /// A map containing descriptions of potential response payloads
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<HashMap<String, MediaType>>,
    /// A map of operations links that can be followed from the response
    #[serde(skip_serializing_if = "Option::is_none")]
    pub links: Option<HashMap<String, ReferenceOr<Link>>>,
    /// Extension fields
    #[serde(flatten)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    #[serde(with = "crate::openapi_common::extensions")]
    pub extensions: HashMap<String, Value>,
}

/// Callbacks Object: map of possible out-of band callbacks related to the parent operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Callbacks {
    /// Extension fields
    #[serde(flatten)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    #[serde(with = "crate::openapi_common::extensions")]
    pub extensions: HashMap<String, Value>,

    /// Callback expression with Path Item Object as values
    #[serde(flatten)]
    pub callbacks: HashMap<String, PathItem>,
}

/// Example Object: example object for content / parameter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Example {
    /// Short description for the example
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// Long description for the example
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Embedded literal example
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
    /// A URI that points to the literal example
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_value: Option<String>,
    /// Extension fields
    #[serde(flatten)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    #[serde(with = "crate::openapi_common::extensions")]
    pub extensions: HashMap<String, Value>,
}

/// Link Object: represents a possible design-time link for a response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Link {
    /// A relative or absolute URI reference to an OAS operation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation_ref: Option<String>,
    /// The name of an existing, resolvable OAS operation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,
    /// A map representing parameters to pass to the operation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<HashMap<String, Value>>,
    /// A literal value or expression to use as a request body
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_body: Option<Value>,
    /// A description of the link
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// A server object to be used by the target operation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server: Option<Server>,
    /// Extension fields
    #[serde(flatten)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    #[serde(with = "crate::openapi_common::extensions")]
    pub extensions: HashMap<String, Value>,
}

/// Header Object: represents a header
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Header {
    /// A brief description of the header
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Determines whether this header is mandatory
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    /// Specifies that a header is deprecated
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<bool>,
    /// The schema defining the type used for the header
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<Schema>,
    /// Example of the header's potential value
    #[serde(skip_serializing_if = "Option::is_none")]
    pub example: Option<Value>,
    /// Examples of the header's potential value
    #[serde(skip_serializing_if = "Option::is_none")]
    pub examples: Option<HashMap<String, ReferenceOr<Example>>>,
    /// A map containing descriptions of potential content
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<HashMap<String, MediaType>>,
    /// Describes how the header value will be serialized
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
    /// When this is true, header values of type array or object generate separate headers
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explode: Option<bool>,
    /// Extension fields
    #[serde(flatten)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    #[serde(with = "crate::openapi_common::extensions")]
    pub extensions: HashMap<String, Value>,
}

/// Tag Object: adds metadata to a single tag that is used by Operation Object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    /// The name of the tag
    pub name: String,
    /// A description for the tag
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Additional external documentation for this tag
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_docs: Option<ExternalDocumentation>,
    /// Extension fields
    #[serde(flatten)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    #[serde(with = "crate::openapi_common::extensions")]
    pub extensions: HashMap<String, Value>,
}

/// Security Scheme Object: defines a security scheme that can be used by the operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityScheme {
    /// The type of the security scheme
    #[serde(rename = "type")]
    pub type_: SecuritySchemeType,
    /// A description for security scheme
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The name of the header, query or cookie parameter to be used
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The location of the API key
    #[serde(rename = "in", skip_serializing_if = "Option::is_none")]
    pub in_: Option<SecuritySchemeLocation>,
    /// The name of the HTTP Authorization scheme
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheme: Option<String>,
    /// A hint to the client to identify how the bearer token is formatted
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bearer_format: Option<String>,
    /// OAuth Flow Object
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flows: Option<OAuthFlows>,
    /// OpenID Connect URL to discover OAuth2 configuration values
    #[serde(skip_serializing_if = "Option::is_none")]
    pub open_id_connect_url: Option<String>,
    /// Extension fields
    #[serde(flatten)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    #[serde(with = "crate::openapi_common::extensions")]
    pub extensions: HashMap<String, Value>,
}

/// Enum for security scheme type
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SecuritySchemeType {
    ApiKey,
    Http,
    MutualTLS,
    OAuth2,
    OpenIdConnect,
}

/// Enum for security scheme location
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SecuritySchemeLocation {
    Query,
    Header,
    Cookie,
}

/// OAuth Flows Object: allows configuration of the supported OAuth Flows
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthFlows {
    /// Configuration for the OAuth Implicit flow
    #[serde(skip_serializing_if = "Option::is_none")]
    pub implicit: Option<OAuthFlow>,
    /// Configuration for the OAuth Resource Owner Password flow
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<OAuthFlow>,
    /// Configuration for the OAuth Client Credentials flow
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_credentials: Option<OAuthFlow>,
    /// Configuration for the OAuth Authorization Code flow
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorization_code: Option<OAuthFlow>,
    /// Extension fields
    #[serde(flatten)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    #[serde(with = "crate::openapi_common::extensions")]
    pub extensions: HashMap<String, Value>,
}

/// OAuth Flow Object: configuration details for a supported OAuth Flow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthFlow {
    /// The authorization URL to be used for this flow
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorization_url: Option<String>,
    /// The token URL to be used for this flow
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_url: Option<String>,
    /// The URL to be used for obtaining refresh tokens
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_url: Option<String>,
    /// The available scopes for the OAuth2 security scheme
    pub scopes: HashMap<String, String>,
    /// Extension fields
    #[serde(flatten)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    #[serde(with = "crate::openapi_common::extensions")]
    pub extensions: HashMap<String, Value>,
}

/// Security Requirement Object: lists the required security schemes to execute this operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityRequirement {
    #[serde(flatten)]
    pub schemes: HashMap<String, Vec<String>>,
}

/// Schema Object: allows the definition of input and output data types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Schema {
    Object(Box<SchemaObject>),
    Boolean(bool),
}

/// Schema Object (when it's an object rather than a boolean)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaObject {
    // Standard JSON Schema fields are allowed but not explicitly defined here
    // as they can be quite extensive. In a real implementation you would add
    // these as needed.

    // Custom fields might also need to be added depending on your specific needs

    /// Extension fields
    #[serde(flatten)]
    pub schema_fields: HashMap<String, Value>,
}