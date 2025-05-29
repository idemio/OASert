use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::str::FromStr;
use jsonschema::Draft;
use serde_json::Value;
use unicase::UniCase;
use crate::{JsonPath, ValidationError};

pub struct Operation {
    pub(crate) data: Value,
    pub(crate) path: JsonPath
}

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
            Err(ValidationError::UnsupportedSpecVersion(s.to_string()))
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