use crate::types::json_path::JsonPath;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Serialize)]
pub struct Operation {
    pub(crate) data: Value,

    #[serde(skip_serializing)]
    pub(crate) path: JsonPath,
}

#[derive(Debug, Serialize)]
pub struct OperationV2 {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) description: Option<String>,
    #[serde(rename = "externalDocs", skip_serializing_if = "Option::is_none")]
    pub(crate) external_docs: Option<Value>,
    #[serde(rename = "operationId", skip_serializing_if = "Option::is_none")]
    pub(crate) operation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) parameters: Vec<Value>,
    #[serde(rename = "requestBody", skip_serializing_if = "Option::is_none")]
    pub(crate) request_body: Option<RequestBody>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) responses: Option<Value>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub(crate) callbacks: HashMap<String, Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) deprecated: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) security: Vec<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) servers: Vec<Value>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub(crate) extensions: HashMap<String, Value>,

    #[serde(skip_serializing)]
    pub(crate) path: JsonPath,
}

impl OperationV2 {
    pub(crate) fn set_path(&mut self, json_path: JsonPath) {
        self.path = json_path
    }
}

#[derive(Debug, Serialize)]
pub struct RequestBody {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) description: Option<String>,

    pub(crate) content: HashMap<String, Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) required: Option<bool>,
}

impl RequestBody {
    pub(crate) fn is_body_required(&self) -> bool {
        self.required.unwrap_or_else(|| false)
    }

    pub(crate) fn get_content_type(&self, content_type: &str) -> Option<&Value> {
        self.content.get(content_type)
    }
}
