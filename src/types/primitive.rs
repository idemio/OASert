use crate::error::ValidationErrorType;
use serde_json::{json, Value};
use std::fmt::{Display, Formatter};
use std::str::FromStr;

#[derive(PartialEq, Debug)]
pub enum OpenApiPrimitives {
    Null,
    Bool,
    Integer,
    Array,
    Number,
    String,
    Object,
}

impl Display for OpenApiPrimitives {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            OpenApiPrimitives::Null => write!(f, "null"),
            OpenApiPrimitives::Bool => write!(f, "bool"),
            OpenApiPrimitives::Integer => write!(f, "integer"),
            OpenApiPrimitives::Array => write!(f, "array"),
            OpenApiPrimitives::Number => write!(f, "number"),
            OpenApiPrimitives::String => write!(f, "string"),
            OpenApiPrimitives::Object => write!(f, "object"),
        }
    }
}

impl FromStr for OpenApiPrimitives {
    type Err = ValidationErrorType;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "null" => Ok(OpenApiPrimitives::Null),
            "bool" => Ok(OpenApiPrimitives::Bool),
            "integer" => Ok(OpenApiPrimitives::Integer),
            "number" => Ok(OpenApiPrimitives::Number),
            "string" => Ok(OpenApiPrimitives::String),
            "array" => Ok(OpenApiPrimitives::Array),
            "object" => Ok(OpenApiPrimitives::Object),
            &_ => todo!(),
        }
    }
}

impl OpenApiPrimitives {
    pub fn get_type_from_serde(schema: &Value) -> Option<OpenApiPrimitives> {
        if schema.is_string() {
            return Some(OpenApiPrimitives::String);
        } else if schema.is_array() {
            return Some(OpenApiPrimitives::Array);
        } else if schema.is_object() {
            return Some(OpenApiPrimitives::Object);
        } else if schema.is_null() {
            return Some(OpenApiPrimitives::Null);
        } else if schema.is_boolean() {
            return Some(OpenApiPrimitives::Bool);
        } else if schema.is_number() {
            return Some(OpenApiPrimitives::Number);
        }
        None
    }

    pub fn convert_string_to_schema_type(
        schema: &Value,
        input: &str,
    ) -> Result<Value, PrimitiveError> {
        let type_field = match schema
            .get("type")
            .and_then(|type_value| type_value.as_str())
        {
            None => {
                return Err(PrimitiveError::invalid_schema_error(
                    "Could not find 'type' field in schema.",
                ));
            }
            Some(v) => v,
        };
        let openapi_type = OpenApiPrimitives::from_str(type_field).map_err(|_| {
            PrimitiveError::invalid_schema_error(format!(
                "Invalid type field in schema: '{}'",
                type_field
            ))
        })?;
        openapi_type.convert_value_to_type(input)
    }

    pub fn convert_value_to_type(&self, input: &str) -> Result<Value, PrimitiveError> {
        match self {
            OpenApiPrimitives::Null => Ok(json!(Value::Null)),
            OpenApiPrimitives::Bool => Self::convert_to_type::<bool>(input),
            OpenApiPrimitives::Integer => Self::convert_to_type::<i64>(input),
            OpenApiPrimitives::Number => Self::convert_to_type::<f64>(input),
            OpenApiPrimitives::String => Self::convert_to_type::<String>(input),
            _ => {
                return Err(PrimitiveError::invalid_primitive_type(format!(
                    "unsupported type: '{}'",
                    self
                )));
            }
        }
    }

    fn convert_to_type<T: for<'de> serde::de::Deserialize<'de> + serde::Serialize + FromStr>(
        input: &str,
    ) -> Result<Value, PrimitiveError> {
        let converted_value: T = match input.parse::<T>() {
            Ok(val) => val,
            Err(_) => {
                return Err(PrimitiveError::conversion_error(format!(
                    "Could not convert '{}' to '{}'.",
                    input,
                    std::any::type_name::<T>()
                )));
            }
        };
        Ok(json!(converted_value))
    }
}

#[derive(Debug)]
pub enum PrimitiveError {
    ConversionError(String),
    InvalidSchemaError(String),
    InvalidPrimitiveType(String),
}

impl PrimitiveError {
    pub fn conversion_error(msg: impl Into<String>) -> Self {
        PrimitiveError::ConversionError(msg.into())
    }

    pub fn invalid_schema_error(msg: impl Into<String>) -> Self {
        PrimitiveError::InvalidSchemaError(msg.into())
    }

    pub fn invalid_primitive_type(msg: impl Into<String>) -> Self {
        PrimitiveError::InvalidPrimitiveType(msg.into())
    }
}

impl Display for PrimitiveError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            PrimitiveError::ConversionError(msg) => {
                write!(f, "Conversion error: {}", msg)
            }
            PrimitiveError::InvalidSchemaError(msg) => {
                write!(f, "Invalid schema error: {}", msg)
            }
            PrimitiveError::InvalidPrimitiveType(msg) => {
                write!(f, "Invalid primitive type: {}", msg)
            }
        }
    }
}

impl std::error::Error for PrimitiveError {}
