use http::{HeaderMap, Method};
use lambda_http::{service_fn, Body, Error, IntoResponse, Request, RequestExt};
use oasert::types::HttpLike;
use oasert::validator::OpenApiPayloadValidator;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Error> {
    let openapi_file =
        std::fs::read_to_string("examples/aws-lambda-http-validation/openapi-v3.1.0.json").unwrap();
    let openapi_value: serde_json::Value = serde_json::from_str(&openapi_file).unwrap();
    let validator = Arc::new(OpenApiPayloadValidator::new(openapi_value).unwrap());
    lambda_http::run(service_fn(|req| {
        validation_function(validator.clone(), req)
    }))
    .await?;
    Ok(())
}

async fn validation_function(
    validator: Arc<OpenApiPayloadValidator>,
    request: Request,
) -> Result<impl IntoResponse, std::convert::Infallible> {
    let _context = request.lambda_context_ref();
    match validator.validate_request(&request, None) {
        Ok(_) => Ok(format!(
            "hello {}",
            request
                .query_string_parameters_ref()
                .and_then(|params| params.first("name"))
                .unwrap_or_else(|| "stranger")
        )),
        Err(e) => Ok(format!("{:?}", e)),
    }
}

pub struct AwsHttpLikeRequestWrapper<'a> {
    request: &'a Request,
}

impl HttpLike<String> for AwsHttpLikeRequestWrapper<'_> {
    fn method(&self) -> &Method {
        &self.request.method()
    }

    fn path(&self) -> &str {
        &self.request.uri().path()
    }

    fn headers(&self) -> &HeaderMap {
        &self.request.headers()
    }

    fn body(&self) -> Option<&String> {
        match self.request.body() {
            Body::Text(some) => Some(some),
            _ => None,
        }
    }

    fn query(&self) -> Option<&str> {
        self.request.uri().query()
    }
}

impl HttpLike<Vec<u8>> for AwsHttpLikeRequestWrapper<'_> {
    fn method(&self) -> &Method {
        &self.request.method()
    }

    fn path(&self) -> &str {
        &self.request.uri().path()
    }

    fn headers(&self) -> &HeaderMap {
        &self.request.headers()
    }

    fn body(&self) -> Option<&Vec<u8>> {
        match &self.request.body() {
            Body::Binary(binary) => Some(binary),
            _ => None,
        }
    }

    fn query(&self) -> Option<&str> {
        todo!()
    }
}
