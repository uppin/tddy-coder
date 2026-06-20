//! gRPC `ServerReflection` (v1) implementation.
//!
//! Serves a curated view of the hosted services: `list_services` returns only the
//! names that were actually registered on this participant, while symbol/filename
//! lookups return the matching `FileDescriptorProto` plus all transitive
//! dependencies, sourced from the combined [`crate::SERVICE_DESCRIPTOR_BYTES`].

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use futures_util::stream::StreamExt;
use prost::Message;
use prost_types::{FileDescriptorProto, FileDescriptorSet};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use tddy_rpc::{Request, Response, Status, Streaming};

use crate::proto::reflection::{
    server_reflection_request::MessageRequest, server_reflection_response::MessageResponse,
    ErrorResponse, FileDescriptorResponse, ListServiceResponse, ServerReflection,
    ServerReflectionRequest, ServerReflectionResponse, ServiceResponse,
};

/// gRPC `ServerReflection` service implementation.
pub struct ServerReflectionImpl {
    /// Names of services actually registered on this participant (the only ones
    /// exposed via `list_services`).
    registered_names: Vec<String>,
    /// All known files, keyed by file name.
    files_by_name: HashMap<String, FileDescriptorProto>,
    /// Fully-qualified symbol name -> declaring file name.
    files_by_symbol: HashMap<String, String>,
}

impl ServerReflectionImpl {
    /// Build the reflection service from the registered service names and a serialized
    /// `FileDescriptorSet`.
    ///
    /// # Panics
    /// Panics if `descriptor_bytes` is not a valid `FileDescriptorSet`. This is a build-time
    /// invariant: the bytes come from the `prost-build` descriptor pass embedded at compile time.
    pub fn new(registered_names: Vec<String>, descriptor_bytes: &'static [u8]) -> Self {
        let fds = FileDescriptorSet::decode(descriptor_bytes)
            .expect("SERVICE_DESCRIPTOR_BYTES must be a valid FileDescriptorSet");

        let mut files_by_name = HashMap::new();
        let mut files_by_symbol = HashMap::new();

        for file in fds.file {
            let name = file.name.clone().unwrap_or_default();
            let pkg = file.package.clone().unwrap_or_default();
            let qualify = |ident: &str| -> String {
                if pkg.is_empty() {
                    ident.to_string()
                } else {
                    format!("{}.{}", pkg, ident)
                }
            };

            for svc in &file.service {
                if let Some(svc_name) = &svc.name {
                    let full = qualify(svc_name);
                    files_by_symbol.insert(full.clone(), name.clone());
                    for method in &svc.method {
                        if let Some(m) = &method.name {
                            files_by_symbol.insert(format!("{}.{}", full, m), name.clone());
                        }
                    }
                }
            }
            for msg in &file.message_type {
                if let Some(msg_name) = &msg.name {
                    files_by_symbol.insert(qualify(msg_name), name.clone());
                }
            }
            for en in &file.enum_type {
                if let Some(e_name) = &en.name {
                    files_by_symbol.insert(qualify(e_name), name.clone());
                }
            }

            files_by_name.insert(name, file);
        }

        Self {
            registered_names,
            files_by_name,
            files_by_symbol,
        }
    }

    /// Collect a file and all its transitive dependencies as serialized bytes.
    /// Dependencies precede the file that requires them.
    fn file_and_deps(&self, file_name: &str) -> Vec<Vec<u8>> {
        let mut result = Vec::new();
        let mut visited = HashSet::new();
        self.collect_deps(file_name, &mut result, &mut visited);
        result
    }

    fn collect_deps(&self, name: &str, out: &mut Vec<Vec<u8>>, visited: &mut HashSet<String>) {
        if visited.contains(name) {
            return;
        }
        visited.insert(name.to_string());
        if let Some(file) = self.files_by_name.get(name) {
            for dep in &file.dependency {
                self.collect_deps(dep, out, visited);
            }
            out.push(file.encode_to_vec());
        }
    }

    /// Compute the response payload for a single reflection request.
    fn respond(&self, req: &ServerReflectionRequest) -> MessageResponse {
        match &req.message_request {
            Some(MessageRequest::ListServices(_)) => {
                let service = self
                    .registered_names
                    .iter()
                    .map(|n| ServiceResponse { name: n.clone() })
                    .collect();
                MessageResponse::ListServicesResponse(ListServiceResponse { service })
            }
            Some(MessageRequest::FileContainingSymbol(symbol)) => {
                match self.files_by_symbol.get(symbol) {
                    Some(file_name) => {
                        MessageResponse::FileDescriptorResponse(FileDescriptorResponse {
                            file_descriptor_proto: self.file_and_deps(file_name),
                        })
                    }
                    None => Self::symbol_not_found(),
                }
            }
            Some(MessageRequest::FileByFilename(file_name)) => {
                if self.files_by_name.contains_key(file_name) {
                    MessageResponse::FileDescriptorResponse(FileDescriptorResponse {
                        file_descriptor_proto: self.file_and_deps(file_name),
                    })
                } else {
                    Self::symbol_not_found()
                }
            }
            // Extension lookups are not supported by the hosted services (proto3, no extensions).
            Some(MessageRequest::FileContainingExtension(_))
            | Some(MessageRequest::AllExtensionNumbersOfType(_)) => {
                MessageResponse::ErrorResponse(ErrorResponse {
                    error_code: 12, // UNIMPLEMENTED
                    error_message: "Extensions are not supported.".to_string(),
                })
            }
            None => MessageResponse::ErrorResponse(ErrorResponse {
                error_code: 3, // INVALID_ARGUMENT
                error_message: "No message_request set.".to_string(),
            }),
        }
    }

    fn symbol_not_found() -> MessageResponse {
        MessageResponse::ErrorResponse(ErrorResponse {
            error_code: 5, // NOT_FOUND
            error_message: "Symbol not found.".to_string(),
        })
    }
}

#[async_trait]
impl ServerReflection for ServerReflectionImpl {
    type ServerReflectionInfoStream = ReceiverStream<Result<ServerReflectionResponse, Status>>;

    async fn server_reflection_info(
        &self,
        request: Request<Streaming<ServerReflectionRequest>>,
    ) -> Result<Response<Self::ServerReflectionInfoStream>, Status> {
        let stream = request.into_inner();
        let (tx, rx) = mpsc::channel(16);

        // Clone the indices into the spawned task. Cheap relative to per-request work and
        // keeps the handler `'static` without holding a reference to `self`.
        let registered_names = self.registered_names.clone();
        let files_by_name = self.files_by_name.clone();
        let files_by_symbol = self.files_by_symbol.clone();

        tokio::spawn(async move {
            let responder = ServerReflectionImpl {
                registered_names,
                files_by_name,
                files_by_symbol,
            };
            futures_util::pin_mut!(stream);
            while let Some(item) = stream.next().await {
                let req = match item {
                    Ok(r) => r,
                    Err(e) => {
                        let _ = tx.send(Err(e)).await;
                        break;
                    }
                };
                let message_response = responder.respond(&req);
                let resp = ServerReflectionResponse {
                    valid_host: req.host.clone(),
                    original_request: Some(req),
                    message_response: Some(message_response),
                };
                if tx.send(Ok(resp)).await.is_err() {
                    break;
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}

/// Build a [`tddy_rpc::ServiceEntry`] for the `ServerReflection` service, exposing only
/// the given `registered_names` via `list_services`.
pub fn reflection_entry_from(registered_names: &[&str]) -> tddy_rpc::ServiceEntry {
    use crate::proto::reflection::ServerReflectionServer;
    let names: Vec<String> = registered_names.iter().map(|s| s.to_string()).collect();
    let impl_ = ServerReflectionImpl::new(names, crate::SERVICE_DESCRIPTOR_BYTES);
    tddy_rpc::ServiceEntry {
        name: ServerReflectionServer::<ServerReflectionImpl>::NAME,
        service: Arc::new(ServerReflectionServer::new(impl_)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build() -> ServerReflectionImpl {
        ServerReflectionImpl::new(
            vec!["test.EchoService".to_string()],
            crate::SERVICE_DESCRIPTOR_BYTES,
        )
    }

    #[test]
    fn embedded_descriptor_set_decodes() {
        // Constructing without panicking proves SERVICE_DESCRIPTOR_BYTES is a valid
        // FileDescriptorSet produced by the build script.

        // When
        let svc = build();

        // Then
        assert!(svc.files_by_symbol.contains_key("test.EchoService"));
    }

    #[test]
    fn list_services_returns_only_registered_names() {
        // Given
        let svc = build();
        let req = ServerReflectionRequest {
            host: String::new(),
            message_request: Some(MessageRequest::ListServices(String::new())),
        };

        // When
        let response = svc.respond(&req);

        // Then
        match response {
            MessageResponse::ListServicesResponse(resp) => {
                let names: Vec<_> = resp.service.iter().map(|s| s.name.as_str()).collect();
                assert_eq!(names, vec!["test.EchoService"]);
            }
            other => panic!("expected ListServicesResponse, got {other:?}"),
        }
    }

    #[test]
    fn file_containing_symbol_returns_descriptor_with_deps() {
        // Given
        let svc = build();
        let req = ServerReflectionRequest {
            host: String::new(),
            message_request: Some(MessageRequest::FileContainingSymbol(
                "test.EchoService".to_string(),
            )),
        };

        // When
        let response = svc.respond(&req);

        // Then
        match response {
            MessageResponse::FileDescriptorResponse(resp) => {
                assert!(!resp.file_descriptor_proto.is_empty());
                // Each entry must be a decodable FileDescriptorProto.
                for bytes in &resp.file_descriptor_proto {
                    FileDescriptorProto::decode(bytes.as_slice())
                        .expect("each entry must be a FileDescriptorProto");
                }
            }
            other => panic!("expected FileDescriptorResponse, got {other:?}"),
        }
    }

    #[test]
    fn unknown_symbol_returns_not_found() {
        // Given
        let svc = build();
        let req = ServerReflectionRequest {
            host: String::new(),
            message_request: Some(MessageRequest::FileContainingSymbol(
                "does.not.Exist".to_string(),
            )),
        };

        // When
        let response = svc.respond(&req);

        // Then
        match response {
            MessageResponse::ErrorResponse(err) => {
                assert_eq!(err.error_code, 5);
                assert_eq!(err.error_message, "Symbol not found.");
            }
            other => panic!("expected ErrorResponse, got {other:?}"),
        }
    }
}
