//! Request, Response, and Streaming wrappers (tonic-mirrored).

use crate::message::{RequestMetadata, RequestTransport};
use crate::status::Status;
use futures_core::Stream;
use std::pin::Pin;

/// Wraps a message with the metadata of how it arrived (transport, sender identity).
///
/// There is no transport-less constructor: a request is either stamped by the host that received
/// it ([`Self::with_metadata`], [`Self::from_rpc_message`]) or says, by [`Self::direct`], that it
/// was made in code and arrived over nothing.
#[derive(Debug)]
pub struct Request<T> {
    inner: T,
    metadata: RequestMetadata,
}

impl<T> Request<T> {
    /// A request one component of this process makes on another in code — received over no
    /// transport ([`RequestTransport::Direct`]).
    pub fn direct(inner: T) -> Self {
        Self::with_metadata(inner, RequestMetadata::over(RequestTransport::Direct))
    }

    pub fn with_metadata(inner: T, metadata: RequestMetadata) -> Self {
        Self { inner, metadata }
    }

    /// Create from an RpcMessage, using the message's metadata.
    /// The caller is responsible for decoding `message.payload` into `inner`.
    pub fn from_rpc_message(inner: T, message: &crate::message::RpcMessage) -> Self {
        Self {
            inner,
            metadata: message.metadata.clone(),
        }
    }

    pub fn into_inner(self) -> T {
        self.inner
    }

    pub fn get_ref(&self) -> &T {
        &self.inner
    }

    pub fn metadata(&self) -> &RequestMetadata {
        &self.metadata
    }
}

/// Wraps a response message.
#[derive(Debug)]
pub struct Response<T> {
    inner: T,
}

impl<T> Response<T> {
    pub fn new(inner: T) -> Self {
        Self { inner }
    }

    pub fn into_inner(self) -> T {
        self.inner
    }

    pub fn get_ref(&self) -> &T {
        &self.inner
    }
}

/// Wraps an async stream of `Result<T, Status>` — used for client/bidi streaming input.
pub struct Streaming<T> {
    inner: Pin<Box<dyn Stream<Item = Result<T, Status>> + Send>>,
}

impl<T> Streaming<T> {
    pub fn new<S>(stream: S) -> Self
    where
        S: Stream<Item = Result<T, Status>> + Send + 'static,
    {
        Self {
            inner: Box::pin(stream),
        }
    }
}

impl<T> Stream for Streaming<T> {
    type Item = Result<T, Status>;

    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.get_mut().inner.as_mut().poll_next(cx)
    }
}

#[cfg(feature = "tonic")]
mod tonic_impl {
    use super::*;

    impl<T> From<Request<T>> for tonic::Request<T> {
        fn from(r: Request<T>) -> Self {
            tonic::Request::new(r.inner)
        }
    }

    /// A request a tonic server received arrived over gRPC, whatever its sender claims.
    impl<T> From<tonic::Request<T>> for Request<T> {
        fn from(r: tonic::Request<T>) -> Self {
            Request::with_metadata(
                r.into_inner(),
                RequestMetadata::over(RequestTransport::Grpc),
            )
        }
    }

    impl<T> From<Response<T>> for tonic::Response<T> {
        fn from(r: Response<T>) -> Self {
            tonic::Response::new(r.inner)
        }
    }

    impl<T> From<tonic::Response<T>> for Response<T> {
        fn from(r: tonic::Response<T>) -> Self {
            Response::new(r.into_inner())
        }
    }
}
