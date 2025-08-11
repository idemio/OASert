pub mod json_path;
pub mod operation;
pub mod primitive;
pub mod version;

use crate::converter::RequestBody;
use http::{HeaderMap, Method};
use serde_json::Value;
use std::fmt::{Display, Formatter};

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


