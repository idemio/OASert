use serde_json::Value;
use crate::JsonPath;

pub struct Operation {
    pub(crate) data: Value,
    pub(crate) path: JsonPath
}

impl Operation {
    pub(crate) fn into_parts(self) -> (Value, JsonPath) {
        (self.data, self.path)
    }
}