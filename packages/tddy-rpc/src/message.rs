//! Protocol-agnostic RPC message types.

/// How a request reached the service handling it.
///
/// Stamped by the **host** that received the request — the engine or server it builds for one
/// transport — and never read from the request's own bytes: nothing a caller sends can choose it.
/// That is what makes it usable for a decision the caller must not be able to influence, such as
/// whether a sign-in is the person at the machine (only [`Self::InProcess`] is).
///
/// There is no default. Every place a [`RequestMetadata`] is built names the transport it models,
/// so a new host cannot forget to stamp one and inherit someone else's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RequestTransport {
    /// The host application's own webview, over its in-process IPC bridge (Tauri IPC) — the
    /// person at the machine running this process.
    InProcess,
    /// A LiveKit room's data channel: the common room, or a session room. Any participant the
    /// room admits can send one, peer daemons forwarding a call included.
    LiveKit,
    /// Framed `tddy-rpc` over a Unix-domain socket: the embedded daemon's agent tool socket, a
    /// sandbox's tool socket, a session's toolcall listener. A separate, co-located process.
    UnixSocket,
    /// Framed `tddy-rpc` over a pipe pair to a parent or child process — its stdin/stdout, or a
    /// jail's piped stdio.
    Pipe,
    /// Connect-RPC over HTTP (the daemon's `/rpc` route).
    Http,
    /// gRPC through a tonic server (the daemon's local Unix-domain socket, the index daemon).
    Grpc,
    /// Received over no transport at all: a call one component of this process makes on another
    /// in code, carrying no transport claim. That includes a payload re-dispatched in code after
    /// the request it arrived in was decoded and its metadata is no longer in scope (the sandbox
    /// host bridge that receives only a method name and raw bytes), and a request the process
    /// builds from its own state.
    ///
    /// It is never what a relay stamps on a request whose original metadata it still holds: a
    /// service forwarding an inbound [`crate::Request`] to an inner one passes that request's
    /// metadata through (`Request::with_metadata`), so the inner service sees the stamp the
    /// receiving host gave it. `Direct` is not a stand-in for an unknown transport, and nothing
    /// may treat it as [`Self::InProcess`].
    Direct,
}

/// Metadata attached to an incoming RPC request.
///
/// Built only through [`Self::over`], which names the transport: the field is private so neither a
/// struct literal nor a `Default` can produce metadata that did not choose one.
#[derive(Debug, Clone)]
pub struct RequestMetadata {
    transport: RequestTransport,
    /// Sender identity from the transport envelope (e.g. LiveKit participant identity). Written by
    /// the sender, so it is a routing hint, never an authority.
    pub sender_identity: Option<String>,
}

impl RequestMetadata {
    /// Metadata for a request received over `transport`, with no sender identity.
    pub fn over(transport: RequestTransport) -> Self {
        Self {
            transport,
            sender_identity: None,
        }
    }

    /// The same metadata, carrying the identity the sender's envelope named.
    pub fn with_sender_identity(mut self, sender_identity: Option<String>) -> Self {
        self.sender_identity = sender_identity;
        self
    }

    /// The transport the host received this request on.
    pub fn transport(&self) -> RequestTransport {
        self.transport
    }
}

/// Protocol-agnostic incoming RPC message.
/// The transport layer (e.g. LiveKit participant) decodes the envelope,
/// extracts service/method, and converts to this form before calling the bridge.
#[derive(Debug, Clone)]
pub struct RpcMessage {
    /// Raw encoded protobuf payload (the service-specific request message).
    pub payload: Vec<u8>,
    /// Request metadata (transport, sender identity).
    pub metadata: RequestMetadata,
}

impl RpcMessage {
    pub fn new(payload: Vec<u8>, metadata: RequestMetadata) -> Self {
        Self { payload, metadata }
    }
}
