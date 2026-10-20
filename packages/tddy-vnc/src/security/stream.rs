use crate::security::RfbVersion;
use crate::security::RFB_VERSION_LEN;
use crate::security::TYPE_NONE;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

/// A byte stream the sign-in layer can run over.
pub trait Transport: AsyncRead + AsyncWrite + Unpin + Send + Sync {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send + Sync> Transport for T {}

/// A stream positioned so that vnc-rs, running its own handshake over it, ends up at ClientInit.
///
/// Reads first return the `replay` bytes, then the underlying stream; the first `swallow` bytes
/// written are dropped instead of reaching the server. See the module docs.
pub struct SecuredStream {
    inner: Box<dyn Transport>,
    replay: Vec<u8>,
    replayed: usize,
    swallow: usize,
}

impl SecuredStream {
    pub(super) fn new(inner: Box<dyn Transport>, replay: Vec<u8>, swallow: usize) -> Self {
        Self {
            inner,
            replay,
            replayed: 0,
            swallow,
        }
    }

    /// A stream whose sign-in is complete: vnc-rs is told RFB 3.3 with security type None, which
    /// it handles without reading or writing anything but the version it echoes.
    pub(super) fn authenticated(inner: Box<dyn Transport>) -> Self {
        let mut replay = RfbVersion::V33.bytes().to_vec();
        replay.extend_from_slice(&u32::from(TYPE_NONE).to_be_bytes());
        Self::new(inner, replay, RFB_VERSION_LEN)
    }
}

impl AsyncRead for SecuredStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        if this.replayed < this.replay.len() {
            let pending = &this.replay[this.replayed..];
            let n = pending.len().min(buf.remaining());
            buf.put_slice(&pending[..n]);
            this.replayed += n;
            return Poll::Ready(Ok(()));
        }
        Pin::new(&mut this.inner).poll_read(cx, buf)
    }
}

impl AsyncWrite for SecuredStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        if this.swallow > 0 {
            let n = this.swallow.min(buf.len());
            this.swallow -= n;
            return Poll::Ready(Ok(n));
        }
        Pin::new(&mut this.inner).poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
    }
}
