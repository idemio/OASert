use crate::error::{
    OperationSection, PayloadSection, Section, SpecificationSection, ValidationErrorType,
};
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
        if schema.as_str().is_some() {
            return Some(OpenApiPrimitives::String);
        } else if schema.as_array().is_some() {
            return Some(OpenApiPrimitives::Array);
        } else if schema.as_object().is_some() {
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
    ) -> Result<Value, ValidationErrorType> {
        let type_field = match schema
            .get("type")
            .and_then(|type_value| type_value.as_str())
        {
            None => {
                return Err(ValidationErrorType::FieldExpected(
                    String::from("type"),
                    Section::Specification(SpecificationSection::Paths(
                        OperationSection::Parameters,
                    )),
                ));
            }
            Some(v) => v,
        };
        let openapi_type = OpenApiPrimitives::from_str(type_field)?;
        openapi_type.convert_value_to_type(input)
    }

    pub fn convert_value_to_type(&self, input: &str) -> Result<Value, ValidationErrorType> {
        match self {
            OpenApiPrimitives::Null => Ok(json!(Value::Null)),
            OpenApiPrimitives::Bool => Self::convert_to_type::<bool>(input),
            OpenApiPrimitives::Integer => Self::convert_to_type::<i64>(input),
            OpenApiPrimitives::Number => Self::convert_to_type::<f64>(input),
            OpenApiPrimitives::String => Self::convert_to_type::<String>(input),
            _ => todo!(),
        }
    }

    fn convert_to_type<T: for<'de> serde::de::Deserialize<'de> + serde::Serialize + FromStr>(
        input: &str,
    ) -> Result<Value, ValidationErrorType> {
        let converted_value: T = match input.parse::<T>() {
            Ok(val) => val,
            Err(_) => {
                return Err(ValidationErrorType::UnableToParse(
                    input.to_string(),
                    Section::Payload(PayloadSection::Path),
                ));
            }
        };
        Ok(json!(converted_value))
    }
}
