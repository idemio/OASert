# OASert

**OASert** is a high-performance Rust library for runtime validation of HTTP requests against OpenAPI 3.x specifications. It provides a comprehensive suite of tools for traversing, validating, and caching OpenAPI document structures to
ensure strict compliance with defined API contracts during request processing.

[![codecov](https://codecov.io/gh/idemio/OASert/branch/main/graph/badge.svg)](https://codecov.io/gh/idemio/OASert)
[![CI Status](https://github.com/idemio/OASert/workflows/Rust/badge.svg)](https://github.com/idemio/OASert/actions)

---

## Core Capabilities

- **Comprehensive Request Validation**  
  Performs rigorous validation of HTTP request elements (payloads, headers, query parameters, path parameters) against OpenAPI v3.x specifications, ensuring complete compliance with defined schemas.

- **High-Performance Validator Caching**  
  Implements a thread-safe, concurrent caching infrastructure powered by `DashMap` (v7.0) that minimizes redundant validator instantiations and optimizes memory usage.

- **Advanced Specification Traversal**  
  Provides sophisticated algorithms for navigating complex OpenAPI documents, with robust handling of nested `$ref` references through pointer resolution and circular reference detection.

- **Validation Error Reporting**  
  Detailed error reporting with specific categories like missing properties, invalid types, or unsupported schema versions.

- **Supports OpenAPI Drafts**  
  Includes support for both OpenAPI 3.0.x (Draft 4) and OpenAPI 3.1.x (Draft 2020–12).

- **Supports Partial Validation**
  Allows for partial validation of requests (i.e. validate headers, validate scopes, validate body, etc.)

- **Runtime Agnostic**
  Does not depend on any specific runtime and can be dropped in where needed (i.e., hyper, aws lambda, etc.)

---

## Installation

Add `OASert` to your `Cargo.toml`:

```toml 
[dependencies]
oasert = "0.1.1"
``` 

---

## Basic Usage

### Initializing the Validator

1. Parse your OpenAPI specification into a `serde_json::Value`.
2. Create an `OpenApiPayloadValidator` using the parsed specification.
3. Pass incoming requests to the validator

See a full example using hyper [here](./examples/hyper-validation/main.rs)
See a full example using AWS Lambda [here](./examples/aws-lambda-http-validation/main.rs)

## Components

### 1. `ValidatorCache`

Efficient caching mechanism for validators to avoid repeated instantiations.

- Insert or retrieve validators dynamically.
- Clear the cache when needed.
- Automatically create validators for specific IDs if not cached.

### 2. `OpenApiTraverser`

Utility class to traverse OpenAPI specifications with support for:

- Resolving `$ref` pointers.
- Fetching required or optional specification nodes.
- Handling complex paths and parameters.

### 3. `OpenApiTypes`

Type mapping utility to convert OpenAPI types (`string`, `boolean`, etc.) into native Rust types.

### 4. Error Handling

Comprehensive error handling for:

- Missing parameters or fields.
- Unsupported specification versions.
- Invalid schema values or types.

