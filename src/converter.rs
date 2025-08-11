use bytes::Bytes;
use http::{HeaderMap, Method};
use serde::Serialize;
use serde_json::Value;

pub trait HttpLike<T>
where
    T: Serialize,
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
    T: Serialize,
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

pub trait RequestBody: Send + Sync {
    type Error: std::error::Error + Send + Sync + 'static;

    fn to_bytes(self) -> impl Future<Output = Result<Bytes, Self::Error>> + Send;
    fn to_string(self) -> impl Future<Output = Result<String, Self::Error>> + Send;
    fn to_json(self) -> impl Future<Output = Result<Value, Self::Error>> + Send;
}

impl RequestBody for String {
    type Error = std::convert::Infallible;

    async fn to_bytes(self) -> Result<Bytes, Self::Error> {
        Ok(Bytes::from(self))
    }

    async fn to_string(self) -> Result<String, Self::Error> {
        Ok(self)
    }

    async fn to_json(self) -> Result<Value, Self::Error> {
        serde_json::from_str(&self).map_err(|_| unreachable!())
    }
}

#[cfg(feature = "hyper")]
pub mod hyper {
    use crate::converter::RequestBody;
    use bytes::Bytes;
    use serde_json::Value;
    use std::fmt::{Display, Formatter};

    #[derive(Debug)]
    pub enum HyperError {
        InvalidUtf8,
        InvalidJson,
        FailedToReadStream,
    }

    impl Display for HyperError {
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            match self {
                HyperError::InvalidJson => {
                    write!(f, "Invalid JSON")
                }
                HyperError::InvalidUtf8 => {
                    write!(f, "Invalid UTF-8")
                }
                HyperError::FailedToReadStream => {
                    write!(f, "Failed to read stream")
                }
            }
        }
    }

    impl std::error::Error for HyperError {}

    impl RequestBody for hyper::body::Incoming {
        type Error = HyperError;

        async fn to_bytes(self) -> Result<Bytes, Self::Error> {
            use http_body_util::BodyExt;
            match self.collect().await {
                Ok(collected) => Ok(collected.to_bytes()),
                Err(_) => Err(HyperError::FailedToReadStream),
            }
        }

        async fn to_string(self) -> Result<String, Self::Error> {
            let bytes = self.to_bytes().await?;
            String::from_utf8(bytes.to_vec()).map_err(|_| HyperError::InvalidUtf8)
        }

        async fn to_json(self) -> Result<Value, Self::Error> {
            let string = self.to_string().await?;
            serde_json::from_str(&string).map_err(|_| HyperError::InvalidJson)
        }
    }
}

#[cfg(feature = "lambda_http")]
pub mod lambda_http {
    use crate::converter::RequestBody;
    use bytes::Bytes;
    use lambda_http::request::LambdaRequest;
    use serde_json::Value;
    use std::fmt::{Display, Formatter};

    #[derive(Debug)]
    pub enum AwsLambdaError {
        InvalidUtf8,
        InvalidJson,
        InvalidApiGatewayV1Request,
        InvalidApiGatewayV2Request,
        InvalidAlbRequest,
        InvalidWebSocketRequest,
    }

    impl RequestBody for LambdaRequest {
        type Error = AwsLambdaError;

        async fn to_bytes(self) -> Result<Bytes, Self::Error> {
            match self {
                LambdaRequest::ApiGatewayV1(req) => match serde_json::to_vec(&req) {
                    Ok(bytes) => Ok(Bytes::from(bytes)),
                    Err(_) => Err(AwsLambdaError::InvalidApiGatewayV1Request),
                },
                LambdaRequest::ApiGatewayV2(req) => match serde_json::to_vec(&req) {
                    Ok(bytes) => Ok(Bytes::from(bytes)),
                    Err(_) => Err(AwsLambdaError::InvalidApiGatewayV2Request),
                },
                LambdaRequest::Alb(req) => match serde_json::to_vec(&req) {
                    Ok(bytes) => Ok(Bytes::from(bytes)),
                    Err(_) => Err(AwsLambdaError::InvalidAlbRequest),
                },
                LambdaRequest::WebSocket(req) => match serde_json::to_vec(&req) {
                    Ok(bytes) => Ok(Bytes::from(bytes)),
                    Err(_) => Err(AwsLambdaError::InvalidWebSocketRequest),
                },
            }
        }

        async fn to_string(self) -> Result<String, Self::Error> {
            let bytes = self.to_bytes().await?;
            String::from_utf8(bytes.to_vec()).map_err(|_| AwsLambdaError::InvalidUtf8)
        }

        async fn to_json(self) -> Result<Value, Self::Error> {
            let string = self.to_string().await?;
            serde_json::from_str(&string).map_err(|_| AwsLambdaError::InvalidJson)
        }
    }

    impl Display for AwsLambdaError {
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            match self {
                AwsLambdaError::InvalidUtf8 => {
                    write!(f, "Invalid UTF-8")
                }
                AwsLambdaError::InvalidJson => {
                    write!(f, "Invalid JSON")
                }
                AwsLambdaError::InvalidApiGatewayV1Request => {
                    write!(f, "Invalid API Gateway v1 request")
                }
                AwsLambdaError::InvalidApiGatewayV2Request => {
                    write!(f, "Invalid API Gateway v2 request")
                }
                AwsLambdaError::InvalidAlbRequest => {
                    write!(f, "Invalid ALB request")
                }
                AwsLambdaError::InvalidWebSocketRequest => {
                    write!(f, "Invalid WebSocket request")
                }
            }
        }
    }

    impl std::error::Error for AwsLambdaError {}
}
