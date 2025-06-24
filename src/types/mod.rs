pub mod json_path;
pub mod primitive;
pub mod version;

use crate::types::json_path::JsonPath;
use http::{HeaderMap, Method};
use serde_json::Value;
use std::fmt::{Display, Formatter};

#[derive(Debug)]
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

pub trait HttpLike<T>
where
    T: serde::ser::Serialize,
{
    fn method_ref(&self) -> &Method;
    fn path_ref(&self) -> &str;
    fn headers_ref(&self) -> &HeaderMap;
    fn body_ref(&self) -> &T;
    fn converted_body(&self) -> Option<Value>;
    fn query_ref(&self) -> Option<&str>;
}

impl<T> HttpLike<T> for http::Request<T>
where
    T: serde::ser::Serialize,
{
    fn method_ref(&self) -> &Method {
        &self.method()
    }

    fn path_ref(&self) -> &str {
        &self.uri().path()
    }

    fn headers_ref(&self) -> &HeaderMap {
        &self.headers()
    }

    fn body_ref(&self) -> &T {
        &self.body()
    }

    fn converted_body(&self) -> Option<Value> {
        match serde_json::to_value(self.body()) {
            Ok(val) => Some(val),
            Err(_) => None,
        }
    }

    fn query_ref(&self) -> Option<&str> {
        match &self.uri().query() {
            None => None,
            Some(x) => Some(x),
        }
    }
}
