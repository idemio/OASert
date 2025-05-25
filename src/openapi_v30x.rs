use std::collections::HashMap;
use serde::{Serialize, Deserialize};

/// Represents an OpenAPI 3.0.x document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenApiDocument {
    /// The version of the OpenAPI specification
    pub openapi: String,
    /// Metadata about the API
    pub info: Info,
    /// Available paths and operations for the API
    pub paths: Paths,
    /// External documentation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_docs: Option<ExternalDocumentation>,
    /// An array of Server Objects which provide connectivity information
    #[serde(skip_serializing_if = "Option::is_none")]
    pub servers: Option<Vec<Server>>,
    /// A declaration of which security mechanisms can be used
    #[serde(skip_serializing_if = "Option::is_none")]
    pub security: Option<Vec<SecurityRequirement>>,
    /// A list of tags used by the specification
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<Tag>>,
    /// An element to hold various schemas
    #[serde(skip_serializing_if = "Option::is_none")]
    pub components: Option<Components>,
    /// Additional custom properties
    #[serde(flatten)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub extensions: HashMap<String, serde_json::Value>,
}

/// Object to hold reusable components
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Components {
    /// Map of reusable Schema Objects
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schemas: Option<HashMap<String, ReferenceOr<Schema>>>,
    /// Map of reusable Response Objects
    #[serde(skip_serializing_if = "Option::is_none")]
    pub responses: Option<HashMap<String, ReferenceOr<Response>>>,
    /// Map of reusable Parameter Objects
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<HashMap<String, ReferenceOr<Parameter>>>,
    /// Map of reusable Example Objects
    #[serde(skip_serializing_if = "Option::is_none")]
    pub examples: Option<HashMap<String, ReferenceOr<Example>>>,
    /// Map of reusable Request Body Objects
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_bodies: Option<HashMap<String, ReferenceOr<RequestBody>>>,
    /// Map of reusable Header Objects
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, ReferenceOr<Header>>>,
    /// Map of reusable Security Scheme Objects
    #[serde(skip_serializing_if = "Option::is_none")]
    pub security_schemes: Option<HashMap<String, ReferenceOr<SecurityScheme>>>,
    /// Map of reusable Link Objects
    #[serde(skip_serializing_if = "Option::is_none")]
    pub links: Option<HashMap<String, ReferenceOr<Link>>>,
    /// Map of reusable Callback Objects
    #[serde(skip_serializing_if = "Option::is_none")]
    pub callbacks: Option<HashMap<String, ReferenceOr<Callback>>>,
    /// Additional custom properties
    #[serde(flatten)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub extensions: HashMap<String, serde_json::Value>,
}

/// Reference Object - used to reference schema definitions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reference {
    /// Reference string in the format of a URL
    #[serde(rename = "$ref")]
    pub ref_field: String,
}

/// A type that can either be a Reference or a specific type T
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ReferenceOr<T> {
    Reference(Reference),
    Item(T),
}

/// Information about the API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Info {
    /// The title of the API
    pub title: String,
    /// The version of the API
    pub version: String,
    /// A short description of the API
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// A URL to the Terms of Service for the API
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terms_of_service: Option<String>,
    /// Contact information for the API
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contact: Option<Contact>,
    /// License information for the API
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<License>,
    /// Additional custom properties
    #[serde(flatten)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub extensions: HashMap<String, serde_json::Value>,
}

/// Contact information for the API
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
    /// Additional custom properties
    #[serde(flatten)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub extensions: HashMap<String, serde_json::Value>,
}

/// License information for the API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct License {
    /// The license name
    pub name: String,
    /// URL to the license
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Additional custom properties
    #[serde(flatten)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub extensions: HashMap<String, serde_json::Value>,
}

/// Server Object representing a server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Server {
    /// URL to the target host
    pub url: String,
    /// A description of the server
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Variables used in the server URL template
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variables: Option<HashMap<String, ServerVariable>>,
    /// Additional custom properties
    #[serde(flatten)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub extensions: HashMap<String, serde_json::Value>,
}

/// Server Variable object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerVariable {
    /// Default value to use for substitution
    pub default: String,
    /// An enumeration of string values to be used if the substitution options are from a limited set
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enum_values: Option<Vec<String>>,
    /// A description for the server variable
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Additional custom properties
    #[serde(flatten)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub extensions: HashMap<String, serde_json::Value>,
}

/// Paths object - holds the relative paths to the individual endpoints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Paths {
    /// Additional custom properties and path items
    #[serde(flatten)]
    pub paths: HashMap<String, ReferenceOr<PathItem>>,
}

/// Path Item Object - describes operations available on a single path
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathItem {
    /// A summary for the path item
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// A description for the path item
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
    /// A list of parameters that are applicable for all operations on this path
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Vec<ReferenceOr<Parameter>>>,
    /// Additional custom properties
    #[serde(flatten)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub extensions: HashMap<String, serde_json::Value>,
}

/// External Documentation Object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalDocumentation {
    /// The URL for the target documentation
    pub url: String,
    /// A description of the target documentation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Additional custom properties
    #[serde(flatten)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub extensions: HashMap<String, serde_json::Value>,
}

/// Schema Object - allows defining input and output data types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Schema {
    /// Schema title
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Type of the schema
    #[serde(skip_serializing_if = "Option::is_none")]
    pub type_: Option<String>,
    /// Format of the schema
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    /// Required properties if type is object
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<Vec<String>>,
    /// Properties if type is object
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<HashMap<String, ReferenceOr<Schema>>>,
    /// Items if type is array
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Box<ReferenceOr<Schema>>>,
    /// Enumeration values
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enum_values: Option<Vec<serde_json::Value>>,
    /// All schemas must match
    #[serde(skip_serializing_if = "Option::is_none")]
    pub all_of: Option<Vec<ReferenceOr<Schema>>>,
    /// One schema must match
    #[serde(skip_serializing_if = "Option::is_none")]
    pub one_of: Option<Vec<ReferenceOr<Schema>>>,
    /// Any schema must match
    #[serde(skip_serializing_if = "Option::is_none")]
    pub any_of: Option<Vec<ReferenceOr<Schema>>>,
    /// Schema to negate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub not: Option<Box<ReferenceOr<Schema>>>,
    // Numerical validations
    #[serde(skip_serializing_if = "Option::is_none")]
    pub multiple_of: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maximum: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclusive_maximum: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minimum: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclusive_minimum: Option<bool>,
    // String validations
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_length: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_length: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    // Array validations
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_items: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_items: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unique_items: Option<bool>,
    // Object validations
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_properties: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_properties: Option<u64>,
    // Common fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nullable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub write_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub example: Option<serde_json::Value>,
    /// Additional custom properties
    #[serde(flatten)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub extensions: HashMap<String, serde_json::Value>,
}

/// Operation Object - describes a single API operation on a path
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
    /// Additional external documentation
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
    pub callbacks: Option<HashMap<String, ReferenceOr<Callback>>>,
    /// Declares this operation to be deprecated
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<bool>,
    /// A declaration of which security mechanisms can be used for this operation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub security: Option<Vec<SecurityRequirement>>,
    /// A list of servers that provide connectivity information
    #[serde(skip_serializing_if = "Option::is_none")]
    pub servers: Option<Vec<Server>>,
    /// Additional custom properties
    #[serde(flatten)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub extensions: HashMap<String, serde_json::Value>,
}

/// Parameter Object - describes a single operation parameter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Parameter {
    /// The name of the parameter
    pub name: String,
    /// The location of the parameter
    pub in_: String,
    /// A brief description of the parameter
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Determines whether this parameter is mandatory
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    /// Specifies that a parameter is deprecated
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<bool>,
    /// Sets the ability to pass empty-valued parameters
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_empty_value: Option<bool>,
    /// The schema defining the type used for the parameter
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<ReferenceOr<Schema>>,
    /// Additional custom properties
    #[serde(flatten)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub extensions: HashMap<String, serde_json::Value>,
}

/// Request Body Object
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
    /// Additional custom properties
    #[serde(flatten)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub extensions: HashMap<String, serde_json::Value>,
}

/// Media Type Object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaType {
    /// The schema defining the content of the request, response, or parameter
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<ReferenceOr<Schema>>,
    /// Example of the media type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub example: Option<serde_json::Value>,
    /// Examples of the media type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub examples: Option<HashMap<String, ReferenceOr<Example>>>,
    /// Additional custom properties
    #[serde(flatten)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub extensions: HashMap<String, serde_json::Value>,
}

/// Responses Object - container for the expected responses of an operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Responses {
    /// The documentation of responses other than the ones declared
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<ReferenceOr<Response>>,
    /// Any HTTP status code can be used as the property name
    #[serde(flatten)]
    pub responses: HashMap<String, ReferenceOr<Response>>,
}

/// Response Object - describes a single response from an API Operation
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
    /// Additional custom properties
    #[serde(flatten)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub extensions: HashMap<String, serde_json::Value>,
}

/// Example Object
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
    pub value: Option<serde_json::Value>,
    /// A URL that points to the literal example
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_value: Option<String>,
    /// Additional custom properties
    #[serde(flatten)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub extensions: HashMap<String, serde_json::Value>,
}

/// Header Object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Header {
    /// A brief description of the header
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Schema for the header
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<ReferenceOr<Schema>>,
    /// Additional custom properties
    #[serde(flatten)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub extensions: HashMap<String, serde_json::Value>,
}

/// Tag Object
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
    /// Additional custom properties
    #[serde(flatten)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub extensions: HashMap<String, serde_json::Value>,
}

/// Security Requirement Object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityRequirement {
    /// Each name must correspond to a security scheme
    #[serde(flatten)]
    pub requirements: HashMap<String, Vec<String>>,
}

/// Security Scheme Object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityScheme {
    /// The type of the security scheme
    #[serde(rename = "type")]
    pub type_: String,
    /// A description for security scheme
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The name of the header, query or cookie parameter to be used
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The location of the API key
    #[serde(skip_serializing_if = "Option::is_none")]
    pub in_: Option<String>,
    /// The name of the HTTP Authorization scheme
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheme: Option<String>,
    /// A hint to the client to identify how the bearer token is formatted
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bearer_format: Option<String>,
    /// OpenID Connect URL to discover OAuth2 configuration values
    #[serde(skip_serializing_if = "Option::is_none")]
    pub open_id_connect_url: Option<String>,
    /// OAuth2 flows
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flows: Option<OAuthFlows>,
    /// Additional custom properties
    #[serde(flatten)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub extensions: HashMap<String, serde_json::Value>,
}

/// OAuth Flows Object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthFlows {
    /// Configuration for the OAuth Implicit flow
    #[serde(skip_serializing_if = "Option::is_none")]
    pub implicit: Option<OAuthFlow>,
    /// Configuration for the OAuth Password flow
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<OAuthFlow>,
    /// Configuration for the OAuth Client Credentials flow
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_credentials: Option<OAuthFlow>,
    /// Configuration for the OAuth Authorization Code flow
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorization_code: Option<OAuthFlow>,
    /// Additional custom properties
    #[serde(flatten)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub extensions: HashMap<String, serde_json::Value>,
}

/// OAuth Flow Object
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
    /// Additional custom properties
    #[serde(flatten)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub extensions: HashMap<String, serde_json::Value>,
}

/// Link Object
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
    pub parameters: Option<HashMap<String, serde_json::Value>>,
    /// A description of the link
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Additional custom properties
    #[serde(flatten)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub extensions: HashMap<String, serde_json::Value>,
}

/// Callback Object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Callback {
    /// A Path Item Object used to define a callback operation
    #[serde(flatten)]
    pub callbacks: HashMap<String, ReferenceOr<PathItem>>,
}