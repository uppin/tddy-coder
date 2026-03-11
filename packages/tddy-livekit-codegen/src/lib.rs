//! Build-time codegen for LiveKit async RPC services.

use prost_build::{Method, Service, ServiceGenerator};
use std::fmt::Write;

/// Custom service generator for LiveKit async RPC service traits.
pub struct LiveKitServiceGenerator;

impl ServiceGenerator for LiveKitServiceGenerator {
    fn generate(&mut self, service: Service, buf: &mut String) {
        writeln!(buf, "use async_trait::async_trait;").unwrap();
        writeln!(buf, "use tokio::sync::mpsc;").unwrap();
        writeln!(buf).unwrap();

        writeln!(
            buf,
            "/// Generated async service trait for {}",
            service.name
        )
        .unwrap();
        writeln!(buf, "#[async_trait]").unwrap();
        writeln!(buf, "pub trait {}: Send + Sync + 'static {{", service.name).unwrap();

        for method in &service.methods {
            generate_method(method, buf);
        }

        writeln!(buf, "}}").unwrap();
        writeln!(buf).unwrap();
    }

    fn finalize_package(&mut self, _package: &str, _buf: &mut String) {}
}

fn generate_method(method: &Method, buf: &mut String) {
    let input = &method.input_type;
    let output = &method.output_type;

    writeln!(buf, "    /// RPC method: {}", method.name).unwrap();

    match (method.client_streaming, method.server_streaming) {
        (false, false) => {
            writeln!(
                buf,
                "    async fn {}(&self, request: {}) -> Result<{}, crate::Status>;",
                to_snake_case(&method.name),
                input,
                output
            )
            .unwrap();
        }
        (false, true) => {
            writeln!(
                buf,
                "    async fn {}(&self, request: {}) -> Result<mpsc::Receiver<Result<{}, crate::Status>>, crate::Status>;",
                to_snake_case(&method.name),
                input,
                output
            )
            .unwrap();
        }
        (true, false) => {
            writeln!(
                buf,
                "    async fn {}(&self, requests: Vec<{}>) -> Result<{}, crate::Status>;",
                to_snake_case(&method.name),
                input,
                output
            )
            .unwrap();
        }
        (true, true) => {
            writeln!(
                buf,
                "    async fn {}(&self, requests: Vec<{}>) -> Result<mpsc::Receiver<Result<{}, crate::Status>>, crate::Status>;",
                to_snake_case(&method.name),
                input,
                output
            )
            .unwrap();
        }
    }

    writeln!(buf).unwrap();
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
