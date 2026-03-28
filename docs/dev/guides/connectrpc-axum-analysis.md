# connectrpc-axum Repository Analysis

Analysis of `/var/tddy/Code/tddy-coder/tmp/connectrpc-axum` for reuse in our ConnectRPC HTTP transport implementation. We have our own `RpcBridge`/`RpcService` trait and only need the HTTP transport layer.

---

## 1. Connect Protocol Handler (URL Parsing, Header Validation, Content-Type Negotiation)

### What It Does

The handler flow is split across several components:

1. **ConnectLayer** (`layer/connect.rs`) – runs first:
   - Pre-validates protocol (415 if unsupported content-type or GET encoding)
   - Builds `ConnectContext` from headers
   - Validates protocol requirements (version header, GET params)
   - Stores context in `req.extensions()`
   - Applies timeout via `tokio::time::timeout`

2. **Protocol detection** (`context/protocol.rs`):
   - POST: `Content-Type` → `RequestProtocol` (ConnectUnaryJson, ConnectUnaryProto, ConnectStreamJson, ConnectStreamProto, Unknown)
   - GET: `encoding` query param → json or proto

3. **Handler** (`handler.rs`) – runs after layer:
   - Reads `ConnectContext` from extensions
   - Validates unary vs streaming content-type for the handler type
   - Extracts `ConnectRequest<T>` (body parsing)
   - Calls user handler, then encodes response via context

### Key Patterns

```rust
// Protocol from Content-Type (POST)
pub fn from_content_type(content_type: &str) -> Self {
    if content_type.starts_with("application/connect+proto") => ConnectStreamProto,
    else if content_type.starts_with("application/connect+json") => ConnectStreamJson,
    else if content_type.starts_with("application/proto") => ConnectUnaryProto,
    else if content_type.starts_with("application/json") => ConnectUnaryJson,
    else => Unknown,
}

// Protocol from GET query (encoding param)
// encoding=proto → ConnectUnaryProto, else → ConnectUnaryJson
```

```rust
// Pre-protocol check (415 before context creation)
fn check_protocol_negotiation<B>(req: &Request<B>) -> Option<ProtocolNegotiationError> {
    if *req.method() == Method::GET {
        if !can_handle_get_encoding(req) { return Some(UnsupportedMediaType); }
    } else if *req.method() == Method::POST {
        let protocol = detect_protocol(req);
        if !can_handle_content_type(protocol) { return Some(UnsupportedMediaType); }
    }
    None
}
```

### Relevant for Our Implementation

- **Context in extensions**: Store `ConnectContext` in `req.extensions()` so pipelines and handlers can use it.
- **Protocol enum**: `RequestProtocol` drives content-type, envelope framing, and error encoding.
- **415 handling**: Return 415 with `Accept-Post` header for unsupported media types.
- **GET support**: `connect=v1`, `encoding=json|proto`, `message=`, optional `base64=1`, `compression=`.

### Gotchas

- Unary vs streaming content-type must match handler type; mismatches return `Code::Unknown`.
- `Connect-Protocol-Version: 1` is optional by default; config can require it.

---

## 2. Streaming Envelope Framing (5-Byte Prefix)

### What It Does

Connect streaming uses per-message framing:

```
[flags:1][length:4][payload:length]
```

- **Flags**: 0x00 = uncompressed, 0x01 = compressed, 0x02 = EndStream
- **Length**: big-endian u32
- **Payload**: message bytes (or EndStream JSON)

### Key Code (`connectrpc-axum-core/src/envelope.rs`)

```rust
pub const ENVELOPE_HEADER_SIZE: usize = 5;

pub fn wrap_envelope(payload: &[u8], compressed: bool) -> Vec<u8> {
    let flags = if compressed { 0x01 } else { 0x00 };
    let mut frame = Vec::with_capacity(5 + payload.len());
    frame.push(flags);
    frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    frame.extend_from_slice(payload);
    frame
}

pub fn parse_envelope_header(data: &[u8]) -> Result<(u8, u32), EnvelopeError> {
    if data.len() < 5 { return Err(IncompleteHeader { expected: 5, actual: data.len() }); }
    let flags = data[0];
    let length = u32::from_be_bytes([data[1], data[2], data[3], data[4]]);
    Ok((flags, length))
}
```

### Streaming Request Parsing (`message/request.rs`)

```rust
// create_frame_stream - parses frames from body
while buffer.len() >= 5 {
    let flags = buffer[0];
    let length = u32::from_be_bytes([buffer[1], buffer[2], buffer[3], buffer[4]]) as usize;

    // Check size limit BEFORE allocating
    if let Err(err) = limits.check_size(length) { yield Err(...); return; }

    if buffer.len() < 5 + length { break; }  // Need more data

    let raw_payload = buffer.split_to(5 + length).split_off(5);
    let payload = process_envelope_payload(flags, raw_payload.freeze(), request_encoding)?;

    if let Ok(None) = payload { return; }  // EndStream
    // Decode and yield message
}
```

### Relevant for Our Implementation

- Reuse `wrap_envelope`, `parse_envelope_header`, `process_envelope_payload` (or equivalent).
- Enforce size limits before allocating for `length`.
- EndStream frame (0x02) has JSON payload: `{"error": {...}, "metadata": {...}}`.

### Gotchas

- `process_envelope_payload` returns `Ok(None)` for EndStream; caller must not treat it as a message.
- Invalid flags (e.g. 0xFF) must be rejected with `InvalidArgument`.

---

## 3. JSON Serialization of Protobuf Messages

### What It Does

Prost does not provide JSON (de)serialization. connectrpc-axum uses **pbjson**:

1. **Codegen** (`connectrpc-axum-build`): After prost, runs `pbjson-build` on the descriptor set to generate `Serialize`/`Deserialize` for protobuf types.
2. **Runtime**: Messages must implement both `prost::Message` and `serde::Serialize` (and `DeserializeOwned` for requests).

### Key Code

**Encode** (`message/response.rs`):

```rust
pub fn encode_json<T>(message: &T) -> Result<Vec<u8>, ConnectError>
where T: Serialize,
{
    serde_json::to_vec(message).map_err(|e| ConnectError::new(Code::Internal, format!("failed to encode JSON: {e}")))
}
```

**Decode** (`message/request.rs`):

```rust
pub fn decode_json<T>(bytes: &[u8]) -> Result<T, ConnectError>
where T: DeserializeOwned,
{
    serde_json::from_slice(bytes).map_err(|e| ConnectError::new(Code::InvalidArgument, format!("failed to decode JSON: {e}")))
}
```

**Build** (`connectrpc-axum-build/src/lib.rs`):

```rust
// Pass 1.5: pbjson serde implementations (always)
Self::generate_pbjson(&out_dir, &descriptor_path, self.pbjson_config.as_ref())?;

fn generate_pbjson(...) {
    let mut pbjson_builder = pbjson_build::Builder::new();
    pbjson_builder.out_dir(out_dir);
    if let Some(config_fn) = pbjson_config { config_fn(&mut pbjson_builder); }
    pbjson_builder.register_descriptors(&descriptor_bytes)?.build(&["."])?;
    // Append .serde.rs content into main .rs files
}
```

### Relevant for Our Implementation

- Use **pbjson** + **pbjson-build** for protobuf ↔ JSON.
- Ensure `extern_path` for `google.protobuf` → `::pbjson_types` in both prost and pbjson config.
- Handler types: `Req: Message + DeserializeOwned + Default`, `Resp: Message + Serialize`.

### Gotchas

- Connect JSON uses standard protobuf JSON mapping (camelCase, etc.); pbjson follows that.
- If you generate types manually, you must also generate pbjson serde impls.

---

## 4. Error Serialization to Connect Error JSON Format

### What It Does

Connect errors use a JSON body:

```json
{
  "code": "not_found",
  "message": "resource not found",
  "details": [
    {"type": "google.rpc.RetryInfo", "value": "base64-encoded-protobuf"}
  ]
}
```

### Key Code (`connectrpc-axum-core/src/error.rs`)

**Code enum** (snake_case for JSON):

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Code {
    Ok, Canceled, Unknown, InvalidArgument, DeadlineExceeded,
    NotFound, AlreadyExists, PermissionDenied, ResourceExhausted,
    FailedPrecondition, Aborted, OutOfRange, Unimplemented,
    Internal, Unavailable, DataLoss, Unauthenticated,
}
```

**ErrorDetail** (strips `type.googleapis.com/` prefix, base64 no-pad):

```rust
impl Serialize for ErrorDetail {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let type_name = self.type_url.strip_prefix("type.googleapis.com/").unwrap_or(&self.type_url);
        let encoded = base64::engine::general_purpose::STANDARD_NO_PAD.encode(&self.value);
        // serialize_struct with "type" and "value" fields
    }
}
```

**Status** delegates to `ErrorResponseBody`:

```rust
impl Serialize for Status {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        ErrorResponseBody { code: self.code, message: self.message.clone(), details: self.details.clone() }
            .serialize(serializer)
    }
}
```

### Relevant for Our Implementation

- Use `#[serde(rename_all = "snake_case")]` for `Code`.
- Use `STANDARD_NO_PAD` for detail values.
- Strip `type.googleapis.com/` from type URLs in details.
- `skip_serializing_if` for optional `message` and empty `details`.

---

## 5. Code ↔ HTTP Status Mapping

### Key Code (`message/error.rs`)

```rust
fn http_status_code(&self) -> StatusCode {
    match self.inner.code() {
        Code::Ok => StatusCode::OK,
        Code::Canceled => StatusCode::from_u16(499).unwrap(),  // Client Closed Request
        Code::Unknown => StatusCode::INTERNAL_SERVER_ERROR,
        Code::InvalidArgument => StatusCode::BAD_REQUEST,
        Code::DeadlineExceeded => StatusCode::GATEWAY_TIMEOUT,
        Code::NotFound => StatusCode::NOT_FOUND,
        Code::AlreadyExists => StatusCode::CONFLICT,
        Code::PermissionDenied => StatusCode::FORBIDDEN,
        Code::ResourceExhausted => StatusCode::TOO_MANY_REQUESTS,
        Code::FailedPrecondition => StatusCode::BAD_REQUEST,
        Code::Aborted => StatusCode::CONFLICT,
        Code::OutOfRange => StatusCode::BAD_REQUEST,
        Code::Unimplemented => StatusCode::NOT_IMPLEMENTED,
        Code::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        Code::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
        Code::DataLoss => StatusCode::INTERNAL_SERVER_ERROR,
        Code::Unauthenticated => StatusCode::UNAUTHORIZED,
    }
}
```

**Streaming**: Errors are sent in EndStream frames with HTTP 200, not as HTTP error status codes.

**Reverse mapping** (HTTP → Code):

```rust
pub fn code_from_status(status: StatusCode) -> Code {
    match status {
        StatusCode::OK => Code::Ok,
        StatusCode::BAD_REQUEST => Code::InvalidArgument,
        StatusCode::UNAUTHORIZED => Code::Unauthenticated,
        StatusCode::FORBIDDEN => Code::PermissionDenied,
        StatusCode::NOT_FOUND => Code::NotFound,
        StatusCode::CONFLICT => Code::AlreadyExists,
        StatusCode::REQUEST_TIMEOUT => Code::DeadlineExceeded,
        StatusCode::TOO_MANY_REQUESTS => Code::ResourceExhausted,
        StatusCode::NOT_IMPLEMENTED => Code::Unimplemented,
        StatusCode::SERVICE_UNAVAILABLE => Code::Unavailable,
        StatusCode::INTERNAL_SERVER_ERROR => Code::Internal,
        _ => Code::Unknown,
    }
}
```

### Relevant for Our Implementation

- Use this mapping for unary error responses.
- For streaming, always return 200 and put errors in the EndStream frame.

---

## 6. Axum Router Construction

### What It Does

`MakeServiceBuilder` composes:

1. Connect routers (with ConnectLayer)
2. Optional axum routers (bypass Connect, get compression/timeout)
3. Optional raw axum routers (no shared layers)
4. Optional gRPC services (ContentTypeSwitch)

### Layer Order (outer → inner)

```
ConnectRouter
  .layer(ConnectLayer)           // Protocol detection, context, timeout
  .layer(CompressionLayer)      // Unary response compression (optional)
  .layer(RequestDecompressionLayer)  // Unary request decompression (optional)
  .layer(BridgeLayer)           // Streaming: override Accept-Encoding to identity; Content-Length limit
```

**BridgeLayer** (`layer/bridge.rs`):

- For `application/connect+*`: set `Accept-Encoding: identity` so Tower does not compress the streaming body (streaming uses per-envelope compression).
- Enforce `Content-Length` vs `receive_max_bytes` before decompression.

### Key Code

```rust
// service_builder.rs - build_connect_and_axum_router
let bridge_layer = BridgeLayer::with_receive_limit(receive_max_bytes);
let mut router = connect_router.layer(layers.connect_layer);
if let (Some(comp), Some(decomp)) = (&layers.compression_layer, &layers.decompression_layer) {
    router = router.layer(comp.clone()).layer(decomp.clone());
}
router = router.layer(bridge_layer);
```

### Route Registration

```rust
// Handlers use post_connect(handler) or get_connect(handler)
Router::new()
    .route("/hello.HelloWorldService/SayHello", post_connect(say_hello).get_connect(say_hello))
```

Connect path format: `/{package}.{Service}/{Method}` (e.g. `/hello.HelloWorldService/SayHello`).

### Relevant for Our Implementation

- Use a similar layer stack if you integrate with Tower compression.
- BridgeLayer is required when mixing Tower compression and Connect streaming.
- Route pattern: `/{full_service_name}/{method_name}`.

---

## File-by-File Summary

| File | Purpose | Reuse |
|------|---------|-------|
| `connectrpc-axum-core/envelope.rs` | 5-byte framing, wrap/parse | High – framing logic |
| `connectrpc-axum-core/error.rs` | Code, Status, ErrorDetail, serialization | High – error types |
| `connectrpc-axum-core/codec.rs` | Compression codec trait (gzip, etc.) | Medium – if you need streaming compression |
| `connectrpc-axum-core/compression.rs` | CompressionEncoding, negotiation | Medium |
| `context/protocol.rs` | RequestProtocol, detect_protocol, validation | High – protocol detection |
| `context/timeout.rs` | Connect-Timeout-Ms parsing | Low – straightforward |
| `context/envelope_compression.rs` | Connect-Content-Encoding, Connect-Accept-Encoding | Medium – for streaming |
| `layer/connect.rs` | ConnectLayer, context building, timeout | High – pattern |
| `layer/bridge.rs` | BridgeLayer for streaming + compression | High – if using Tower compression |
| `message/request.rs` | RequestPipeline, ConnectRequest, frame stream | High – request parsing |
| `message/response.rs` | ResponsePipeline, ConnectResponse, StreamBody | High – response encoding |
| `message/error.rs` | ConnectError, HTTP mapping, EndStream | High – error handling |
| `handler.rs` | ConnectHandlerWrapper, post_connect, get_connect | Medium – adapt to your RpcService |
| `service_builder.rs` | MakeServiceBuilder | Low – you have your own builder |

---

## Build/Codegen (connectrpc-axum-build)

- **Prost** for Rust types and service traits.
- **pbjson-build** for `Serialize`/`Deserialize` on protobuf types.
- Custom service generator for Connect handlers.
- Optional tonic integration for gRPC.

For our case: we can reuse the pbjson approach and possibly the envelope/error types; the service generator is less relevant if we use our own RpcBridge.

---

## Test Patterns (connectrpc-axum-test)

- **Dual implementation**: Rust server + Go server, both tested with Rust and Go clients.
- **Unix socket** for tests (`TestSocket`).
- **Per-scenario modules**: e.g. `connect_unary`, `connect_server_stream`, `streaming_error`.
- **HTTP/1.1** via `hyper` + `http_body_util` for clients.

---

## Gotchas and Issues

1. **Debug logging**: `message/response.rs` line 352 has `eprintln!` for streaming; remove for production.
2. **Streaming errors**: Always HTTP 200; errors go in EndStream frame.
3. **Unary errors**: Always JSON, even for `application/proto` requests.
4. **GET**: Requires `encoding` and `message`; `connect=v1` optional unless configured.
5. **BridgeLayer order**: Must sit between compression layers and Connect handlers when both are used.
