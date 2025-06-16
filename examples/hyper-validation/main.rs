use bytes::Bytes;
use http_body_util::{combinators::BoxBody, BodyExt, Empty};
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::Service;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use oasert::validator::OpenApiPayloadValidator;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use tokio::net::TcpListener;

fn empty() -> BoxBody<Bytes, hyper::Error> {
    Empty::<Bytes>::new()
        .map_err(|never| match never {})
        .boxed()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));

    let listener = TcpListener::bind(addr).await?;
    println!("Listening on http://{}", addr);

    let openapi_file =
        std::fs::read_to_string("examples/hyper-validation/openapi-v3.1.0.json").unwrap();
    let openapi_value: serde_json::Value = serde_json::from_str(&openapi_file).unwrap();
    let validator = Arc::new(OpenApiPayloadValidator::new(openapi_value).unwrap());
    let validation_service = TestHyperService { validator };
    loop {
        let (stream, _) = listener.accept().await?;
        let io = TokioIo::new(stream);

        let validator_service_clone = validation_service.clone();
        tokio::task::spawn(async move {
            if let Err(err) = http1::Builder::new()
                .serve_connection(io, validator_service_clone)
                .await
            {
                println!("Error serving connection: {:?}", err);
            }
        });
    }
}

#[derive(Clone)]
pub struct TestHyperService {
    pub validator: Arc<OpenApiPayloadValidator>,
}

impl Service<Request<Incoming>> for TestHyperService {
    type Response = Response<BoxBody<Bytes, hyper::Error>>;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn call(&self, req: Request<Incoming>) -> Self::Future {
        let validation_service = self.clone();
        Box::pin(async move {
            let (parts, body) = req.into_parts();
            let body = match body.collect().await {
                Ok(x) => x,
                Err(_) => panic!("Failed to collect body"),
            }
            .boxed_unsync();
            // Convert UnsyncBoxBody to bytes
            let bytes = match body.collect().await {
                Ok(bytes) => bytes,
                Err(_) => panic!("Error reading body data"),
            };

            // Parse string as JSON
            let json_value: serde_json::Value =
                serde_json::from_slice(&bytes.to_bytes()).expect("Body was not valid JSON");

            let request = Request::from_parts(parts, json_value);
            let mut response = Response::new(empty());
            match validation_service
                .validator
                .validate_request(&request, None)
            {
                Ok(_) => {
                    *response.status_mut() = StatusCode::OK;
                }
                Err(_) => {
                    *response.status_mut() = StatusCode::BAD_REQUEST;
                }
            }
            Ok(response)
        })
    }
}
