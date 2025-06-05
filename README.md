[![codecov](https://codecov.io/gh/idemio/OASert/branch/main/graph/badge.svg)](https://codecov.io/gh/idemio/OASert)
[![CI Status](https://github.com/idemio/OASert/workflows/Rust/badge.svg)](https://github.com/idemio/OASert/actions)

# OASert

(OAS + Assert)
Utility library to validate payloads against a provided specification.

## Validation Use Cases

### Simple Specification Validation (no cache)

#### Summary

How to validate in-flight payloads against a single specification.

#### Example Cases

- Reverse proxies
- Sidecar deployments

#### Usage

```rust

use oasert::validator::OpenApiPayloadValidator;
use serde_json::json;
use http::Request;
fn main() {
    let incoming_request: Request<T> = ...;
    let my_spec: Value = ...;
    let validator = OpenApiPayloadValidator::new(my_spec).unwrap();
    let result = validator.validate_request(&incoming_request, None);
}
```

### Multiple Specification Validation (with cache)

#### Summary

// TODO

#### Example Cases

- Gateways
- Proxy Servers/Clients

#### Usage

// TODO

### Scope Validation (no cache)

#### Summary

// TODO

#### Example Cases

- Reverse proxies
- Sidecar deployments

#### Usage

// TODO

### Scope Validation (with cache)

#### Summary

// TODO

#### Example Cases

- Gateways
- Proxy Servers/Clients

#### Usage

// TODO

