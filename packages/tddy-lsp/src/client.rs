//! LSP JSON-RPC client. Speaks to a running server through `tddy-task` channels: requests
//! go out via the task's stdin sender, responses/notifications arrive on the stdout
//! broadcast. Requests are correlated to responses by id; `publishDiagnostics`
//! notifications are cached per document URI, and every other notification goes to the
//! [`notifications`] sink, which serves a drainer and any number of observers at once.

mod dispatch;
mod documents;
pub mod notifications;
mod parse;
mod queries;
mod types;

use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bytes::Bytes;
use serde_json::{json, Value};
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::task::JoinHandle;

use crate::error::LspError;
use crate::protocol::encode_message;
use dispatch::read_loop;
use documents::DocumentVersions;
use notifications::NotificationSink;

pub use notifications::{NotificationEvent, NotificationStream};
pub use types::{Diagnostic, Location, Position, Range, ResponseError, SymbolInfo};

/// How long a single request waits for its correlated response before giving up, unless a
/// caller raises it with [`LspClient::set_request_timeout`].
///
/// Ten seconds is right for the interactive queries this client was built for and far too
/// short for a code-action request against a cold index, which is why it is a default rather
/// than a cap.
const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// What a pending request is handed: the response `result`, or the server's error.
type Response = std::result::Result<Value, ResponseError>;

/// Pending in-flight requests, keyed by JSON-RPC id.
type Pending = Arc<Mutex<HashMap<i64, oneshot::Sender<Response>>>>;
/// Cache of the most recent `publishDiagnostics` per document URI.
type DiagnosticsCache = Arc<Mutex<HashMap<String, Vec<Diagnostic>>>>;

/// Where server notifications the client does not consume itself are put.
type Notifications = Arc<NotificationSink>;

/// What a host installs to learn that this client was used.
///
/// A caller that borrows the client from a registry and then works for minutes never goes back to
/// the registry, so nothing else tells an idle timer that the server is in use.
pub type ActivityHook = Arc<dyn Fn() + Send + Sync>;

/// A live LSP client attached to one running server.
pub struct LspClient {
    /// Outbound byte stream to the server's stdin (framed JSON-RPC).
    stdin: mpsc::UnboundedSender<Bytes>,
    /// Monotonic source of request ids.
    next_id: AtomicI64,
    /// In-flight requests awaiting their response.
    pending: Pending,
    /// Latest published diagnostics per document.
    diagnostics: DiagnosticsCache,
    /// The background reader draining the server's stdout; aborted on drop.
    reader: JoinHandle<()>,
    /// How long one request waits for its response, in milliseconds. Adjustable through
    /// `&self` because callers only ever hold the client behind an `Arc` from the registry.
    request_timeout_ms: AtomicU64,
    /// The server's `initialize` result, kept because what the server negotiated — its position
    /// encoding above all — decides whether a caller's coordinates mean what it thinks.
    handshake: Mutex<Value>,
    /// Server notifications this client does not consume itself, kept for a caller to drain.
    ///
    /// `$/progress` and `experimental/serverStatus` are the two that matter: they are the only
    /// account of what a server is doing during a load that answers no requests, and dropping
    /// them left every such wait silent and every timeout unable to say where the server got to.
    notifications: Notifications,
    /// The version each open document was last announced at.
    ///
    /// Held here rather than by a caller because a server tracks versions per document for its
    /// whole life, while a caller that drives one operation does not.
    documents: DocumentVersions,
    /// Installed by the host that owns this client's idle timer, and called on every request and
    /// notification. Behind a `Mutex` because the host only ever holds the client behind an `Arc`.
    activity: Mutex<Option<ActivityHook>>,
}

impl Drop for LspClient {
    fn drop(&mut self) {
        self.reader.abort();
    }
}

impl LspClient {
    /// Attach to a spawned server's channels and complete the
    /// `initialize` / `initialized` handshake against `root_uri`.
    ///
    /// `capabilities` is advertised verbatim. It is the caller's to choose because a server
    /// tailors its answers to it — rust-analyzer returns no code actions whatsoever to a client
    /// that advertised no `codeAction` support, and that is indistinguishable from a range which
    /// supports no refactoring.
    pub async fn initialize(
        stdin: mpsc::UnboundedSender<Bytes>,
        stdout: broadcast::Receiver<Bytes>,
        root_uri: &str,
        capabilities: Value,
        initialization_options: Value,
    ) -> Result<Self, LspError> {
        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let diagnostics: DiagnosticsCache = Arc::new(Mutex::new(HashMap::new()));
        let notifications: Notifications = Arc::new(NotificationSink::new());

        let reader = tokio::spawn(read_loop(
            stdout,
            Arc::clone(&pending),
            Arc::clone(&diagnostics),
            Arc::clone(&notifications),
            stdin.clone(),
        ));

        let client = Self {
            stdin,
            next_id: AtomicI64::new(1),
            pending,
            diagnostics,
            reader,
            request_timeout_ms: AtomicU64::new(DEFAULT_REQUEST_TIMEOUT.as_millis() as u64),
            handshake: Mutex::new(Value::Null),
            notifications,
            documents: Mutex::new(HashMap::new()),
            activity: Mutex::new(None),
        };

        let params = json!({
            "processId": Value::Null,
            "rootUri": root_uri,
            "capabilities": capabilities,
            "initializationOptions": initialization_options,
        });
        let handshake = client.request("initialize", params).await?;
        *client.handshake.lock().unwrap() = handshake;
        client.notify("initialized", json!({}))?;

        Ok(client)
    }

    /// Graceful `shutdown` / `exit`.
    pub async fn shutdown(&self) -> Result<(), LspError> {
        self.request("shutdown", Value::Null).await?;
        self.notify("exit", Value::Null)?;
        Ok(())
    }

    /// Send a request and await its correlated response `result`.
    pub async fn request_raw(&self, method: &str, params: Value) -> Result<Value, LspError> {
        self.request(method, params).await
    }

    /// Send a notification (no id, no response expected).
    pub async fn notify_raw(&self, method: &str, params: Value) -> Result<(), LspError> {
        self.notify(method, params)
    }

    /// Take every server notification received since the last drain, oldest first.
    ///
    /// Destructive and single-consumer by design: the bridged restructuring path folds what it
    /// drains into its own account of the server. An observer that must not take notifications
    /// from that account wants [`Self::subscribe_notifications`].
    pub fn drain_notifications(&self) -> Vec<Value> {
        self.notifications.drain()
    }

    /// Watch this server's notifications without consuming them.
    ///
    /// Any number of subscribers may attach alongside each other and alongside
    /// [`Self::drain_notifications`], each seeing every notification that arrives after it
    /// attached. See [`NotificationStream`] for what a subscriber that falls behind is told.
    pub fn subscribe_notifications(&self) -> NotificationStream {
        self.notifications.subscribe()
    }

    /// The server's `initialize` result, as it answered the handshake.
    pub fn handshake(&self) -> Value {
        self.handshake.lock().unwrap().clone()
    }

    /// Set how long a single request waits for its response.
    ///
    /// Callers whose own budget governs the wait — `tddy-tools restructure` and its
    /// `--indexing-budget` — raise this so a slow index reports as a slow index rather than
    /// as a request that failed.
    pub fn set_request_timeout(&self, timeout: Duration) {
        self.request_timeout_ms
            .store(timeout.as_millis() as u64, Ordering::SeqCst);
    }

    /// How long a single request currently waits for its response.
    pub fn request_timeout(&self) -> Duration {
        Duration::from_millis(self.request_timeout_ms.load(Ordering::SeqCst))
    }

    /// Install the hook called whenever this client is used.
    ///
    /// The host that owns the client's idle timer installs it once, on receiving the client, so
    /// that a caller which borrows the client and then works for minutes keeps its server alive
    /// without going back to the registry for it. The last installer wins.
    pub fn set_activity_hook(&self, hook: ActivityHook) {
        *self.activity.lock().unwrap() = Some(hook);
    }

    /// Report use of this client to the installed hook, if any.
    ///
    /// The hook is cloned out of the lock before it runs: it is the host's code, and holding this
    /// client's lock across it would make every request wait on whatever the host does.
    fn record_activity(&self) {
        let hook = self.activity.lock().unwrap().clone();
        if let Some(hook) = hook {
            hook();
        }
    }

    /// Send a request and await its correlated response `result`, bounded only by the request
    /// timeout.
    async fn request(&self, method: &str, params: Value) -> Result<Value, LspError> {
        // A caller with nothing to abandon it for waits exactly as long as it always did.
        self.request_abandonable(method, params, std::future::pending())
            .await
    }

    /// Send a request and await its response, giving up as soon as `abandon` completes.
    ///
    /// For a caller that has its own reason to stop — a `^C`d run, a dropped RPC — and must not be
    /// held until the request timeout expires. `abandon` is any future: a cancellation token's
    /// `cancelled()`, a `oneshot` the caller keeps the sending half of, a `Notify`.
    ///
    /// An abandoned request is also cancelled *at the server* with `$/cancelRequest`, so it stops
    /// computing an answer nobody will read — which on a wedged run is the point: the caller
    /// stopping has to stop the work, not only the waiting.
    pub async fn request_abandonable<A>(
        &self,
        method: &str,
        params: Value,
        abandon: A,
    ) -> Result<Value, LspError>
    where
        A: Future<Output = ()>,
    {
        self.record_activity();
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        // Held for the rest of this call: however the request ends, the correlation map is left
        // as it was found.
        let _slot = PendingSlot::register(&self.pending, id, tx);

        let message = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        if self
            .stdin
            .send(Bytes::from(encode_message(&message)))
            .is_err()
        {
            return Err(LspError::ServerExited);
        }

        tokio::select! {
            delivered = tokio::time::timeout(self.request_timeout(), rx) => match delivered {
                Ok(Ok(Ok(result))) => Ok(result),
                Ok(Ok(Err(error))) => Err(LspError::Server {
                    code: error.code,
                    message: error.message,
                }),
                Ok(Err(_)) => Err(LspError::ServerExited),
                Err(_) => Err(LspError::Timeout),
            },
            () = abandon => {
                if let Err(err) = self.notify("$/cancelRequest", json!({ "id": id })) {
                    log::debug!(
                        target: "tddy_lsp::client",
                        "could not cancel request {id} ({method}) at the server: {err}"
                    );
                }
                Err(LspError::Abandoned)
            }
        }
    }

    /// Send a notification (no id, no response expected).
    fn notify(&self, method: &str, params: Value) -> Result<(), LspError> {
        self.record_activity();
        let message = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        self.stdin
            .send(Bytes::from(encode_message(&message)))
            .map_err(|_| LspError::ServerExited)
    }
}

/// One request's place in the correlation map, given up when this is dropped.
///
/// Every way a request can end has to leave the map as it was found — answered, timed out,
/// abandoned, or its future simply dropped by a caller that raced it. The abandoned and dropped
/// endings have no code of their own to clean up in, which is how an id nothing would ever answer
/// stayed in the map for the client's whole life.
struct PendingSlot {
    pending: Pending,
    id: i64,
}

impl PendingSlot {
    /// Register `id` as in flight, awaiting its response on `tx`.
    fn register(pending: &Pending, id: i64, tx: oneshot::Sender<Response>) -> Self {
        pending.lock().unwrap().insert(id, tx);
        Self {
            pending: Arc::clone(pending),
            id,
        }
    }
}

impl Drop for PendingSlot {
    fn drop(&mut self) {
        self.pending.lock().unwrap().remove(&self.id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A request that ends any way other than being answered — abandoned, timed out, or its
    /// future dropped by a caller racing it — must leave nothing behind, or the correlation map
    /// grows an entry per such request for the client's whole life.
    #[test]
    fn forgets_an_in_flight_request_once_its_slot_is_given_up() {
        // Given a request registered as in flight on id 9
        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let (tx, _rx) = oneshot::channel();
        let slot = PendingSlot::register(&pending, 9, tx);
        assert!(
            pending.lock().unwrap().contains_key(&9),
            "expected the request to be registered before it is given up"
        );

        // When its slot is given up
        drop(slot);

        // Then the correlation map holds nothing for it
        assert!(pending.lock().unwrap().is_empty());
    }
}
