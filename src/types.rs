use crate::validator::{JsonPath, ValidationError};
use jsonschema::Draft;
use serde_json::{Value, json, Map};
use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::str::FromStr;
use unicase::UniCase;

#[derive(PartialEq, Debug)]
pub enum OpenApiTypes {
    Null,
    Bool,
    Integer,
    Array,
    Number,
    String,
    Object,
}

impl FromStr for OpenApiTypes {
    type Err = ValidationError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "null" => Ok(OpenApiTypes::Null),
            "bool" => Ok(OpenApiTypes::Bool),
            "integer" => Ok(OpenApiTypes::Integer),
            "number" => Ok(OpenApiTypes::Number),
            "string" => Ok(OpenApiTypes::String),
            "array" => Ok(OpenApiTypes::Array),
            "object" => Ok(OpenApiTypes::Object),
            &_ => Err(ValidationError::InvalidType)
        }
    }
}

impl OpenApiTypes {
    
    pub fn convert_string_to_schema_type(schema: &Value, input: &str) -> Result<Value, ValidationError> {
        let type_field = match schema.get("type").and_then(|type_value| type_value.as_str()) {
            None => return Err(ValidationError::FieldMissing),
            Some(v) => v
        };
        let openapi_type = OpenApiTypes::from_str(type_field)?;
        openapi_type.convert_value_to_type(input)
    }
    
    pub fn convert_value_to_type(&self, input: &str) -> Result<Value, ValidationError> {
        match self {
            OpenApiTypes::Null => Ok(json!(Value::Null)),
            OpenApiTypes::Bool => Self::convert_to_type::<bool>(input),
            OpenApiTypes::Integer => Self::convert_to_type::<i64>(input),
            OpenApiTypes::Number => Self::convert_to_type::<f64>(input),
            OpenApiTypes::String => Self::convert_to_type::<String>(input),
//            OpenApiTypes::Array => Self::convert_to_type::<Vec<Value>>(input),
//            OpenApiTypes::Object => Self::convert_to_type::<Map<String, Value>>(input)
            _ => Err(ValidationError::UnsupportedSpecVersion)
        }
    }

    fn convert_to_type<T: for<'de> serde::de::Deserialize<'de> + serde::Serialize + std::str::FromStr>(
        input: &str,
    ) -> Result<Value, ValidationError> {
        let converted_value: T = match input.parse::<T>() {
            Ok(val) => val,
            Err(_) => return Err(ValidationError::InvalidType)
        };
        Ok(json!(converted_value))
    }
}

pub struct Operation {
    pub(crate) data: Value,
    pub(crate) path: JsonPath,
}

#[derive(PartialEq, Debug)]
pub enum ParameterLocation {
    Header,
    Query,
    Cookie,
    Path,
}

impl Display for ParameterLocation {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let str = String::from(match self {
            ParameterLocation::Header => "header",
            ParameterLocation::Query => "query",
            ParameterLocation::Cookie => "cookie",
            ParameterLocation::Path => "path",
        });
        write!(f, "{}", str)
    }
}

pub enum OpenApiVersion {
    V30x,
    V31x,
}

impl FromStr for OpenApiVersion {
    type Err = ValidationError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.starts_with("3.1") {
            Ok(OpenApiVersion::V31x)
        } else if s.starts_with("3.0") {
            Ok(OpenApiVersion::V30x)
        } else {
            Err(ValidationError::UnsupportedSpecVersion)
        }
    }
}

impl OpenApiVersion {
    pub(crate) fn get_draft(&self) -> Draft {
        match self {
            OpenApiVersion::V30x => Draft::Draft4,
            OpenApiVersion::V31x => Draft::Draft202012,
        }
    }
}

pub trait RequestParamData {
    fn get(&self) -> &HashMap<UniCase<String>, String>;
}

pub trait RequestBodyData {
    fn get(&self) -> &Value;
}
