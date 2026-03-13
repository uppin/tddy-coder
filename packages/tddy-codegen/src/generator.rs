//! Service generator implementations.

use prost_build::{Method, Service, ServiceGenerator};
use std::fmt::Write;

use crate::config::TddyServiceGenerator;

impl ServiceGenerator for TddyServiceGenerator {
    fn generate(&mut self, service: Service, buf: &mut String) {
        let rpc = &self.rpc_crate_path;

        // Trait imports and definition
        writeln!(buf, "use async_trait::async_trait;").unwrap();
        writeln!(buf, "use futures_util::Stream;").unwrap();
        writeln!(buf).unwrap();

        writeln!(
            buf,
            "/// Generated async service trait for {} (tonic-mirrored signatures).",
            service.name
        )
        .unwrap();
        writeln!(buf, "#[async_trait]").unwrap();
        writeln!(buf, "pub trait {}: Send + Sync + 'static {{", service.name).unwrap();

        for method in &service.methods {
            generate_trait_method(method, buf, rpc);
        }

        writeln!(buf, "}}").unwrap();
        writeln!(buf).unwrap();

        if self.generate_rpc_server {
            generate_server_struct(&service, buf, rpc);
        }

        if self.generate_tonic_adapter {
            generate_tonic_adapter(&service, buf, rpc);
        }
    }

    fn finalize_package(&mut self, _package: &str, _buf: &mut String) {}
}

/// Legacy generator for backward compatibility (old Vec/mpsc::Receiver trait).
/// Prefer TddyServiceGenerator for new code.
pub struct LiveKitServiceGenerator;

impl ServiceGenerator for LiveKitServiceGenerator {
    fn generate(&mut self, service: Service, buf: &mut String) {
        writeln!(buf, "use async_trait::async_trait;").unwrap();
        writeln!(buf, "use tokio::sync::mpsc;").unwrap();
        writeln!(buf).unwrap();

        writeln!(
            buf,
            "/// Generated async service trait for {} (legacy signatures)",
            service.name
        )
        .unwrap();
        writeln!(buf, "#[async_trait]").unwrap();
        writeln!(buf, "pub trait {}: Send + Sync + 'static {{", service.name).unwrap();

        for method in &service.methods {
            generate_legacy_method(method, buf);
        }

        writeln!(buf, "}}").unwrap();
        writeln!(buf).unwrap();
    }

    fn finalize_package(&mut self, _package: &str, _buf: &mut String) {}
}

fn generate_trait_method(method: &Method, buf: &mut String, rpc: &str) {
    let method_snake = to_snake_case(&method.name);
    let input = &method.input_type;
    let output = &method.output_type;

    writeln!(buf, "    /// RPC method: {}", method.name).unwrap();

    match (method.client_streaming, method.server_streaming) {
        (false, false) => {
            writeln!(
                buf,
                "    async fn {}(&self, request: {}::Request<{}>) -> Result<{}::Response<{}>, {}::Status>;",
                method_snake, rpc, input, rpc, output, rpc
            )
            .unwrap();
        }
        (false, true) => {
            let stream_assoc = format!("{}Stream", to_pascal_case(&method.name));
            writeln!(
                buf,
                "    type {}: Stream<Item = Result<{}, {}::Status>> + Send + Unpin;",
                stream_assoc, output, rpc
            )
            .unwrap();
            writeln!(
                buf,
                "    async fn {}(&self, request: {}::Request<{}>) -> Result<{}::Response<Self::{}>, {}::Status>;",
                method_snake, rpc, input, rpc, stream_assoc, rpc
            )
            .unwrap();
        }
        (true, false) => {
            writeln!(
                buf,
                "    async fn {}(&self, request: {}::Request<{}::Streaming<{}>>) -> Result<{}::Response<{}>, {}::Status>;",
                method_snake, rpc, rpc, input, rpc, output, rpc
            )
            .unwrap();
        }
        (true, true) => {
            let stream_assoc = format!("{}Stream", to_pascal_case(&method.name));
            writeln!(
                buf,
                "    type {}: Stream<Item = Result<{}, {}::Status>> + Send + Unpin;",
                stream_assoc, output, rpc
            )
            .unwrap();
            writeln!(
                buf,
                "    async fn {}(&self, request: {}::Request<{}::Streaming<{}>>) -> Result<{}::Response<Self::{}>, {}::Status>;",
                method_snake, rpc, rpc, input, rpc, stream_assoc, rpc
            )
            .unwrap();
        }
    }

    writeln!(buf).unwrap();
}

fn generate_legacy_method(method: &Method, buf: &mut String) {
    let method_snake = to_snake_case(&method.name);
    let input = &method.input_type;
    let output = &method.output_type;

    writeln!(buf, "    /// RPC method: {}", method.name).unwrap();

    match (method.client_streaming, method.server_streaming) {
        (false, false) => {
            writeln!(
                buf,
                "    async fn {}(&self, request: {}) -> Result<{}, tddy_rpc::Status>;",
                method_snake, input, output
            )
            .unwrap();
        }
        (false, true) => {
            writeln!(
                buf,
                "    async fn {}(&self, request: {}) -> Result<mpsc::Receiver<Result<{}, tddy_rpc::Status>>, tddy_rpc::Status>;",
                method_snake, input, output
            )
            .unwrap();
        }
        (true, false) => {
            writeln!(
                buf,
                "    async fn {}(&self, requests: Vec<{}>) -> Result<{}, tddy_rpc::Status>;",
                method_snake, input, output
            )
            .unwrap();
        }
        (true, true) => {
            writeln!(
                buf,
                "    async fn {}(&self, requests: Vec<{}>) -> Result<mpsc::Receiver<Result<{}, tddy_rpc::Status>>, tddy_rpc::Status>;",
                method_snake, input, output
            )
            .unwrap();
        }
    }

    writeln!(buf).unwrap();
}

fn generate_server_struct(service: &Service, buf: &mut String, rpc: &str) {
    let server_name = format!("{}Server", service.name);
    let service_qualified = format!("{}.{}", service.package, service.name);

    writeln!(buf, "use std::sync::Arc;").unwrap();
    writeln!(buf, "use futures_util::StreamExt;").unwrap();
    writeln!(buf, "use tokio::sync::mpsc;").unwrap();
    writeln!(buf, "use prost::Message;").unwrap();
    writeln!(buf).unwrap();

    writeln!(buf, "/// Generated RpcService server for {}.", service.name).unwrap();
    writeln!(buf, "pub struct {}<T: {}> {{", server_name, service.name).unwrap();
    writeln!(buf, "    inner: Arc<T>,").unwrap();
    writeln!(buf, "}}").unwrap();
    writeln!(buf).unwrap();

    writeln!(buf, "impl<T: {}> {}<T> {{", service.name, server_name).unwrap();
    writeln!(
        buf,
        "    pub const NAME: &'static str = \"{}\";",
        service_qualified
    )
    .unwrap();
    writeln!(buf).unwrap();
    writeln!(buf, "    pub fn new(inner: T) -> Self {{").unwrap();
    writeln!(buf, "        Self {{ inner: Arc::new(inner) }}").unwrap();
    writeln!(buf, "    }}").unwrap();
    writeln!(buf, "}}").unwrap();
    writeln!(buf).unwrap();

    // Per-method structs and server RpcService impl
    for method in &service.methods {
        generate_per_method_struct(service, method, buf, rpc);
    }

    generate_rpc_service_impl(service, buf, rpc);
}

fn generate_per_method_struct(service: &Service, method: &Method, buf: &mut String, rpc: &str) {
    let svc_name = format!("{}Svc", to_pascal_case(&method.name));
    let method_snake = to_snake_case(&method.name);
    let input = &method.input_type;

    writeln!(buf, "struct {}<T: {}>(Arc<T>);", svc_name, service.name).unwrap();

    match (method.client_streaming, method.server_streaming) {
        (false, false) => {
            writeln!(buf, "impl<T: {}> {}<T> {{", service.name, svc_name).unwrap();
            writeln!(
                buf,
                "    async fn call(&self, message: &{}::RpcMessage) -> {}::RpcResult {{",
                rpc, rpc
            )
            .unwrap();
            writeln!(
                buf,
                "        let req = match {}::decode(&message.payload[..]) {{",
                input
            )
            .unwrap();
            writeln!(buf, "            Ok(r) => r,").unwrap();
            writeln!(
                buf,
                "            Err(e) => return {}::RpcResult::Unary(Err({}::Status::invalid_argument(e.to_string()))),",
                rpc, rpc
            )
            .unwrap();
            writeln!(buf, "        }};").unwrap();
            writeln!(
                buf,
                "        let request = {}::Request::from_rpc_message(req, message);",
                rpc
            )
            .unwrap();
            writeln!(
                buf,
                "        match self.0.{}(request).await {{",
                method_snake
            )
            .unwrap();
            writeln!(
                buf,
                "            Ok(resp) => {}::RpcResult::Unary(Ok(resp.into_inner().encode_to_vec())),",
                rpc
            )
            .unwrap();
            writeln!(
                buf,
                "            Err(e) => {}::RpcResult::Unary(Err(e)),",
                rpc
            )
            .unwrap();
            writeln!(buf, "        }}").unwrap();
            writeln!(buf, "    }}").unwrap();
            writeln!(buf, "}}").unwrap();
        }
        (false, true) => {
            writeln!(buf, "impl<T: {}> {}<T> {{", service.name, svc_name).unwrap();
            writeln!(
                buf,
                "    async fn call(&self, message: &{}::RpcMessage) -> {}::RpcResult {{",
                rpc, rpc
            )
            .unwrap();
            writeln!(
                buf,
                "        let req = match {}::decode(&message.payload[..]) {{",
                input
            )
            .unwrap();
            writeln!(buf, "            Ok(r) => r,").unwrap();
            writeln!(
                buf,
                "            Err(e) => return {}::RpcResult::ServerStream(Err({}::Status::invalid_argument(e.to_string()))),",
                rpc, rpc
            )
            .unwrap();
            writeln!(buf, "        }};").unwrap();
            writeln!(
                buf,
                "        let request = {}::Request::from_rpc_message(req, message);",
                rpc
            )
            .unwrap();
            writeln!(
                buf,
                "        match self.0.{}(request).await {{",
                method_snake
            )
            .unwrap();
            writeln!(buf, "            Ok(resp) => {{",).unwrap();
            writeln!(buf, "                let stream = resp.into_inner();",).unwrap();
            writeln!(buf, "                let (tx, rx) = mpsc::channel(16);",).unwrap();
            writeln!(buf, "                tokio::spawn(async move {{",).unwrap();
            writeln!(buf, "                    let mut stream = stream;",).unwrap();
            writeln!(
                buf,
                "                    while let Some(item) = stream.next().await {{",
            )
            .unwrap();
            writeln!(
                buf,
                "                        let _ = tx.send(item.map(|r| r.encode_to_vec())).await;",
            )
            .unwrap();
            writeln!(buf, "                    }}",).unwrap();
            writeln!(buf, "                }});",).unwrap();
            writeln!(
                buf,
                "                {}::RpcResult::ServerStream(Ok(rx))",
                rpc
            )
            .unwrap();
            writeln!(buf, "            }}",).unwrap();
            writeln!(
                buf,
                "            Err(e) => {}::RpcResult::ServerStream(Err(e)),",
                rpc
            )
            .unwrap();
            writeln!(buf, "        }}").unwrap();
            writeln!(buf, "    }}").unwrap();
            writeln!(buf, "}}").unwrap();
        }
        (true, false) => {
            writeln!(buf, "impl<T: {}> {}<T> {{", service.name, svc_name).unwrap();
            writeln!(
                buf,
                "    async fn call(&self, messages: &[{}::RpcMessage]) -> {}::RpcResult {{",
                rpc, rpc
            )
            .unwrap();
            writeln!(buf, "        let decoded: Vec<_> = match messages",).unwrap();
            writeln!(buf, "            .iter()",).unwrap();
            writeln!(buf, "            .filter(|m| !m.payload.is_empty())",).unwrap();
            writeln!(
                buf,
                "            .map(|m| {}::decode(&m.payload[..]).map_err(|e| {}::Status::invalid_argument(e.to_string())))",
                input, rpc
            )
            .unwrap();
            writeln!(buf, "            .collect::<Result<_, _>>() {{",).unwrap();
            writeln!(buf, "                Ok(d) => d,",).unwrap();
            writeln!(
                buf,
                "                Err(e) => return {}::RpcResult::Unary(Err(e)),",
                rpc
            )
            .unwrap();
            writeln!(buf, "            }};",).unwrap();
            writeln!(buf, "        let first = messages.first().unwrap();",).unwrap();
            writeln!(
                buf,
                "        let streaming = {}::Streaming::new(futures_util::stream::iter(decoded.into_iter().map(Ok)));",
                rpc
            )
            .unwrap();
            writeln!(
                buf,
                "        let request = {}::Request::from_rpc_message(streaming, first);",
                rpc
            )
            .unwrap();
            writeln!(
                buf,
                "        match self.0.{}(request).await {{",
                method_snake
            )
            .unwrap();
            writeln!(
                buf,
                "            Ok(resp) => {}::RpcResult::Unary(Ok(resp.into_inner().encode_to_vec())),",
                rpc
            )
            .unwrap();
            writeln!(
                buf,
                "            Err(e) => {}::RpcResult::Unary(Err(e)),",
                rpc
            )
            .unwrap();
            writeln!(buf, "        }}").unwrap();
            writeln!(buf, "    }}").unwrap();
            writeln!(buf, "}}").unwrap();
        }
        (true, true) => {
            writeln!(buf, "impl<T: {}> {}<T> {{", service.name, svc_name).unwrap();
            writeln!(
                buf,
                "    async fn call(&self, messages: &[{}::RpcMessage]) -> {}::RpcResult {{",
                rpc, rpc
            )
            .unwrap();
            writeln!(buf, "        let decoded: Vec<_> = match messages",).unwrap();
            writeln!(buf, "            .iter()",).unwrap();
            writeln!(buf, "            .filter(|m| !m.payload.is_empty())",).unwrap();
            writeln!(
                buf,
                "            .map(|m| {}::decode(&m.payload[..]).map_err(|e| {}::Status::invalid_argument(e.to_string())))",
                input, rpc
            )
            .unwrap();
            writeln!(buf, "            .collect::<Result<_, _>>() {{",).unwrap();
            writeln!(buf, "                Ok(d) => d,",).unwrap();
            writeln!(
                buf,
                "                Err(e) => return {}::RpcResult::ServerStream(Err(e)),",
                rpc
            )
            .unwrap();
            writeln!(buf, "            }};",).unwrap();
            writeln!(buf, "        let first = messages.first().unwrap();",).unwrap();
            writeln!(
                buf,
                "        let streaming = {}::Streaming::new(futures_util::stream::iter(decoded.into_iter().map(Ok)));",
                rpc
            )
            .unwrap();
            writeln!(
                buf,
                "        let request = {}::Request::from_rpc_message(streaming, first);",
                rpc
            )
            .unwrap();
            writeln!(
                buf,
                "        match self.0.{}(request).await {{",
                method_snake
            )
            .unwrap();
            writeln!(buf, "            Ok(resp) => {{",).unwrap();
            writeln!(buf, "                let stream = resp.into_inner();",).unwrap();
            writeln!(buf, "                let (tx, rx) = mpsc::channel(16);",).unwrap();
            writeln!(buf, "                tokio::spawn(async move {{",).unwrap();
            writeln!(buf, "                    let mut stream = stream;",).unwrap();
            writeln!(
                buf,
                "                    while let Some(item) = stream.next().await {{",
            )
            .unwrap();
            writeln!(
                buf,
                "                        let _ = tx.send(item.map(|r| r.encode_to_vec())).await;",
            )
            .unwrap();
            writeln!(buf, "                    }}",).unwrap();
            writeln!(buf, "                }});",).unwrap();
            writeln!(
                buf,
                "                {}::RpcResult::ServerStream(Ok(rx))",
                rpc
            )
            .unwrap();
            writeln!(buf, "            }}",).unwrap();
            writeln!(
                buf,
                "            Err(e) => {}::RpcResult::ServerStream(Err(e)),",
                rpc
            )
            .unwrap();
            writeln!(buf, "        }}").unwrap();
            writeln!(buf, "    }}").unwrap();
            writeln!(buf, "}}").unwrap();
        }
    }
    writeln!(buf).unwrap();
}

fn generate_rpc_service_impl(service: &Service, buf: &mut String, rpc: &str) {
    let server_name = format!("{}Server", service.name);

    let bidi_methods: Vec<_> = service
        .methods
        .iter()
        .filter(|m| m.client_streaming && m.server_streaming)
        .map(method_proto_name)
        .collect();

    writeln!(buf, "#[async_trait]").unwrap();
    writeln!(
        buf,
        "impl<T: {}> {}::RpcService for {}<T> {{",
        service.name, rpc, server_name
    )
    .unwrap();
    writeln!(
        buf,
        "    fn is_bidi_stream(&self, _service: &str, method: &str) -> bool {{"
    )
    .unwrap();
    if bidi_methods.is_empty() {
        writeln!(buf, "        false").unwrap();
    } else {
        writeln!(
            buf,
            "        matches!(method, {})",
            bidi_matches_arms(&bidi_methods)
        )
        .unwrap();
    }
    writeln!(buf, "    }}").unwrap();
    writeln!(buf).unwrap();
    writeln!(
        buf,
        "    async fn handle_rpc(&self, _service: &str, method: &str, message: &{}::RpcMessage) -> {}::RpcResult {{",
        rpc, rpc
    )
    .unwrap();
    let unary_methods: Vec<_> = service
        .methods
        .iter()
        .filter(|m| !m.client_streaming)
        .collect();
    if unary_methods.is_empty() {
        writeln!(buf, "        let _ = message;").unwrap();
        writeln!(
            buf,
            "        {}::RpcResult::Unary(Err({}::Status::not_found(format!(\"Unknown method: {{}}\", method))))",
            rpc, rpc
        )
        .unwrap();
    } else {
        writeln!(buf, "        match method {{").unwrap();
        for method in unary_methods {
            let svc_name = format!("{}Svc", to_pascal_case(&method.name));
            let proto_name = method_proto_name(method);
            writeln!(
                buf,
                "            \"{}\" => {}::<T>(self.inner.clone()).call(message).await,",
                proto_name, svc_name
            )
            .unwrap();
        }
        writeln!(
            buf,
            "            _ => {}::RpcResult::Unary(Err({}::Status::not_found(format!(\"Unknown method: {{}}\", method)))),",
            rpc, rpc
        )
        .unwrap();
        writeln!(buf, "        }}").unwrap();
    }
    writeln!(buf, "    }}").unwrap();
    writeln!(buf).unwrap();
    writeln!(
        buf,
        "    async fn handle_rpc_stream(&self, service: &str, method: &str, messages: &[{}::RpcMessage]) -> {}::RpcResult {{",
        rpc, rpc
    )
    .unwrap();
    writeln!(buf, "        match method {{").unwrap();
    for method in &service.methods {
        if method.client_streaming {
            let svc_name = format!("{}Svc", to_pascal_case(&method.name));
            let proto_name = method_proto_name(method);
            writeln!(
                buf,
                "            \"{}\" => {}::<T>(self.inner.clone()).call(messages).await,",
                proto_name, svc_name
            )
            .unwrap();
        }
    }
    writeln!(
        buf,
        "            _ if messages.len() == 1 => self.handle_rpc(service, method, &messages[0]).await,",
    )
    .unwrap();
    writeln!(
        buf,
        "            _ => {}::RpcResult::Unary(Err({}::Status::unimplemented(\"streaming not supported\"))),",
        rpc, rpc
    )
    .unwrap();
    writeln!(buf, "        }}").unwrap();
    writeln!(buf, "    }}").unwrap();
    writeln!(buf, "}}").unwrap();
}

fn bidi_matches_arms(methods: &[String]) -> String {
    if methods.is_empty() {
        return "\"\"".to_string();
    }
    let arms: Vec<String> = methods.iter().map(|m| format!("\"{}\"", m)).collect();
    arms.join(" | ")
}

fn method_proto_name(method: &Method) -> String {
    // prost Method.name is Rust snake_case; proto_name is the original.
    // Use proto_name if it looks like PascalCase, else convert name.
    if method
        .proto_name
        .chars()
        .next()
        .map(|c| c.is_uppercase())
        .unwrap_or(false)
    {
        method.proto_name.clone()
    } else {
        to_pascal_case(&method.name)
    }
}

fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    for (i, ch) in s.chars().enumerate() {
        if ch.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.push(ch.to_lowercase().next().unwrap());
        } else {
            result.push(ch);
        }
    }
    result
}

fn to_pascal_case(s: &str) -> String {
    let mut result = String::new();
    let mut capitalize = true;
    for ch in s.chars() {
        if ch == '_' {
            capitalize = true;
        } else if capitalize {
            result.push(ch.to_uppercase().next().unwrap());
            capitalize = false;
        } else {
            result.push(ch.to_lowercase().next().unwrap());
        }
    }
    result
}

/// Generate tonic adapter struct (feature-gated).
/// Wraps a service impl for use with tonic gRPC server.
/// TODO: Full impl of tonic server trait requires consumer to compile proto with tonic-build.
#[cfg(feature = "tonic")]
fn generate_tonic_adapter(service: &Service, buf: &mut String, _rpc: &str) {
    let adapter_name = format!("{}TonicAdapter", service.name);

    writeln!(buf).unwrap();
    writeln!(
        buf,
        "/// Adapter to use {} with tonic gRPC server.",
        service.name
    )
    .unwrap();
    writeln!(
        buf,
        "/// Requires tddy-rpc with `tonic` feature for type conversions.",
    )
    .unwrap();
    writeln!(buf, "#[allow(dead_code)]").unwrap();
    writeln!(buf, "pub struct {}<T: {}> {{", adapter_name, service.name).unwrap();
    writeln!(buf, "    inner: std::sync::Arc<T>,").unwrap();
    writeln!(buf, "}}").unwrap();
    writeln!(buf).unwrap();
    writeln!(buf, "impl<T: {}> {}<T> {{", service.name, adapter_name).unwrap();
    writeln!(buf, "    pub fn new(inner: T) -> Self {{",).unwrap();
    writeln!(buf, "        Self {{ inner: std::sync::Arc::new(inner) }}",).unwrap();
    writeln!(buf, "    }}").unwrap();
    writeln!(buf, "}}").unwrap();
}

#[cfg(not(feature = "tonic"))]
fn generate_tonic_adapter(_service: &Service, _buf: &mut String, _rpc: &str) {}
