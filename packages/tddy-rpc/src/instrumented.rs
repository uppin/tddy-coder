//! An [`RpcService`] that records every call it dispatches on the daemon's metrics.
//!
//! The wrapper owns no dispatch: [`MultiRpcService`] (or any inner service) answers, and this
//! layer times the call and records it as [`tddy_metrics::RpcCall`] — `ok` when the inner
//! service answered, the lowercase status code (`not_found`, `internal`, …) when it refused.
//! Durations cover dispatch only: a server stream is recorded when it is established, not when
//! its last message leaves.

use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use tddy_metrics::{DaemonMetrics, RpcCall};

use crate::bridge::{BidiStreamOutput, RpcResult, RpcService};
use crate::message::{RequestMetadata, RpcMessage};
use crate::status::Status;

/// [`RpcService`] dispatch with per-call metrics recording.
pub struct InstrumentedRpcService<S> {
    inner: S,
    metrics: Arc<DaemonMetrics>,
}

impl<S> InstrumentedRpcService<S> {
    /// Dispatch to `inner`, recording every call on `metrics`.
    pub fn new(inner: S, metrics: Arc<DaemonMetrics>) -> Self {
        Self { inner, metrics }
    }

    /// The wrapped service, for the layers (reflection, tests) that name it directly.
    pub fn inner(&self) -> &S {
        &self.inner
    }

    fn record(&self, service: &str, method: &str, code: &str, started: Instant) {
        self.metrics.rpc_completed(
            &RpcCall {
                service,
                method,
                code,
            },
            started.elapsed(),
        );
    }
}

/// The [`Status`] of a finished call as the `code` label: `ok`, or the lowercase status name.
fn code_of(result: &RpcResult) -> String {
    match result {
        RpcResult::Unary(Ok(_)) | RpcResult::ServerStream(Ok(_)) => "ok".to_string(),
        RpcResult::Unary(Err(status)) | RpcResult::ServerStream(Err(status)) => {
            status.code().as_str().to_lowercase()
        }
    }
}

#[async_trait]
impl<S: RpcService> RpcService for InstrumentedRpcService<S> {
    fn is_bidi_stream(&self, service: &str, method: &str) -> bool {
        self.inner.is_bidi_stream(service, method)
    }

    async fn handle_rpc(&self, service: &str, method: &str, message: &RpcMessage) -> RpcResult {
        let started = Instant::now();
        let result = self.inner.handle_rpc(service, method, message).await;
        let code = code_of(&result);
        self.record(service, method, &code, started);
        result
    }

    async fn handle_rpc_stream(
        &self,
        service: &str,
        method: &str,
        messages: &[RpcMessage],
    ) -> RpcResult {
        let started = Instant::now();
        let result = self
            .inner
            .handle_rpc_stream(service, method, messages)
            .await;
        let code = code_of(&result);
        self.record(service, method, &code, started);
        result
    }

    async fn start_bidi_stream(
        &self,
        service: &str,
        method: &str,
        metadata: RequestMetadata,
        input_rx: tokio::sync::mpsc::Receiver<RpcMessage>,
    ) -> Result<BidiStreamOutput, Status> {
        let started = Instant::now();
        let result = self
            .inner
            .start_bidi_stream(service, method, metadata, input_rx)
            .await;
        let code = match &result {
            Ok(_) => "ok".to_string(),
            Err(status) => status.code().as_str().to_lowercase(),
        };
        self.record(service, method, &code, started);
        result
    }
}

#[cfg(test)]
mod instrumented_rpc_service_acceptance {
    use super::*;
    use crate::bridge::{MultiRpcService, ServiceEntry};
    use crate::message::{RequestMetadata, RequestTransport};

    fn a_message() -> RpcMessage {
        RpcMessage::new(vec![], RequestMetadata::over(RequestTransport::Direct))
    }

    struct OkService;

    #[async_trait::async_trait]
    impl RpcService for OkService {
        async fn handle_rpc(
            &self,
            _service: &str,
            _method: &str,
            _message: &RpcMessage,
        ) -> RpcResult {
            RpcResult::Unary(Ok(vec![1, 2, 3]))
        }
    }

    fn stub_metrics() -> Arc<DaemonMetrics> {
        Arc::new(DaemonMetrics::new("test"))
    }

    fn wrapped() -> (InstrumentedRpcService<MultiRpcService>, Arc<DaemonMetrics>) {
        let metrics = stub_metrics();
        let multi = MultiRpcService::new(vec![ServiceEntry {
            name: "session.SessionService",
            service: Arc::new(OkService),
        }]);
        let svc = InstrumentedRpcService::new(multi, Arc::clone(&metrics));
        (svc, metrics)
    }

    #[tokio::test]
    async fn an_answered_call_counts_its_service_method_and_ok() {
        // Given an instrumented roster serving one method
        let (svc, metrics) = wrapped();

        // When the call is answered
        let result = svc
            .handle_rpc("session.SessionService", "ListSessions", &a_message())
            .await;

        // Then the answer passes through and the call is counted as ok
        assert!(matches!(result, RpcResult::Unary(Ok(_))));
        let text = metrics.encode();
        assert!(
            text.lines()
                .any(|l| l.starts_with("tddy_rpc_requests_total")
                    && l.contains("service=\"session.SessionService\"")
                    && l.contains("method=\"ListSessions\"")
                    && l.contains("code=\"ok\"")
                    && l.ends_with(" 1")),
            "{text}"
        );
        assert!(
            text.contains("tddy_rpc_request_duration_seconds_count"),
            "{text}"
        );
    }

    #[tokio::test]
    async fn a_refused_call_counts_its_status_code() {
        // Given an instrumented roster with no such service
        let (svc, metrics) = wrapped();

        // When the call is refused
        let result = svc
            .handle_rpc("no.SuchService", "Missing", &a_message())
            .await;

        // Then the refusal passes through and the call is counted as not_found
        assert!(matches!(result, RpcResult::Unary(Err(_))));
        let text = metrics.encode();
        assert!(
            text.lines()
                .any(|l| l.starts_with("tddy_rpc_requests_total")
                    && l.contains("code=\"not_found\"")
                    && l.ends_with(" 1")),
            "{text}"
        );
    }
}
