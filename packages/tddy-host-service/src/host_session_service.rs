//! Daemon-hosted RPC service reached by a spawned tddy-coder tool session over its **OS user's
//! host-session socket**.
//!
//! A tddy-coder tool session serves its own toolcall socket, so its agent's `spawn_conversation` and
//! `github-token` requests land on the *coder's* listener — which can neither create a worktree nor
//! read the owner's vault (daemon-owned work). The coder therefore relays them to the daemon as a
//! reverse RPC over one unix socket **per OS user**, which the daemon binds with owner-only
//! permissions.
//!
//! One socket serves every tool session of that user, so **every request names its session**
//! (`session_id`). The service looks the session up in the [`HostSessionRegistry`], refuses one that
//! is unknown or belongs to a different OS user than the socket's owner, and answers from the
//! handlers registered for that session when it started or resumed.
//!
//! **Trust.** The `session_id` is a label, not a secret: the socket's filesystem permissions are the
//! authentication boundary. Anything running as the socket's OS user can ask for a token for any of
//! that user's registered sessions — the same boundary as that user's own vault.
//!
//! The wire is JSON over [`tddy_stdio`]; there is no proto, so no generated code. A client that
//! predates a method gets `unimplemented`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tddy_core::toolcall::{
    ConversationSpawnHandler, GithubCredentialHandler, SpawnConversationRequestWire,
};
use tddy_rpc::bridge::{RpcResult, RpcService};
use tddy_rpc::{RpcMessage, Status};

// Service/method names are shared with the coder-side relay client via tddy-core so both agree on
// the address without either crate depending on the other.
pub use tddy_core::toolcall::{
    GITHUB_TOKEN_METHOD, HOST_SESSION_SERVICE, SPAWN_CONVERSATION_METHOD,
};

/// What the daemon registered for one tool session when it started or resumed it.
#[derive(Clone)]
pub struct RegisteredSession {
    /// The OS user the session runs as. Compared with the socket's owner on every request.
    pub os_user: String,
    /// Answers `spawn_conversation`; `None` for a recipe that does not spawn conversations.
    pub conversation_spawn_handler: Option<Arc<dyn ConversationSpawnHandler>>,
    /// Answers `github_token` from the session's assignment snapshot and start token; `None` when
    /// the session's project could not be read.
    pub github_credential_handler: Option<Arc<dyn GithubCredentialHandler>>,
}

/// One registered session and the process it was last started as.
struct Slot {
    session: RegisteredSession,
    /// The `tddy-coder` process of the start or resume that registered it; `None` until the
    /// caller learns it (registration precedes the spawn, which needs the socket path first).
    process: Option<u32>,
}

/// The tool sessions a daemon answers on its host-session sockets, by session id.
#[derive(Default)]
pub struct HostSessionRegistry {
    sessions: Mutex<HashMap<String, Slot>>,
}

impl HostSessionRegistry {
    /// Register (or replace — a resume registers again with fresh handlers) a session. A
    /// replacement forgets the previous process: a report that it stopped no longer applies.
    pub fn register(&self, session_id: &str, session: RegisteredSession) {
        self.lock().insert(
            session_id.to_string(),
            Slot {
                session,
                process: None,
            },
        );
    }

    /// Forget a session; its requests are refused from now on.
    pub fn unregister(&self, session_id: &str) {
        self.lock().remove(session_id);
    }

    /// Record the process the registered session is running as. `false` when the session is no
    /// longer registered (it was deleted between the registration and the spawn returning).
    pub fn attach_process(&self, session_id: &str, pid: u32) -> bool {
        match self.lock().get_mut(session_id) {
            Some(slot) => {
                slot.process = Some(pid);
                true
            }
            None => false,
        }
    }

    /// The process the registered session is running as, if it has been recorded.
    pub fn process_of(&self, session_id: &str) -> Option<u32> {
        self.lock().get(session_id).and_then(|slot| slot.process)
    }

    /// Forget a session because `pid` — the process it was registered with — has stopped, so the
    /// daemon stops holding the start's session token for it. Does nothing, and returns `false`,
    /// when the session was registered again since (a resume's process is another one) or has no
    /// recorded process.
    pub fn unregister_stopped(&self, session_id: &str, pid: u32) -> bool {
        let mut sessions = self.lock();
        if sessions.get(session_id).and_then(|slot| slot.process) == Some(pid) {
            sessions.remove(session_id);
            true
        } else {
            false
        }
    }

    fn get(&self, session_id: &str) -> Option<RegisteredSession> {
        self.lock().get(session_id).map(|slot| slot.session.clone())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Slot>> {
        self.sessions.lock().unwrap_or_else(|p| p.into_inner())
    }
}

/// Whether a session `(os_user, session_id)` exists on this host's disk — durable state, not the
/// registry. Lets the service tell a session that predates a daemon restart from one that never was.
pub type SessionProbe = Arc<dyn Fn(&str, &str) -> bool + Send + Sync>;

/// Hosts [`SPAWN_CONVERSATION_METHOD`] and [`GITHUB_TOKEN_METHOD`] for the sessions of one OS user.
pub struct HostSessionService {
    socket_owner: String,
    registry: Arc<HostSessionRegistry>,
    session_exists: Option<SessionProbe>,
}

/// The one field every request carries.
#[derive(serde::Deserialize)]
struct SessionScoped {
    session_id: String,
}

impl HostSessionService {
    /// A service for the socket owned by `socket_owner`, answering sessions in `registry`.
    pub fn new(socket_owner: &str, registry: Arc<HostSessionRegistry>) -> Self {
        Self {
            socket_owner: socket_owner.to_string(),
            registry,
            session_exists: None,
        }
    }

    /// Tell an unregistered session that exists (started before a restart, still running) apart
    /// from one that does not. The probe is asked about **this socket's owner** only, so a request
    /// can never learn that another OS user's session exists.
    pub fn with_session_probe(mut self, probe: SessionProbe) -> Self {
        self.session_exists = Some(probe);
        self
    }

    /// The session a request names, refused when unknown or another OS user's.
    fn session_for(&self, payload: &[u8]) -> Result<RegisteredSession, Status> {
        let scoped: SessionScoped = serde_json::from_slice(payload).map_err(|e| {
            Status::invalid_argument(format!("a host-session request must name its session: {e}"))
        })?;
        let session = self
            .registry
            .get(&scoped.session_id)
            .ok_or_else(|| self.unregistered(&scoped.session_id))?;
        if session.os_user != self.socket_owner {
            // Neither user is named: the caller learns only that this socket is not that session's.
            return Err(Status::permission_denied(
                "that session runs as a different OS user than this socket's owner",
            ));
        }
        Ok(session)
    }

    /// Why a session is not registered: it predates a daemon restart (and exists), or is unknown.
    fn unregistered(&self, session_id: &str) -> Status {
        let exists = self
            .session_exists
            .as_ref()
            .is_some_and(|probe| probe(&self.socket_owner, session_id));
        if exists {
            return Status::failed_precondition(
                "this session was started before the daemon restarted; resume it to re-enable \
                 GitHub tools",
            );
        }
        Status::not_found(format!(
            "no session `{session_id}` is registered with this host (it was deleted, or never \
             existed)"
        ))
    }

    async fn handle_spawn_conversation(&self, payload: &[u8]) -> Result<Vec<u8>, Status> {
        let session = self.session_for(payload)?;
        let handler = session.conversation_spawn_handler.ok_or_else(|| {
            Status::failed_precondition("this session's recipe does not spawn conversations")
        })?;
        let wire: SpawnConversationRequestWire = serde_json::from_slice(payload).map_err(|e| {
            Status::invalid_argument(format!("invalid spawn-conversation request: {e}"))
        })?;
        let session_id = handler
            .spawn_conversation(
                &wire.prompt,
                wire.branch.as_deref(),
                wire.base_ref.as_deref(),
            )
            .await
            .map_err(Status::internal)?;
        serde_json::to_vec(&serde_json::json!({ "session_id": session_id }))
            .map_err(|e| Status::internal(format!("serialize spawn-conversation response: {e}")))
    }

    async fn handle_github_token(&self, payload: &[u8]) -> Result<Vec<u8>, Status> {
        let session = self.session_for(payload)?;
        let handler = session.github_credential_handler.ok_or_else(|| {
            Status::failed_precondition(
                "no GitHub account can be resolved for this session: its host has no credential \
                 handler for it",
            )
        })?;
        // The refusal is the resolver's own words, verbatim; the token is never logged here.
        let token = handler
            .github_token()
            .await
            .map_err(Status::failed_precondition)?;
        serde_json::to_vec(&serde_json::json!({ "token": token }))
            .map_err(|e| Status::internal(format!("serialize github-token response: {e}")))
    }
}

#[async_trait]
impl RpcService for HostSessionService {
    async fn handle_rpc(&self, service: &str, method: &str, message: &RpcMessage) -> RpcResult {
        if service != HOST_SESSION_SERVICE {
            return RpcResult::Unary(Err(Status::unimplemented(format!(
                "HostSessionService has no method {service}/{method}"
            ))));
        }
        RpcResult::Unary(match method {
            SPAWN_CONVERSATION_METHOD => self.handle_spawn_conversation(&message.payload).await,
            GITHUB_TOKEN_METHOD => self.handle_github_token(&message.payload).await,
            _ => Err(Status::unimplemented(format!(
                "HostSessionService has no method {service}/{method}"
            ))),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `(prompt, branch, base_ref)` the fake handler last saw, shared with the test body.
    type SeenSpawnArgs = Arc<Mutex<Option<(String, Option<String>, Option<String>)>>>;

    /// Records the arguments it was called with and returns a fixed child session id.
    struct FakeConversationSpawn {
        session_id: String,
        seen: SeenSpawnArgs,
    }

    #[async_trait]
    impl ConversationSpawnHandler for FakeConversationSpawn {
        async fn spawn_conversation(
            &self,
            prompt: &str,
            branch: Option<&str>,
            base_ref: Option<&str>,
        ) -> Result<String, String> {
            *self.seen.lock().unwrap() = Some((
                prompt.to_string(),
                branch.map(str::to_string),
                base_ref.map(str::to_string),
            ));
            Ok(self.session_id.clone())
        }
    }

    struct FailingSpawn;
    #[async_trait]
    impl ConversationSpawnHandler for FailingSpawn {
        async fn spawn_conversation(
            &self,
            _prompt: &str,
            _branch: Option<&str>,
            _base_ref: Option<&str>,
        ) -> Result<String, String> {
            Err("worktree already exists".to_string())
        }
    }

    /// Answers with a fixed outcome: the token of "its" account, or a refusal.
    struct FixedCredential(Result<String, String>);
    #[async_trait]
    impl GithubCredentialHandler for FixedCredential {
        async fn github_token(&self) -> Result<String, String> {
            self.0.clone()
        }
    }

    fn a_session_of(os_user: &str) -> RegisteredSession {
        RegisteredSession {
            os_user: os_user.to_string(),
            conversation_spawn_handler: None,
            github_credential_handler: None,
        }
    }

    impl RegisteredSession {
        fn with_token(mut self, token: &str) -> Self {
            self.github_credential_handler = Some(Arc::new(FixedCredential(Ok(token.to_string()))));
            self
        }
        fn refusing(mut self, reason: &str) -> Self {
            self.github_credential_handler =
                Some(Arc::new(FixedCredential(Err(reason.to_string()))));
            self
        }
        fn spawning(mut self, handler: Arc<dyn ConversationSpawnHandler>) -> Self {
            self.conversation_spawn_handler = Some(handler);
            self
        }
    }

    /// A socket owned by `owner` whose registry holds `sessions`.
    fn a_host_socket_of(owner: &str, sessions: &[(&str, RegisteredSession)]) -> HostSessionService {
        let registry = Arc::new(HostSessionRegistry::default());
        for (id, session) in sessions {
            registry.register(id, session.clone());
        }
        HostSessionService::new(owner, registry)
    }

    fn request_bytes(json: serde_json::Value) -> RpcMessage {
        RpcMessage::new(
            serde_json::to_vec(&json).unwrap(),
            tddy_rpc::RequestMetadata::over(tddy_rpc::RequestTransport::Direct),
        )
    }

    async fn call(
        service: &HostSessionService,
        method: &str,
        json: serde_json::Value,
    ) -> Result<serde_json::Value, Status> {
        match service
            .handle_rpc(HOST_SESSION_SERVICE, method, &request_bytes(json))
            .await
        {
            RpcResult::Unary(Ok(bytes)) => Ok(serde_json::from_slice(&bytes).unwrap()),
            RpcResult::Unary(Err(status)) => Err(status),
            _ => panic!("expected a unary response"),
        }
    }

    fn refusal_text(outcome: Result<serde_json::Value, Status>) -> String {
        format!("{:?}", outcome.expect_err("the request must be refused"))
    }

    #[tokio::test]
    async fn spawn_conversation_routes_to_the_named_sessions_handler_and_returns_the_child_session_id(
    ) {
        // Given a session of "ada" whose handler yields "child-123"
        let seen = Arc::new(Mutex::new(None));
        let handler = Arc::new(FakeConversationSpawn {
            session_id: "child-123".to_string(),
            seen: seen.clone(),
        });
        let socket = a_host_socket_of("ada", &[("s1", a_session_of("ada").spawning(handler))]);

        // When that session relays a spawn-conversation request
        let answer = call(
            &socket,
            SPAWN_CONVERSATION_METHOD,
            serde_json::json!({
                "session_id": "s1",
                "type": "spawn-conversation",
                "prompt": "implement the brief",
                "branch": "feat-x",
            }),
        )
        .await
        .expect("answered");

        // Then the handler received the prompt/branch (base_ref absent) and the answer carries the id
        assert_eq!(answer["session_id"], "child-123");
        let seen = seen.lock().unwrap().clone().expect("handler was called");
        assert_eq!(seen.0, "implement the brief");
        assert_eq!(seen.1.as_deref(), Some("feat-x"));
        assert_eq!(seen.2, None, "base_ref must be None when omitted");
    }

    #[tokio::test]
    async fn a_spawn_conversation_handler_error_surfaces_verbatim() {
        // Given a session whose handler fails
        let socket = a_host_socket_of(
            "ada",
            &[("s1", a_session_of("ada").spawning(Arc::new(FailingSpawn)))],
        );

        // When it relays a spawn
        let outcome = call(
            &socket,
            SPAWN_CONVERSATION_METHOD,
            serde_json::json!({ "session_id": "s1", "type": "spawn-conversation", "prompt": "x" }),
        )
        .await;

        // Then the handler's words are the refusal
        assert!(refusal_text(outcome).contains("worktree already exists"));
    }

    #[tokio::test]
    async fn a_session_whose_recipe_spawns_no_conversations_is_refused_one() {
        // Given a session with no conversation handler (any recipe but grill-me)
        let socket = a_host_socket_of("ada", &[("s1", a_session_of("ada"))]);

        // When it asks to spawn a conversation
        let outcome = call(
            &socket,
            SPAWN_CONVERSATION_METHOD,
            serde_json::json!({ "session_id": "s1", "type": "spawn-conversation", "prompt": "x" }),
        )
        .await;

        // Then it is refused rather than silently spawning
        assert!(refusal_text(outcome).contains("does not spawn conversations"));
    }

    #[tokio::test]
    async fn the_token_of_the_named_session_is_returned() {
        // Given two sessions of one user, each acting as a different account
        let socket = a_host_socket_of(
            "ada",
            &[
                ("s-ada", a_session_of("ada").with_token("ghp_ada_token")),
                ("s-grace", a_session_of("ada").with_token("ghp_grace_token")),
            ],
        );

        // When each asks over the one socket
        let for_ada = call(
            &socket,
            GITHUB_TOKEN_METHOD,
            serde_json::json!({"session_id": "s-ada"}),
        )
        .await;
        let for_grace = call(
            &socket,
            GITHUB_TOKEN_METHOD,
            serde_json::json!({"session_id": "s-grace"}),
        )
        .await;

        // Then each gets its own account's token, never the other's
        assert_eq!(for_ada.unwrap()["token"], "ghp_ada_token");
        assert_eq!(for_grace.unwrap()["token"], "ghp_grace_token");
    }

    #[tokio::test]
    async fn a_refused_resolution_travels_as_the_resolvers_own_words() {
        // Given two sessions refused for different reasons
        let socket = a_host_socket_of(
            "ada",
            &[
                (
                    "unassigned",
                    a_session_of("ada").refusing("this project assigns no GitHub account"),
                ),
                (
                    "unknown",
                    a_session_of("ada").refusing("the assigned account is not held on this host"),
                ),
            ],
        );

        // When each asks
        let unassigned = call(
            &socket,
            GITHUB_TOKEN_METHOD,
            serde_json::json!({"session_id": "unassigned"}),
        )
        .await;
        let unknown = call(
            &socket,
            GITHUB_TOKEN_METHOD,
            serde_json::json!({"session_id": "unknown"}),
        )
        .await;

        // Then the two refusals are distinct and verbatim
        let unassigned = refusal_text(unassigned);
        let unknown = refusal_text(unknown);
        assert!(unassigned.contains("this project assigns no GitHub account"));
        assert!(unknown.contains("the assigned account is not held on this host"));
        assert_ne!(unassigned, unknown);
    }

    #[tokio::test]
    async fn an_unknown_session_is_refused_and_no_token_is_returned() {
        // Given a socket that knows one session
        let socket = a_host_socket_of(
            "ada",
            &[("s1", a_session_of("ada").with_token("ghp_secret"))],
        );

        // When another id asks
        let outcome = call(
            &socket,
            GITHUB_TOKEN_METHOD,
            serde_json::json!({"session_id": "s-other"}),
        )
        .await;

        // Then it is refused, naming the session and carrying no token
        let text = refusal_text(outcome);
        assert!(
            text.contains("s-other") && !text.contains("ghp_secret"),
            "{text}"
        );
    }

    #[tokio::test]
    async fn a_session_of_another_os_user_is_refused_over_this_users_socket() {
        // Given ada's socket and a session registered as bob's
        let socket = a_host_socket_of(
            "ada",
            &[("bobs", a_session_of("bob").with_token("ghp_bob_token"))],
        );

        // When bob's session id is asked about over ada's socket
        let outcome = call(
            &socket,
            GITHUB_TOKEN_METHOD,
            serde_json::json!({"session_id": "bobs"}),
        )
        .await;

        // Then it is refused as a different user's, with no token
        let text = refusal_text(outcome);
        assert!(
            text.contains("different OS user") && !text.contains("ghp_bob_token"),
            "{text}"
        );
    }

    #[tokio::test]
    async fn a_deleted_session_is_refused() {
        // Given a registered session that is then unregistered
        let registry = Arc::new(HostSessionRegistry::default());
        registry.register("s1", a_session_of("ada").with_token("ghp_ada_token"));
        let socket = HostSessionService::new("ada", registry.clone());
        registry.unregister("s1");

        // When it asks
        let outcome = call(
            &socket,
            GITHUB_TOKEN_METHOD,
            serde_json::json!({"session_id": "s1"}),
        )
        .await;

        // Then it is refused
        assert!(refusal_text(outcome).contains("no session `s1`"));
    }

    /// A socket of `owner` that knows the sessions `existing` as ones that exist on disk.
    fn a_host_socket_that_knows_sessions_exist(
        owner: &str,
        existing: &[&str],
    ) -> HostSessionService {
        let existing: Vec<String> = existing.iter().map(|s| s.to_string()).collect();
        HostSessionService::new(owner, Arc::new(HostSessionRegistry::default())).with_session_probe(
            Arc::new(move |os_user, session_id| {
                os_user == "ada" && existing.iter().any(|known| known == session_id)
            }),
        )
    }

    #[tokio::test]
    async fn a_session_that_exists_but_is_not_registered_is_told_to_resume_it() {
        // Given a host that restarted: its registry is empty, but session s1 exists on disk
        let socket = a_host_socket_that_knows_sessions_exist("ada", &["s1"]);

        // When the still-running session asks for a token
        let outcome = call(
            &socket,
            GITHUB_TOKEN_METHOD,
            serde_json::json!({"session_id": "s1"}),
        )
        .await;

        // Then the refusal says what to do, and is not the unknown-session refusal
        let status = outcome.expect_err("refused");
        assert_eq!(status.code(), tddy_rpc::Code::FailedPrecondition);
        assert_eq!(
            status.message(),
            "this session was started before the daemon restarted; resume it to re-enable GitHub tools"
        );
    }

    #[tokio::test]
    async fn a_session_that_does_not_exist_is_not_told_to_resume() {
        // Given the same host
        let socket = a_host_socket_that_knows_sessions_exist("ada", &["s1"]);

        // When a session that exists nowhere asks
        let outcome = call(
            &socket,
            GITHUB_TOKEN_METHOD,
            serde_json::json!({"session_id": "gone"}),
        )
        .await;

        // Then it is not found, and nothing suggests resuming
        let status = outcome.expect_err("refused");
        assert_eq!(status.code(), tddy_rpc::Code::NotFound);
        assert!(!status.message().contains("resume"), "{}", status.message());
    }

    #[tokio::test]
    async fn the_existence_of_another_os_users_session_is_not_revealed() {
        // Given bob's socket, and a probe that knows only ada's sessions
        let other = HostSessionService::new("bob", Arc::new(HostSessionRegistry::default()))
            .with_session_probe(Arc::new(|os_user, _| os_user == "ada"));

        // When bob's socket is asked about ada's session
        let outcome = call(
            &other,
            GITHUB_TOKEN_METHOD,
            serde_json::json!({"session_id": "adas"}),
        )
        .await;

        // Then it is the plain not-found: the probe is asked about bob, not about ada
        assert_eq!(
            outcome.expect_err("refused").code(),
            tddy_rpc::Code::NotFound
        );
    }

    #[test]
    fn a_stopped_process_unregisters_its_session_only_while_it_is_still_the_registered_one() {
        // Given a session registered with a process, then registered again by a resume
        let registry = HostSessionRegistry::default();
        registry.register("s1", a_session_of("ada"));
        registry.attach_process("s1", 100);
        registry.register("s1", a_session_of("ada"));
        registry.attach_process("s1", 200);

        // When the first process is reported stopped, and then the second
        let stale_report = registry.unregister_stopped("s1", 100);
        let current_report = registry.unregister_stopped("s1", 200);

        // Then the stale report removed nothing and the current one removed the session
        assert!(!stale_report);
        assert!(current_report);
        assert!(registry.get("s1").is_none());
    }

    #[test]
    fn a_session_registered_with_no_process_is_not_removed_by_a_stopped_report() {
        // Given a session that has not been given its process yet
        let registry = HostSessionRegistry::default();
        registry.register("s1", a_session_of("ada"));

        // When some process is reported stopped under its id
        let removed = registry.unregister_stopped("s1", 100);

        // Then it stays
        assert!(!removed);
        assert!(registry.get("s1").is_some());
    }

    #[tokio::test]
    async fn a_request_that_names_no_session_is_refused() {
        // Given a socket with a session
        let socket = a_host_socket_of(
            "ada",
            &[("s1", a_session_of("ada").with_token("ghp_ada_token"))],
        );

        // When a request carries no session_id
        let outcome = call(&socket, GITHUB_TOKEN_METHOD, serde_json::json!({})).await;

        // Then it is refused, not answered for some default session
        assert!(refusal_text(outcome).contains("must name its session"));
    }

    #[tokio::test]
    async fn a_session_with_no_credential_handler_is_refused_a_token() {
        // Given a session whose project could not be read (no handler)
        let socket = a_host_socket_of("ada", &[("s1", a_session_of("ada"))]);

        // When it asks for a token
        let outcome = call(
            &socket,
            GITHUB_TOKEN_METHOD,
            serde_json::json!({"session_id": "s1"}),
        )
        .await;

        // Then there is nothing to fall back to
        assert!(refusal_text(outcome).contains("no credential handler"));
    }

    #[tokio::test]
    async fn an_unknown_method_is_unimplemented_so_an_older_host_is_additive() {
        // Given a socket
        let socket = a_host_socket_of("ada", &[]);

        // When a method this host does not have is called
        let outcome = call(&socket, "Nope", serde_json::json!({})).await;

        // Then it is unimplemented
        assert!(refusal_text(outcome)
            .to_lowercase()
            .contains("unimplemented"));
    }
}
