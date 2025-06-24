use lambda_http::{service_fn, Body, Error, IntoResponse, RequestExt};
use oasert::cache::global_validator_cache;
use oasert::validator::OpenApiPayloadValidator;
use serde_json::Value;
use std::sync::Arc;

const VALIDATOR_CACHE_ID: &'static str = "MY_VALIDATOR";

#[tokio::main]
async fn main() -> Result<(), Error> {
    let validator_id = VALIDATOR_CACHE_ID.to_string();
    let validator: Arc<OpenApiPayloadValidator> =
        if global_validator_cache().contains(&validator_id) {
            match global_validator_cache().get(&validator_id) {
                Ok(validator) => validator,
                Err(_) => panic!("Cache error occurred!"),
            }
        } else {
            let openapi_file =
                std::fs::read_to_string("examples/aws-lambda-http-validation/openapi-v3.1.0.json")
                    .unwrap();
            let openapi_value: Value = serde_json::from_str(&openapi_file).unwrap();

            match global_validator_cache().insert(validator_id, openapi_value) {
                Ok(validator) => validator,
                Err(_) => panic!("Cache error occurred!"),
            }
        };

    lambda_http::run(service_fn(|req| {
        validation_function(validator.clone(), req)
    }))
    .await?;
    Ok(())
}

async fn validation_function(
    validator: Arc<OpenApiPayloadValidator>,
    request: http::Request<Body>,
) -> Result<impl IntoResponse, std::convert::Infallible> {
    let _context = request.lambda_context_ref();
    let response = match validator.validate_request(&request, None) {
        Ok(_) => http::Response::builder()
            .status(200)
            .body(String::from("OK")),
        Err(e) => {
            println!("{}", e);
            http::Response::builder()
                .status(400)
                .body(String::from("Bad Request"))
        }
    }
    .unwrap();

    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::Method;
    use http_body_util::BodyExt;
    use lambda_http::{Body, Context, Request};
    use oasert::validator::OpenApiPayloadValidator;
    use std::sync::Arc;

    // Helper function to create a test request
    fn create_test_request(
        method: Method,
        uri: &str,
        query_params: Option<&[(&str, &str)]>,
        body: Option<Body>,
    ) -> Request {
        let mut request = if let Some(body) = body {
            Request::new(body)
        } else {
            Request::default()
        };
        *request.method_mut() = method;
        *request.uri_mut() = uri.parse().unwrap();
        request.extensions_mut().insert(Context::default());
        request
    }

    fn create_validator() -> Arc<OpenApiPayloadValidator> {
        let openapi_file =
            std::fs::read_to_string("examples/aws-lambda-http-validation/openapi-v3.1.0.json")
                .unwrap();
        let openapi_value: serde_json::Value = serde_json::from_str(&openapi_file).unwrap();
        Arc::new(OpenApiPayloadValidator::new(openapi_value).unwrap())
    }

    #[tokio::test]
    async fn test_validation_function_success() {
        let validator = create_validator();
        let request = create_test_request(Method::GET, "/pets", Some(&[("name", "tester")]), None);
        let response = validation_function(validator, request).await.unwrap();
        let response = response.into_response().await;
        let bytes = response.collect().await.unwrap().to_bytes();
        let response_body = String::from_utf8(bytes.to_vec()).unwrap();
        assert!(response_body.contains("OK"));
    }
}
