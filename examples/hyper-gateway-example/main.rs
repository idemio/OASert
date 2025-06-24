use bytes::Bytes;
use http::Response;
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::{server::conn::http1, service::service_fn};
use hyper_util::rt::TokioIo;
use oasert::cache::ValidatorCollection;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};

type ResponseBody = BoxBody<Bytes, Box<dyn std::error::Error + Send + Sync>>;

// Helper function to create error responses - now uses the same error type
fn error_response(status: u16, message: &str) -> Response<BoxBody<Bytes, Infallible>> {
    let body = Full::new(Bytes::from(message.to_string()))
        .map_err(|e| e)
        .boxed();
    Response::builder().status(status).body(body).unwrap()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let in_addr: SocketAddr = ([127, 0, 0, 1], 3001).into();
    let out_addr: SocketAddr = ([127, 0, 0, 1], 3000).into();
    let out_addr_clone = out_addr;
    let listener = TcpListener::bind(in_addr).await?;

    let validator_cache = ValidatorCollection::<String>::new();

    // Insert api 1 into the validator cache
    let spec1_path = "examples/hyper-gateway-example/api1-v3.1.0.json";
    let spec1_prefix = "/api1";
    match validator_cache.insert_from_file_path(spec1_prefix.to_string(), spec1_path.to_string()) {
        Ok(x) => x,
        Err(_) => panic!("Failed to insert spec1 into validator cache"),
    };

    // Insert api2 into the validator cache
    let spec2_path = "examples/hyper-gateway-example/api2-v3.1.0.json";
    let spec2_prefix = "/api2";
    match validator_cache.insert_from_file_path(spec2_prefix.to_string(), spec2_path.to_string()) {
        Ok(x) => x,
        Err(_) => panic!("Failed to insert spec2 into validator cache"),
    };

    let shared_cache = Arc::new(validator_cache);

    loop {
        let (stream, _) = listener.accept().await?;
        let io = TokioIo::new(stream);
        let loop_cache = shared_cache.clone();

        let service = service_fn(move |mut req: http::Request<Incoming>| {
            let cache_binding = loop_cache.clone();
            async move {
                let uri_string = format!(
                    "http://{}{}",
                    out_addr_clone,
                    req.uri()
                        .path_and_query()
                        .map(|x| x.as_str())
                        .unwrap_or("/")
                );

                let uri = match uri_string.parse() {
                    Ok(uri) => uri,
                    Err(_) => {
                        return Ok::<Response<BoxBody<Bytes, Infallible>>, Infallible>(
                            error_response(400, "Invalid URI format"),
                        );
                    }
                };
                *req.uri_mut() = uri;

                let host = match req.uri().host() {
                    Some(host) => host,
                    None => return Ok(error_response(400, "URI has no host")),
                };
                let port = req.uri().port_u16().unwrap_or(80);
                let addr = format!("{}:{}", host, port);

                let inner_cache = cache_binding.clone();

                // Connect to downstream service
                let client_stream = match TcpStream::connect(addr).await {
                    Ok(stream) => stream,
                    Err(e) => {
                        println!("Failed to connect to downstream: {:?}", e);
                        return Ok(error_response(
                            502,
                            "Failed to connect to downstream service",
                        ));
                    }
                };
                let io = TokioIo::new(client_stream);

                // Collect and convert request body
                let (parts, body) = req.into_parts();
                let body_bytes = match body.collect().await {
                    Ok(collected) => collected.to_bytes(),
                    Err(e) => {
                        println!("Failed to read request body: {:?}", e);
                        return Ok(error_response(400, "Failed to read request body"));
                    }
                };

                let string_body = match String::from_utf8(body_bytes.to_vec()) {
                    Ok(s) => s,
                    Err(e) => {
                        println!("Request body contains invalid UTF-8: {:?}", e);
                        return Ok(error_response(400, "Request body contains invalid UTF-8"));
                    }
                };

                let mut req = http::Request::from_parts(parts, string_body);

                // Validation logic
                if req.uri().path().starts_with(spec1_prefix) {
                    let validator = match inner_cache.get(&spec1_prefix.to_string()) {
                        Ok(x) => x,
                        Err(e) => {
                            println!("No validator found for spec1: {:?}", e);
                            return Ok(error_response(
                                500,
                                "Internal server error - validator not found",
                            ));
                        }
                    };

                    let result = validator.validate_request(&req, None);
                    match result {
                        Ok(_) => println!("Request is valid when validating against spec1!"),
                        Err(err) => {
                            println!("Validation failed for spec1: {:?}", err);
                            return Ok(error_response(400, &err.to_string()));
                        }
                    }
                } else if req.uri().path().starts_with(spec2_prefix) {
                    let validator = match inner_cache.get(&spec2_prefix.to_string()) {
                        Ok(x) => x,
                        Err(e) => {
                            println!("No validator found for spec2: {:?}", e);
                            return Ok(error_response(
                                500,
                                "Internal server error - validator not found",
                            ));
                        }
                    };

                    let result = validator.validate_request(&req, None);
                    match result {
                        Ok(_) => println!("Request is valid when validating against spec2!"),
                        Err(err) => {
                            println!("Validation failed for spec2: {:?}", err);
                            return Ok(error_response(400, &err.to_string()));
                        }
                    }
                } else {
                    println!("Path does not start with /api1 or /api2.");
                }

                // Establish HTTP connection to downstream
                let (mut sender, conn) = match hyper::client::conn::http1::handshake(io).await {
                    Ok((sender, conn)) => (sender, conn),
                    Err(e) => {
                        println!("HTTP handshake failed: {:?}", e);
                        return Ok(error_response(502, "Failed to establish HTTP connection"));
                    }
                };

                // Spawn connection task
                tokio::task::spawn(async move {
                    if let Err(err) = conn.await {
                        println!("Connection failed: {:?}", err);
                    }
                });

                // Send request to downstream service
                let response = match sender.send_request(req).await {
                    Ok(resp) => resp,
                    Err(e) => {
                        println!("Failed to send request to downstream: {:?}", e);
                        return Ok(error_response(
                            502,
                            "Failed to forward request to downstream service",
                        ));
                    }
                };

                // Convert response body to boxed body with proper error type mapping
                let (parts, body) = response.into_parts();
                let boxed_body = body
                    .map_err(|_| {
                        unreachable!("This should not happen as we handle errors properly")
                    })
                    .boxed();
                let response = Response::from_parts(parts, boxed_body);
                Ok(response)
            }
        });

        tokio::task::spawn(async move {
            if let Err(err) = http1::Builder::new().serve_connection(io, service).await {
                println!("Failed to serve the connection: {:?}", err);
            }
        });
    }
}
