//! Service generator implementations.

use prost_build::{Method, Service, ServiceGenerator};
use std::fmt::Write;

use crate::config::TddyServiceGenerator;

impl ServiceGenerator for TddyServiceGenerator {
    fn generate(&mut self, service: Service, buf: &mut String) {
        let rpc = &self.rpc_crate_path;

        // Trait imports and definition
        import_once(buf, "use async_trait::async_trait;");
        let needs_stream_trait = service.methods.iter().any(|m| m.server_streaming);
        if needs_stream_trait {
            import_once(buf, "use futures_util::Stream;");
        }
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
            match self.tonic_trait_path.as_deref() {
                Some(trait_module) => {
                    // The impl reaches the trait through tonic-build's `<service>_server` module,
                    // whose name is the path's final segment.
                    import_once(buf, &format!("use {trait_module};"));
                    generate_tonic_adapter(&service, buf, rpc);
                }
                None => generate_tonic_adapter_struct(&service, buf),
            }
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

    let needs_stream_ext = service.methods.iter().any(|m| m.server_streaming);
    let needs_mpsc = service.methods.iter().any(|m| m.server_streaming)
        || service
            .methods
            .iter()
            .any(|m| m.client_streaming && m.server_streaming);

    import_once(buf, "use std::sync::Arc;");
    if needs_stream_ext {
        import_once(buf, "use futures_util::StreamExt;");
    }
    if needs_mpsc {
        import_once(buf, "use tokio::sync::mpsc;");
    }
    import_once(buf, "use prost::Message;");
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
    writeln!(buf).unwrap();
    writeln!(
        buf,
        "    /// Build from an already-shared handle, so the same implementation instance can be"
    )
    .unwrap();
    writeln!(
        buf,
        "    /// served over more than one transport (e.g. LiveKit and a local Unix socket)."
    )
    .unwrap();
    writeln!(buf, "    pub fn from_arc(inner: Arc<T>) -> Self {{").unwrap();
    writeln!(buf, "        Self {{ inner }}").unwrap();
    writeln!(buf, "    }}").unwrap();
    writeln!(buf, "}}").unwrap();
    writeln!(buf).unwrap();

    // Per-method structs and server RpcService impl
    for method in &service.methods {
        generate_per_method_struct(service, method, buf, rpc);
    }

    generate_rpc_service_impl(service, buf, rpc);
}

/// Emit the body of a response pump's `while let Some(item) = stream.next().await` loop, indented
/// by `indent` spaces.
///
/// The send is *not* ignored: the receiving end belongs to the transport, which drops it as soon as
/// the peer that made the call is gone. Ignoring the error would leave this loop pulling items into
/// a void forever, and — because it holds the handler's stream alive — the handler's own
/// "my subscriber left" check could never fire either, so a polling handler would go on polling for
/// the life of the process. Breaking here propagates the teardown one hop further back.
///
/// (The emitted code carries no comment of its own: the generated file is formatted by a
/// token-level pretty-printer, which drops comments.)
fn write_stream_pump_send(buf: &mut String, indent: usize) {
    let pad = " ".repeat(indent);
    writeln!(
        buf,
        "{pad}if tx.send(item.map(|r| r.encode_to_vec())).await.is_err() {{"
    )
    .unwrap();
    writeln!(buf, "{pad}    break;").unwrap();
    writeln!(buf, "{pad}}}").unwrap();
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
            writeln!(buf, "                let (tx, rx) = mpsc::channel(256);",).unwrap();
            writeln!(buf, "                tokio::spawn(async move {{",).unwrap();
            writeln!(buf, "                    let mut stream = stream;",).unwrap();
            writeln!(
                buf,
                "                    while let Some(item) = stream.next().await {{",
            )
            .unwrap();
            write_stream_pump_send(buf, 24);
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
            writeln!(buf, "                let (tx, rx) = mpsc::channel(256);",).unwrap();
            writeln!(buf, "                tokio::spawn(async move {{",).unwrap();
            writeln!(buf, "                    let mut stream = stream;",).unwrap();
            writeln!(
                buf,
                "                    while let Some(item) = stream.next().await {{",
            )
            .unwrap();
            write_stream_pump_send(buf, 24);
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
    if bidi_methods.is_empty() {
        writeln!(
            buf,
            "    fn is_bidi_stream(&self, _service: &str, _method: &str) -> bool {{"
        )
        .unwrap();
        writeln!(buf, "        false").unwrap();
    } else {
        writeln!(
            buf,
            "    fn is_bidi_stream(&self, _service: &str, method: &str) -> bool {{"
        )
        .unwrap();
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
        "    async fn handle_rpc(&self, service: &str, method: &str, message: &{}::RpcMessage) -> {}::RpcResult {{",
        rpc, rpc
    )
    .unwrap();
    writeln!(buf, "        if service != Self::NAME {{",).unwrap();
    writeln!(
        buf,
        "            return {}::RpcResult::Unary(Err({}::Status::not_found(format!(\"Unknown service: {{}}\", service))));",
        rpc, rpc
    )
    .unwrap();
    writeln!(buf, "        }}").unwrap();
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
    writeln!(buf, "        if service != Self::NAME {{",).unwrap();
    writeln!(
        buf,
        "            return {}::RpcResult::Unary(Err({}::Status::not_found(format!(\"Unknown service: {{}}\", service))));",
        rpc, rpc
    )
    .unwrap();
    writeln!(buf, "        }}").unwrap();
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

    if !bidi_methods.is_empty() {
        generate_start_bidi_stream(service, buf, rpc);
    }

    writeln!(buf, "}}").unwrap();
}

fn generate_start_bidi_stream(service: &Service, buf: &mut String, rpc: &str) {
    writeln!(buf).unwrap();
    writeln!(
        buf,
        "    async fn start_bidi_stream(&self, service: &str, method: &str, mut input_rx: mpsc::Receiver<{}::RpcMessage>) -> Result<{}::BidiStreamOutput, {}::Status> {{",
        rpc, rpc, rpc
    ).unwrap();
    writeln!(buf, "        if service != Self::NAME {{").unwrap();
    writeln!(
        buf,
        "            return Err({}::Status::not_found(format!(\"Unknown service: {{}}\", service)));",
        rpc
    )
    .unwrap();
    writeln!(buf, "        }}").unwrap();
    writeln!(buf, "        match method {{").unwrap();

    for method in &service.methods {
        if !(method.client_streaming && method.server_streaming) {
            continue;
        }
        let method_snake = to_snake_case(&method.name);
        let proto_name = method_proto_name(method);
        let input = &method.input_type;

        writeln!(buf, "            \"{}\" => {{", proto_name).unwrap();
        // Create channel for decoded items (fed into Streaming)
        writeln!(
            buf,
            "                let (item_tx, item_rx) = mpsc::channel::<Result<{}, {}::Status>>(64);",
            input, rpc
        )
        .unwrap();
        // Spawn decoder task: reads RpcMessage from input_rx, decodes, forwards to item_tx
        writeln!(buf, "                tokio::spawn(async move {{").unwrap();
        writeln!(
            buf,
            "                    log::trace!(\"[BIDI_TRACE] codegen decoder: task started for {}\");",
            method_snake
        ).unwrap();
        writeln!(buf, "                    let mut msg_count: u64 = 0;").unwrap();
        writeln!(
            buf,
            "                    while let Some(msg) = input_rx.recv().await {{"
        )
        .unwrap();
        writeln!(buf, "                        msg_count += 1;").unwrap();
        writeln!(
            buf,
            "                        log::trace!(\"[BIDI_TRACE] codegen decoder: msg #{{}}, payload_len={{}}\", msg_count, msg.payload.len());"
        ).unwrap();
        writeln!(
            buf,
            "                        if msg.payload.is_empty() {{ log::trace!(\"[BIDI_TRACE] codegen decoder: skipping empty payload msg #{{}}\", msg_count); continue; }}"
        )
        .unwrap();
        writeln!(
            buf,
            "                        match {}::decode(&msg.payload[..]) {{",
            input
        )
        .unwrap();
        writeln!(
            buf,
            "                            Ok(decoded) => {{ log::trace!(\"[BIDI_TRACE] codegen decoder: decoded msg #{{}}, forwarding to item_tx\", msg_count); if item_tx.send(Ok(decoded)).await.is_err() {{ log::trace!(\"[BIDI_TRACE] codegen decoder: item_tx closed, breaking\"); break; }} }}"
        ).unwrap();
        writeln!(
            buf,
            "                            Err(e) => {{ log::error!(\"[BIDI_TRACE] codegen decoder: decode error msg #{{}}: {{}}\", msg_count, e); let _ = item_tx.send(Err({}::Status::invalid_argument(e.to_string()))).await; break; }}",
            rpc
        ).unwrap();
        writeln!(buf, "                        }}").unwrap();
        writeln!(buf, "                    }}").unwrap();
        writeln!(
            buf,
            "                    log::trace!(\"[BIDI_TRACE] codegen decoder: task ended, processed {{}} msgs\", msg_count);"
        ).unwrap();
        writeln!(buf, "                }});").unwrap();
        // Create Streaming from the decoded item receiver
        writeln!(
            buf,
            "                let streaming = {}::Streaming::new(tokio_stream::wrappers::ReceiverStream::new(item_rx));",
            rpc
        ).unwrap();
        writeln!(
            buf,
            "                let request = {}::Request::new(streaming);",
            rpc
        )
        .unwrap();
        // Call the service handler once
        writeln!(
            buf,
            "                match self.inner.{}(request).await {{",
            method_snake
        )
        .unwrap();
        writeln!(buf, "                    Ok(resp) => {{").unwrap();
        writeln!(
            buf,
            "                        let stream = resp.into_inner();"
        )
        .unwrap();
        writeln!(
            buf,
            "                        let (tx, rx) = mpsc::channel(256);"
        )
        .unwrap();
        writeln!(buf, "                        tokio::spawn(async move {{").unwrap();
        writeln!(buf, "                            let mut stream = stream;").unwrap();
        writeln!(
            buf,
            "                            while let Some(item) = stream.next().await {{"
        )
        .unwrap();
        write_stream_pump_send(buf, 32);
        writeln!(buf, "                            }}").unwrap();
        writeln!(buf, "                        }});").unwrap();
        writeln!(
            buf,
            "                        Ok({}::BidiStreamOutput {{ output: {}::ResponseBody::Streaming(rx) }})",
            rpc, rpc
        ).unwrap();
        writeln!(buf, "                    }}").unwrap();
        writeln!(buf, "                    Err(e) => Err(e),").unwrap();
        writeln!(buf, "                }}").unwrap();
        writeln!(buf, "            }}").unwrap();
    }

    writeln!(
        buf,
        "            _ => Err({}::Status::not_found(format!(\"Unknown method: {{}}\", method))),",
        rpc
    )
    .unwrap();
    writeln!(buf, "        }}").unwrap();
    writeln!(buf, "    }}").unwrap();
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

/// Write an import into the generated file unless it is already there.
///
/// `prost_build` hands every service in a `.proto` the *same* output buffer, so a file declaring
/// two services would otherwise emit `use async_trait::async_trait;` twice and fail to compile —
/// which made "one service per file" an unwritten rule rather than a choice.
fn import_once(buf: &mut String, import: &str) {
    if !buf.lines().any(|line| line == import) {
        writeln!(buf, "{import}").unwrap();
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

/// Where the `Status` conversions every tonic adapter needs are published.
///
/// Emitted adapters land in the `OUT_DIR` of `tddy-service` and its dependents, and convert a
/// refusal through the same pair as the hand-written adapters in `tddy-daemon`, so a given refusal
/// cannot reach two transports as two different gRPC codes. Not configurable: there is one such
/// pair in the workspace, and an adapter that used a second one would be the drift this avoids.
const TONIC_STATUS_PATH: &str = "tddy_service";

/// Emit the adapter wrapper struct alone: a `.proto` with no tonic-build pass has no server trait
/// to implement, so there is nothing to delegate to.
fn generate_tonic_adapter_struct(service: &Service, buf: &mut String) {
    write_tonic_adapter_wrapper(service, buf, false);
}

/// Emit the adapter: the wrapper struct plus a full impl of the tonic server trait, every method
/// delegating to the wrapped tddy-rpc implementation.
///
/// The trait is reached through tonic-build's own `<service>_server` module, which the caller must
/// bring into scope (see [`TddyServiceGenerator::tonic_trait_path`]) — the trait itself cannot be
/// imported unqualified, because it shares its name with the tddy-rpc flavor generated above.
fn generate_tonic_adapter(service: &Service, buf: &mut String, rpc: &str) {
    let adapter_name = format!("{}TonicAdapter", service.name);
    let server_module = format!("{}_server", to_snake_case(&service.name));

    let needs_stream_ext = service
        .methods
        .iter()
        .any(|m| m.client_streaming || m.server_streaming);
    if needs_stream_ext {
        import_once(buf, "use futures_util::StreamExt;");
    }
    import_once(buf, &format!("use {TONIC_STATUS_PATH}::to_tonic_status;"));
    if service.methods.iter().any(|m| m.client_streaming) {
        import_once(buf, &format!("use {TONIC_STATUS_PATH}::to_rpc_status;"));
    }

    write_tonic_adapter_wrapper(service, buf, true);

    writeln!(buf, "#[tonic::async_trait]").unwrap();
    writeln!(
        buf,
        "impl<T> {}::{} for {}<T>",
        server_module, service.name, adapter_name
    )
    .unwrap();
    writeln!(buf, "where").unwrap();
    writeln!(buf, "    T: {},", service.name).unwrap();
    // The tddy-rpc trait bounds its stream associated types `Send + Unpin` but not `'static`, which
    // boxing them into the tonic trait's `Pin<Box<dyn Stream + Send>>` requires.
    for method in &service.methods {
        if method.server_streaming {
            writeln!(
                buf,
                "    T::{}Stream: 'static,",
                to_pascal_case(&method.name)
            )
            .unwrap();
        }
    }
    writeln!(buf, "{{").unwrap();

    for (i, method) in service.methods.iter().enumerate() {
        if i > 0 {
            writeln!(buf).unwrap();
        }
        generate_tonic_adapter_method(service, method, buf, rpc);
    }

    writeln!(buf, "}}").unwrap();
}

/// Emit the wrapper struct and its constructor, shared by both adapter flavors.
///
/// `T` is unbounded so the struct can be named without the service trait in scope; the trait bound
/// rides on the impl instead. `inner` is an `Arc` so one implementation instance can be served over
/// more than one transport at once (gRPC and LiveKit, say) rather than being moved into the adapter.
fn write_tonic_adapter_wrapper(service: &Service, buf: &mut String, has_trait_impl: bool) {
    let adapter_name = format!("{}TonicAdapter", service.name);

    writeln!(buf).unwrap();
    writeln!(
        buf,
        "/// Adapter to use {} with tonic gRPC server.",
        service.name
    )
    .unwrap();
    if !has_trait_impl {
        // Nothing reads `inner` without a trait impl to delegate through.
        writeln!(buf, "#[allow(dead_code)]").unwrap();
    }
    writeln!(buf, "pub struct {}<T> {{", adapter_name).unwrap();
    writeln!(buf, "    inner: std::sync::Arc<T>,").unwrap();
    writeln!(buf, "}}").unwrap();
    writeln!(buf).unwrap();

    writeln!(buf, "impl<T> {}<T> {{", adapter_name).unwrap();
    writeln!(buf, "    pub fn new(inner: std::sync::Arc<T>) -> Self {{").unwrap();
    writeln!(buf, "        Self {{ inner }}").unwrap();
    writeln!(buf, "    }}").unwrap();
    writeln!(buf, "}}").unwrap();
    if has_trait_impl {
        writeln!(buf).unwrap();
    }
}

/// Emit one tonic trait method — and, for a server-streaming rpc, the associated stream type the
/// tonic trait declares alongside it.
///
/// Both names come from what tonic-build itself uses, which is *not* the same field twice:
///
/// * the method name is prost's [`Method::name`] verbatim — tonic-build declares its trait method
///   as `format_ident!("{}", method.name())`, so prost's `sanitize_identifier(to_snake_case(..))`
///   is already applied: `StreamSessionTerminalIO` arrives as `stream_session_terminal_io`, and an
///   rpc named `Type` arrives as `r#type`, which is what the trait declares and therefore what the
///   impl must spell. Re-deriving it here would have to reproduce both halves of that rule, and a
///   derivation that got the keyword half wrong would emit `async fn type(` — uncompilable.
/// * the associated stream type comes from the *proto* name, because tonic-build builds it from
///   `method.identifier()`: `StreamSessionTerminalIOStream`.
///
/// The delegation target is the tddy-rpc trait's method, named the same way the trait emitted above
/// it names it.
fn generate_tonic_adapter_method(service: &Service, method: &Method, buf: &mut String, rpc: &str) {
    let tonic_method = &method.name;
    let rpc_method = to_snake_case(&method.name);
    let stream_assoc = format!("{}Stream", method_proto_name(method));
    let input = &method.input_type;
    let output = &method.output_type;
    let svc = &service.name;

    if method.server_streaming {
        writeln!(
            buf,
            "    type {} = std::pin::Pin<Box<dyn futures_util::Stream<Item = Result<{}, tonic::Status>> + Send>>;",
            stream_assoc, output
        )
        .unwrap();
        writeln!(buf).unwrap();
        // A tonic `Status` is large enough to trip `result_large_err`, and the size is tonic's
        // choice, not this signature's: the trait being implemented dictates the return type.
        writeln!(buf, "    #[allow(clippy::result_large_err)]").unwrap();
    }

    let request_type = if method.client_streaming {
        format!("tonic::Streaming<{}>", input)
    } else {
        input.to_string()
    };
    let response_type = if method.server_streaming {
        format!("Self::{}", stream_assoc)
    } else {
        output.to_string()
    };

    writeln!(buf, "    async fn {}(", tonic_method).unwrap();
    writeln!(buf, "        &self,").unwrap();
    writeln!(buf, "        request: tonic::Request<{}>,", request_type).unwrap();
    writeln!(
        buf,
        "    ) -> Result<tonic::Response<{}>, tonic::Status> {{",
        response_type
    )
    .unwrap();

    if method.client_streaming {
        writeln!(
            buf,
            "        let inbound = request.into_inner().map(|item| item.map_err(to_rpc_status));"
        )
        .unwrap();
        writeln!(
            buf,
            "        let rpc_request = {}::Request::new({}::Streaming::new(inbound));",
            rpc, rpc
        )
        .unwrap();
        writeln!(
            buf,
            "        let resp = {}::{}(&*self.inner, rpc_request)",
            svc, rpc_method
        )
        .unwrap();
    } else {
        writeln!(
            buf,
            "        let resp = {}::{}(&*self.inner, {}::Request::new(request.into_inner()))",
            svc, rpc_method, rpc
        )
        .unwrap();
    }
    writeln!(buf, "            .await").unwrap();
    writeln!(buf, "            .map_err(to_tonic_status)?;").unwrap();

    if method.server_streaming {
        writeln!(
            buf,
            "        let outbound = resp.into_inner().map(|item| item.map_err(to_tonic_status));"
        )
        .unwrap();
        writeln!(buf, "        Ok(tonic::Response::new(Box::pin(outbound)))").unwrap();
    } else {
        writeln!(buf, "        Ok(tonic::Response::new(resp.into_inner()))").unwrap();
    }
    writeln!(buf, "    }}").unwrap();
}

#[cfg(test)]
mod tonic_adapter_tests {
    use super::*;

    /// A method as `prost-build` hands it to a generator: the name the rpc was declared with in the
    /// `.proto`, and the Rust name prost derived from it.
    ///
    /// Both are spelled out at every call site, because keeping them apart is the generator's job and
    /// a fixture that derived the second from the first would derive it *its* way rather than prost's.
    /// prost's way is `sanitize_identifier(heck::to_snake_case(..))`: it collapses an acronym run
    /// (`StreamSessionTerminalIO` -> `stream_session_terminal_io`, never `..._i_o`) and raw-escapes a
    /// keyword (`Type` -> `r#type`).
    fn a_method(
        proto_name: &str,
        prost_name: &str,
        client_streaming: bool,
        server_streaming: bool,
    ) -> Method {
        Method {
            name: prost_name.to_string(),
            proto_name: proto_name.to_string(),
            comments: Default::default(),
            input_type: format!("{proto_name}Request"),
            output_type: format!("{proto_name}Response"),
            input_proto_type: format!(".session_files.{proto_name}Request"),
            output_proto_type: format!(".session_files.{proto_name}Response"),
            options: Default::default(),
            client_streaming,
            server_streaming,
        }
    }

    fn a_service_with(methods: Vec<Method>) -> Service {
        Service {
            name: "SessionFilesService".to_string(),
            proto_name: "SessionFilesService".to_string(),
            package: "session_files".to_string(),
            comments: Default::default(),
            methods,
            options: Default::default(),
        }
    }

    fn generated_for(methods: Vec<Method>) -> String {
        let mut buf = String::new();
        generate_tonic_adapter(&a_service_with(methods), &mut buf, "tddy_rpc");
        buf
    }

    /// The whole point of the generator: a delegating body, not just a struct.
    ///
    /// The stub emitted an adapter struct and a `new()` and stopped, which is why node 1 hand-wrote
    /// 17 `async fn`s and node 6 would have written 22.
    #[test]
    fn generates_a_delegating_body_for_a_unary_method() {
        // Given
        let generated = generated_for(vec![a_method(
            "ReadHostDocument",
            "read_host_document",
            false,
            false,
        )]);

        // Then
        assert!(
            generated.contains("async fn read_host_document"),
            "the adapter must implement the method, not merely declare a struct:\n{generated}"
        );
        assert!(
            generated.contains("self.inner"),
            "the body must delegate to the wrapped Connect-RPC impl:\n{generated}"
        );
    }

    /// A server-streaming method needs the associated stream type as well as the method, because the
    /// tonic trait declares one per streaming rpc.
    #[test]
    fn generates_the_associated_stream_type_for_a_server_streaming_method() {
        // Given
        let generated = generated_for(vec![a_method(
            "StreamReadHostDocument",
            "stream_read_host_document",
            false,
            true,
        )]);

        // Then
        assert!(
            generated.contains("type StreamReadHostDocumentStream"),
            "a server-streaming rpc needs its associated Stream type:\n{generated}"
        );
        assert!(
            generated.contains("async fn stream_read_host_document"),
            "and its method:\n{generated}"
        );
    }

    /// **The case only family K has.** `StreamSessionTerminalIO` is the one bidirectional method in
    /// the entire 90-method surface, so a generator built in any other node would have handled unary
    /// and server-streaming and been found incomplete here.
    ///
    /// A bidi method takes `tonic::Streaming<In>` rather than `tonic::Request<In>` and answers with a
    /// stream, so both halves differ from every other shape.
    #[test]
    fn generates_both_halves_of_a_bidirectional_method() {
        // Given
        let generated = generated_for(vec![a_method(
            "StreamSessionTerminalIO",
            "stream_session_terminal_io",
            true,
            true,
        )]);

        // Then
        assert!(
            generated.contains("Streaming"),
            "a bidi rpc's request is a Streaming, not a Request:\n{generated}"
        );
        assert!(
            generated.contains("type StreamSessionTerminalIOStream"),
            "and its response is still a stream:\n{generated}"
        );
        assert!(
            generated.contains("SessionFilesService::stream_session_terminal_io(&*self.inner"),
            "and it must delegate to the trait method under the name prost gave it — the delegation \
             target is the half no assertion used to cover:\n{generated}"
        );
    }

    /// prost raw-escapes an rpc whose snake_case name is a Rust keyword, and tonic-build declares the
    /// trait method with that exact string: `async fn r#type`. An adapter that re-derived the name
    /// from the proto name instead would emit `async fn type(`, which is not a legal signature — and
    /// nothing in the 90-method surface would catch it, because no rpc there is keyword-named yet.
    #[test]
    fn spells_a_keyword_named_rpc_the_way_prost_escaped_it() {
        // Given
        let generated = generated_for(vec![a_method("Type", "r#type", false, false)]);

        // Then
        assert!(
            generated.contains("async fn r#type("),
            "the impl must declare the escaped name tonic-build's trait declares:\n{generated}"
        );
        assert!(
            !generated.contains("async fn type("),
            "a bare keyword cannot name a fn, so this adapter would not compile:\n{generated}"
        );
        assert!(
            generated.contains("SessionFilesService::r#type(&*self.inner"),
            "and it must delegate through the same escaped name:\n{generated}"
        );
    }

    /// Three hand-written adapters already share `to_tonic_status` so they cannot drift on how a
    /// refusal maps to a tonic code. A generator that built its own `tonic::Status` would reintroduce
    /// exactly that drift — between generated and hand-written adapters.
    #[test]
    fn delegates_status_conversion_rather_than_constructing_its_own() {
        // Given
        let generated = generated_for(vec![a_method(
            "ReadHostDocument",
            "read_host_document",
            false,
            false,
        )]);

        // Then
        assert!(
            generated.contains("to_tonic_status"),
            "the generated body must call the shared conversion:\n{generated}"
        );
        assert!(
            !generated.contains("tonic::Status::internal")
                && !generated.contains("tonic::Status::unknown"),
            "it must not construct a status itself, or generated and hand-written adapters drift:\n{generated}"
        );
    }

    /// A service with every shape at once generates one impl block carrying all of them — the shape
    /// node 6 actually needs, since `terminal_session.TerminalSessionService` mixes all three.
    #[test]
    fn generates_one_impl_carrying_every_method_shape() {
        // Given
        let generated = generated_for(vec![
            a_method("SendTerminalInput", "send_terminal_input", false, false),
            a_method(
                "StreamTerminalOutput",
                "stream_terminal_output",
                false,
                true,
            ),
            a_method(
                "StreamSessionTerminalIO",
                "stream_session_terminal_io",
                true,
                true,
            ),
        ]);

        // Then
        for expected in [
            "async fn send_terminal_input",
            "async fn stream_terminal_output",
            "async fn stream_session_terminal_io",
        ] {
            assert!(
                generated.contains(expected),
                "missing {expected}:\n{generated}"
            );
        }
    }
}
