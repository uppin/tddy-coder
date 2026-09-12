//! Acceptance: an agent **inside a real jail** holds a subagent conversation through the relay
//! allowlist, on the coordinate `#unbundle` node 7 moved family B to.
//!
//! Feature: docs/ft/daemon/session-agent-roster.md (§ Prompting an agent), and
//! docs/dev/changesets/2026-09-09-unbundle-session-agent-services.md § Five of family B's nine are a
//! security boundary.
//!
//! `packages/tddy-sandbox-runner/src/runner.rs` holds the `(service, method)` allowlist of what an
//! in-jail agent may relay to its host, and five of its entries are family B. When node 7 moved
//! the coordinate, that list had to move with it — and the failure mode of getting it wrong is the
//! reason this suite spawns a real Seatbelt jail instead of reading the list: a mismatched
//! allowlist fails **closed**, silently, at runtime. `tddy-session-agents`' unit tests already pin
//! the permitted *set* and its service name; a test that also only read tuples would pass while
//! every conversation held from inside a jail was broken.
//!
//! So the whole chain runs for real: a jailed `tddy-sandbox-runner --stdio`, the production
//! `dial_and_bridge` with the daemon's own `HostRpcHandler` behind it, and the three conversation
//! RPCs issued over the jail's tool-IPC socket — the only transport an in-jail agent has. The
//! coordinate is spelled out here as a literal rather than read from
//! `tddy_service::session_agents::IN_JAIL_RELAYABLE`: the runner's allowlist and the daemon's
//! served entry both read that constant, so a test that read it too would agree with them by
//! construction and prove nothing about the name.
//!
//! Where this suite differs from `sandbox_session_stdio_acceptance.rs`, which it is modelled on:
//! that one drives the terminal bridge and the tool relay and so passes `NullRpcHandler`, which
//! refuses every host RPC. Here the host RPC *is* the subject, so the handler is the real one.

#![cfg(target_os = "macos")]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use prost::Message as _;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::SessionMetadata;
use tddy_daemon::connection_service::DaemonSessionHost;
use tddy_daemon::test_util::{test_service, TestDaemon, TEST_TOKEN};
use tddy_daemon_sandbox::sandbox_session::{
    build_sandbox_runner_env, dial_and_bridge, pick_free_loopback_port, spawn_sandbox_runner,
    SandboxRunnerSpawn,
};
use tddy_rpc::Request;
use tddy_service::proto::catalog::{CatalogService, ListSubagentsRequest};
use tddy_service::proto::session_agents_svc::{
    AgentConversationChunk, AttachSessionAgentRequest, CancelAgentConversationRequest,
    CancelAgentConversationResponse, OpenAgentConversationRequest, OpenAgentConversationResponse,
    PromptAgentConversationRequest, SessionAgentRoster, SessionAgentService as _,
    StreamSessionAgentsRequest,
};
use tokio::sync::{broadcast, mpsc};

/// The coordinate an in-jail agent addresses family B at. Spelled out, not imported — see the
/// module header.
const SESSION_AGENT_SERVICE: &str = "session_agents.SessionAgentService";

/// How long the jailed runner is given to write its ready marker. A Seatbelt jail plus a process
/// start, on a loaded machine.
const THE_JAIL_COMES_UP_WITHIN: Duration = Duration::from_secs(15);

/// How long one relayed RPC is given: jail → host over stdio, a turn against a local stub model,
/// and back. Generous enough that a slow machine is not a failure and short enough that a wedged
/// relay fails rather than hangs.
const A_RELAYED_CALL_ANSWERS_WITHIN: Duration = Duration::from_secs(15);

/// How long the host's `SessionChannel` is given to reach the jail after `dial_and_bridge` has
/// sent it. One stdio round trip, with a jailed process scheduling on the other end.
const THE_RELAY_ATTACHES_WITHIN: Duration = Duration::from_secs(10);

/// The answer the agent's model gives, distinctive enough that finding it on the far side of the
/// relay cannot be a coincidence.
const THE_AGENTS_ANSWER: &str = "main lives in src/main.rs";

// ---------------------------------------------------------------------------
// The behaviour
// ---------------------------------------------------------------------------

/// An in-jail agent opens a conversation, prompts it and cancels it, all relayed to the host at
/// `session_agents.SessionAgentService`. Three of the five allowlisted family-B operations, in the
/// order a real subagent conversation performs them.
///
/// One test rather than three because a jail spawn is the expensive part and a conversation is one
/// behaviour: an open nothing can prompt, or a prompt nothing can cancel, is not a working
/// conversation. What each call is asserted on is its own outcome — the conversation id the open
/// returns, the model's answer arriving whole through the prompt stream, and the cancel being
/// accepted for the conversation that open created.
#[tokio::test(flavor = "multi_thread")]
async fn an_in_jail_agent_holds_a_conversation_on_the_new_coordinate() {
    // Given a daemon serving one session with one local agent attached, and a real Seatbelt jail
    // bridged to it by the production `dial_and_bridge` with the daemon's own host RPC handler
    let model = a_model_answering(THE_AGENTS_ANSWER).await;
    let daemon = a_daemon_with_one_agent_attached(&model.base_url).await;
    let jail = a_seatbelt_jail_bridged_to(&daemon).await;

    // When the three conversation RPCs are issued over the jail's tool-IPC socket, which is the
    // only transport an in-jail agent has
    let opened: OpenAgentConversationResponse = jail
        .relay_unary(
            "OpenAgentConversation",
            OpenAgentConversationRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: daemon.session_id.clone(),
                daemon_instance_id: String::new(),
                agent_id: daemon.agent_id.clone(),
                conversation_id: String::new(),
            },
        )
        .await
        .expect("opening a conversation from inside the jail must be relayed and answered");
    let conversation_id = opened.conversation_id;
    assert!(
        !conversation_id.is_empty(),
        "the relayed open must name the conversation it created"
    );

    let answer = jail
        .relay_prompt(PromptAgentConversationRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: daemon.session_id.clone(),
            daemon_instance_id: String::new(),
            conversation_id: conversation_id.clone(),
            prompt: "where is main?".to_string(),
        })
        .await;

    let cancelled: Result<CancelAgentConversationResponse, tddy_rpc::Status> = jail
        .relay_unary(
            "CancelAgentConversation",
            CancelAgentConversationRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: daemon.session_id.clone(),
                daemon_instance_id: String::new(),
                conversation_id: conversation_id.clone(),
            },
        )
        .await;

    // Then the agent answered through the relay, and the conversation it answered on could be
    // closed from the same place
    assert_eq!(
        answer
            .iter()
            .map(|frame| frame.content_chunk.as_str())
            .collect::<String>(),
        THE_AGENTS_ANSWER,
        "the agent's answer must arrive whole on the far side of the jail relay"
    );
    assert!(
        answer.last().is_some_and(|frame| frame.last),
        "the relayed prompt stream must end with a frame marked last, or an in-jail caller waits \
         forever: {answer:?}"
    );
    cancelled
        .expect("cancelling the conversation from inside the jail must be relayed and accepted");
}

// ---------------------------------------------------------------------------
// The jail
// ---------------------------------------------------------------------------

/// A live Seatbelt jail whose runner is bridged to a daemon, plus the tool-IPC socket an in-jail
/// caller reaches that daemon through.
struct BridgedJail {
    handle: tddy_sandbox::SandboxHandle,
    /// The transport an in-jail agent's RPCs ride. One connection, as `tddy-tools` holds one.
    transport: Arc<dyn tddy_rpc::RpcClientTransport>,
    _project: tempfile::TempDir,
}

impl BridgedJail {
    /// Relay one unary family-B RPC the way the in-jail `tddy-tools` does, and decode its reply.
    async fn relay_unary<Req, Resp>(
        &self,
        method: &str,
        request: Req,
    ) -> Result<Resp, tddy_rpc::Status>
    where
        Req: prost::Message,
        Resp: prost::Message + Default,
    {
        let bytes = tokio::time::timeout(
            A_RELAYED_CALL_ANSWERS_WITHIN,
            self.transport
                .call_unary(SESSION_AGENT_SERVICE, method, request.encode_to_vec()),
        )
        .await
        .unwrap_or_else(|_| panic!("the relayed {method} never answered"))?;
        Ok(Resp::decode(bytes.as_slice()).expect("the relayed reply must decode"))
    }

    /// Relay `PromptAgentConversation` and drain its server stream to the frame marked `last`.
    async fn relay_prompt(
        &self,
        request: PromptAgentConversationRequest,
    ) -> Vec<AgentConversationChunk> {
        let mut frames = tokio::time::timeout(
            A_RELAYED_CALL_ANSWERS_WITHIN,
            self.transport.call_server_stream(
                SESSION_AGENT_SERVICE,
                "PromptAgentConversation",
                request.encode_to_vec(),
            ),
        )
        .await
        .expect("the relayed PromptAgentConversation never opened its stream")
        .expect("the relayed PromptAgentConversation must be accepted");

        let mut answer = Vec::new();
        tokio::time::timeout(A_RELAYED_CALL_ANSWERS_WITHIN, async {
            while let Some(frame) = frames.recv().await {
                let bytes = frame.expect("the relayed prompt stream must not error");
                let frame = AgentConversationChunk::decode(bytes.as_slice())
                    .expect("a relayed conversation chunk must decode");
                let last = frame.last;
                answer.push(frame);
                if last {
                    return;
                }
            }
        })
        .await
        .expect("the relayed prompt stream never produced its final frame");
        answer
    }
}

/// Tear the jail down on every exit from the test, a failed assertion included: a panic that
/// skipped an explicit teardown would leave a jailed process running for as long as the machine is
/// up, and the next run would start beside it.
impl Drop for BridgedJail {
    fn drop(&mut self) {
        self.handle.child_mut().kill().ok();
        self.handle.child_mut().wait().ok();
    }
}

/// Spawn a real Seatbelt-jailed `tddy-sandbox-runner --stdio` and bridge it to `daemon` through the
/// production `dial_and_bridge`, with `daemon`'s own `HostRpcHandler` serving the relayed RPCs.
///
/// The jail layout follows `sandbox_session_stdio_acceptance.rs`: `--stdio` and no gRPC flags, the
/// project canonicalized before the socket is bound because Seatbelt matches SBPL rules against
/// canonical paths.
async fn a_seatbelt_jail_bridged_to(daemon: &DaemonServingOneAgent) -> BridgedJail {
    let tmp = tempfile::tempdir().expect("jail project tempdir");
    let project = tmp.path().join("project");
    let egress = tmp.path().join("egress");
    std::fs::create_dir_all(project.join(".work").join("home")).expect("create jail home");
    std::fs::create_dir_all(project.join(".work").join("tmp")).expect("create jail tmp");
    std::fs::create_dir_all(project.join("context")).expect("create context dir");
    std::fs::create_dir_all(&egress).expect("create egress dir");
    let project = std::fs::canonicalize(&project).expect("canonicalize project");
    let egress = std::fs::canonicalize(&egress).expect("canonicalize egress");
    let scratch = project.join(".work");
    let context = project.join("context");
    let worktree = project.join("worktree");
    std::fs::create_dir_all(&worktree).expect("create worktree");

    let runner = a_built_binary("tddy-sandbox-runner");
    let tools = a_built_binary("tddy-tools");

    let ready_marker = project.join("sandbox.ready");
    let tool_ipc_socket = project.join("tool_ipc.sock");
    let session_id = daemon.session_id.as_str();
    let runner_argv = vec![
        runner.to_string_lossy().to_string(),
        "--session-id".into(),
        session_id.to_string(),
        "--context-dir".into(),
        context.to_string_lossy().to_string(),
        "--tool-ipc-socket".into(),
        tool_ipc_socket.to_string_lossy().to_string(),
        "--tddy-tools-path".into(),
        tools.to_string_lossy().to_string(),
        "--ready-marker".into(),
        ready_marker.to_string_lossy().to_string(),
        // The jailed agent process itself is not the subject: what is relayed out of the jail is.
        // The same PTY leader `sandbox_session_stdio_acceptance` uses, so the two real-jail
        // fixtures differ in exactly one thing: the host RPC handler behind the relay.
        "--claude-binary".into(),
        "/bin/sleep".into(),
        "--model".into(),
        "claude-opus-4-8".into(),
        "--permission-mode".into(),
        "auto".into(),
        "--stdio".into(),
    ];

    let mut env = build_sandbox_runner_env(
        &scratch.join("home"),
        &scratch.join("tmp"),
        session_id,
        &tool_ipc_socket,
        &egress,
    );
    env.insert(
        "TDDY_SANDBOX_EGRESS_DIR".into(),
        egress.to_string_lossy().to_string(),
    );

    let shim_port = pick_free_loopback_port().expect("egress shim port");
    let mut handle = spawn_sandbox_runner(SandboxRunnerSpawn {
        project_root: project.clone(),
        scratch_dir: scratch,
        egress_dir: egress.clone(),
        profile_path: project.join("profile.sb"),
        runner_argv,
        env,
        loopback_allow_ports: vec![shim_port],
        ipc_socket: None,
        mounts: vec![],
        host_home: None,
        cgroup: Default::default(),
    })
    .expect("spawn the Seatbelt-jailed sandbox-runner");

    await_the_ready_marker(&mut handle, &ready_marker).await;

    let (stdout_tx, _stdout_rx) = broadcast::channel(16);
    let (_stdin_tx, stdin_rx) = mpsc::unbounded_channel();
    tokio::time::timeout(
        A_RELAYED_CALL_ANSWERS_WITHIN,
        dial_and_bridge(
            session_id,
            worktree,
            &mut handle,
            tddy_task::TaskRegistry::default(),
            stdout_tx,
            Arc::new(StdMutex::new(tddy_task::TerminalCapture::new())),
            stdin_rx,
            Arc::new(Vec::new()),
            tmp.path().join("session"),
            Arc::new(tddy_daemon_kernel::AgentActivityHub::default()),
            // The real handler, not `NullRpcHandler`: this suite's whole subject is what it
            // answers, and it is the same object the three sandboxed-session spawn paths build.
            daemon.service.sandbox_rpc_handler(),
        ),
    )
    .await
    .expect("dial_and_bridge timed out")
    .expect("dial_and_bridge over the jailed runner's stdio");

    await_the_host_relay(&mut handle, &egress, &tool_ipc_socket, &daemon.session_id).await;

    let transport = tddy_session_tool_client::connect_sandbox_ipc(&tool_ipc_socket)
        .await
        .expect("an in-jail caller connects to the tool-IPC socket");

    BridgedJail {
        handle,
        transport,
        _project: tmp,
    }
}

/// Wait until a relayed family-B call actually reaches the host and comes back.
///
/// FIXME(sandbox-stdio-attach): the jail's end of the `SessionChannel` does not always attach, and
/// when it does not, nothing relayed out of the jail can ever be answered — the runner refuses with
/// "the sandbox session channel is not connected to the host daemon yet" while its tool-IPC socket
/// answers normally and its own log shows `SandboxService serving over stdio`.
///
/// Measured on macOS: the first run in a fresh process/shell attaches in ~50ms and the whole
/// conversation completes in ~0.5s; back-to-back repeats then fail about three runs in four.
/// Killing the jails leaked by a failed run does not change that rate, so it is not contention
/// from leftovers.
///
/// The race is **not** family B's. The untouched
/// `sandbox_session_stdio_acceptance::real_daemon_session_drives_a_seatbelt_jailed_sandbox_runner_entirely_over_stdio`
/// fails the same way (1 of 3 back-to-back runs), and with `NullRpcHandler` in place of the
/// daemon's handler this suite's probe comes back with that handler's own
/// "this host does not serve session_agents.SessionAgentService/StreamSessionAgents" — the relay
/// carrying a family-B call to the host and the refusal back, at the new coordinate. So the defect
/// is in the stdio bridge (`tddy-daemon-sandbox::bridge_sandbox_stdio` and the runner's
/// `StdioEndpoint::from_process_stdio`), which `#unbundle` node 7 does not own.
///
/// Reported rather than worked around: a setup retry would hide a real defect behind a suite that
/// looks green, and the retry experiment showed the conversation timing out even after a later
/// attempt attached.
async fn await_the_host_relay(
    handle: &mut tddy_sandbox::SandboxHandle,
    egress_dir: &Path,
    socket: &Path,
    session_id: &str,
) {
    let request = StreamSessionAgentsRequest {
        session_token: TEST_TOKEN.to_string(),
        session_id: session_id.to_string(),
        daemon_instance_id: String::new(),
    }
    .encode_to_vec();

    // Bounded from the outside as well as per attempt. Neither `connect_sandbox_ipc` nor a relayed
    // call carries a deadline of its own, so a retry loop that only checked the clock *between*
    // attempts would hang forever on the first attempt that never came back — which is exactly
    // what an unattached relay can do.
    //
    // The last refusal is kept and reported: "the relay never attached" and "the relay attached and
    // refused the token" are different bug reports, and a bare timeout tells them apart for nobody.
    let last_refusal = Arc::new(StdMutex::new(String::from(
        "the relay was never asked anything",
    )));
    let seen = Arc::clone(&last_refusal);
    let waited = tokio::time::timeout(THE_RELAY_ATTACHES_WITHIN, async move {
        while let Some(refusal) = a_probe_of_the_relay(socket, &request).await {
            *seen.lock().expect("the refusal cell is not poisoned") = refusal;
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await;

    if waited.is_err() {
        // A jail that died on the way is a different bug report from one that is up and refusing,
        // and the child's own exit reason is the only thing that tells them apart.
        let died = handle
            .try_exit_diagnostic()
            .unwrap_or_else(|| "the jailed runner is still running".to_string());
        panic!(
            "no relayed family-B call reached the host within {THE_RELAY_ATTACHES_WITHIN:?}; last \
             refusal: {}; jail: {died}\n{}",
            last_refusal
                .lock()
                .expect("the refusal cell is not poisoned"),
            // The jailed runner's own log, the way the production spawn path reports a sandbox
            // failure: all this test can see from outside is "it refused", and the reason was
            // written in there.
            tddy_sandbox::format_egress_logs(egress_dir)
        );
    }
}

/// One attempt at the wait above: `None` once a roster frame has come back through the relay,
/// `Some(reason)` while it has not.
///
/// A connection per attempt, dropped with it: the socket hands each caller its own, and a probe
/// sharing one with the conversation under test would interleave with frames it does not read.
/// Each attempt is bounded too, so a connection that hangs costs one attempt rather than the wait.
async fn a_probe_of_the_relay(socket: &Path, request: &[u8]) -> Option<String> {
    let attempt = async {
        let transport = tddy_session_tool_client::connect_sandbox_ipc(socket)
            .await
            .map_err(|e| format!("tool-IPC connect: {e}"))?;
        let mut frames = transport
            .call_server_stream(
                SESSION_AGENT_SERVICE,
                "StreamSessionAgents",
                request.to_vec(),
            )
            .await
            .map_err(|status| status.message().to_string())?;
        match frames.recv().await {
            Some(Ok(bytes)) => {
                SessionAgentRoster::decode(bytes.as_slice())
                    .expect("a relayed roster snapshot must decode");
                Ok(())
            }
            Some(Err(status)) => Err(status.message().to_string()),
            None => Err("the roster stream ended without a frame".to_string()),
        }
    };
    match tokio::time::timeout(A_RELAYED_CALL_ANSWERS_WITHIN, attempt).await {
        Ok(Ok(())) => None,
        Ok(Err(refusal)) => Some(refusal),
        Err(_) => Some("the probe itself never came back".to_string()),
    }
}

/// Wait for the jailed runner to declare itself ready, failing with the child's own exit reason
/// rather than a bare timeout when it died on the way up.
async fn await_the_ready_marker(handle: &mut tddy_sandbox::SandboxHandle, ready_marker: &Path) {
    let deadline = std::time::Instant::now() + THE_JAIL_COMES_UP_WITHIN;
    while std::time::Instant::now() < deadline {
        if ready_marker.exists() {
            return;
        }
        if let Some(reason) = handle.try_exit_diagnostic() {
            panic!("the jailed sandbox-runner died before its ready marker: {reason}");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the jailed sandbox-runner never wrote its ready marker within {THE_JAIL_COMES_UP_WITHIN:?}");
}

/// A binary this suite drives, from Cargo's own path when it built it and from `target/debug`
/// otherwise, so `cargo build -p <it>` is enough to run the suite.
fn a_built_binary(name: &str) -> PathBuf {
    let path = std::env::var_os(format!("CARGO_BIN_EXE_{name}"))
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/debug")
                .join(name)
        });
    assert!(path.exists(), "build {name} first (cargo build -p {name})");
    path
}

// ---------------------------------------------------------------------------
// The daemon behind the jail
// ---------------------------------------------------------------------------

/// The host side of the relay: a daemon whose session has one local agent to converse with.
struct DaemonServingOneAgent {
    service: Arc<DaemonSessionHost>,
    session_id: String,
    agent_id: String,
    _data_dir: tempfile::TempDir,
}

/// A daemon whose `agents/` directory defines one agent pointed at `model_base_url`, attached to a
/// live session.
///
/// Held as an `Arc` with its self-handle set, because `sandbox_rpc_handler` recovers that `Arc` —
/// the same wiring `runtime::build` does right after its own `Arc::new`.
async fn a_daemon_with_one_agent_attached(model_base_url: &str) -> DaemonServingOneAgent {
    let data_dir = tempfile::tempdir().expect("daemon data tempdir");
    write_agent_def(data_dir.path(), "explorer", model_base_url);

    let session_id = "1780828020298-in-jail-conversation".to_string();
    let session_dir = unified_session_dir_path(data_dir.path(), &session_id);
    std::fs::create_dir_all(&session_dir).expect("create session dir");
    tddy_core::write_session_metadata(&session_dir, &a_sandboxed_session(&session_id))
        .expect("write session metadata");

    let service = test_service(data_dir.path().to_path_buf()).as_arc();
    service.install_sandbox_rpc_bridge();

    // Read the agent id the way a client reads it: a hand-spelled "explorer@some-host" would pass
    // while the daemon stamped something else entirely.
    let agent_id = CatalogService::list_subagents(
        &TestDaemon::from_arc(Arc::clone(&service)),
        Request::new(ListSubagentsRequest {}),
    )
    .await
    .expect("listing subagents must succeed")
    .into_inner()
    .subagents
    .into_iter()
    .find(|s| s.name == "explorer")
    .expect("the fixture must advertise a def named 'explorer'")
    .agent_id;
    service
        .session_agents_service()
        .attach_session_agent(Request::new(AttachSessionAgentRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: session_id.clone(),
            daemon_instance_id: String::new(),
            agent_id: agent_id.clone(),
        }))
        .await
        .expect("attaching a local agent must succeed");

    DaemonServingOneAgent {
        service,
        session_id,
        agent_id,
        _data_dir: data_dir,
    }
}

fn write_agent_def(tddy_data_dir: &Path, name: &str, base_url: &str) {
    let agents_dir = tddy_data_dir.join("agents");
    std::fs::create_dir_all(&agents_dir).expect("create agents dir");
    std::fs::write(
        agents_dir.join(format!("{name}.yaml")),
        format!(
            "name: {name}\nlabel: \"{name}\"\nmodel: stub-model\n\
             base_url: {base_url}\ntools: [READ, GLOB, GREP]\nreplaces: []\n"
        ),
    )
    .expect("write agent def");
}

fn a_sandboxed_session(session_id: &str) -> SessionMetadata {
    SessionMetadata {
        session_id: session_id.to_string(),
        project_id: "project-under-jailed-conversation".to_string(),
        created_at: "2026-09-09T10:00:00Z".to_string(),
        updated_at: "2026-09-09T10:00:00Z".to_string(),
        status: "active".to_string(),
        repo_path: Some("/tmp/worktrees/in-jail-conversation".to_string()),
        pid: None,
        tool: None,
        livekit_room: None,
        pending_elicitation: false,
        previous_session_id: None,
        session_type: Some("claude-cli".to_string()),
        model: None,
        cursor_chat_id: None,
        activity_status: None,
        hook_token: None,
        sandbox: Some(true),
        agent: None,
        recipe: None,
        agents: Vec::new(),
        agents_rev: 0,
        legacy_specialized_agents: Vec::new(),
        codebase_daemon_instance_id: None,
        codebase_session_id: None,
        agent_daemon_instance_id: None,
        agent_session_id: None,
    }
}

// ---------------------------------------------------------------------------
// The agent's model
// ---------------------------------------------------------------------------

/// An OpenAI-compatible chat-completions endpoint on the host, answering `answer` as soon as it is
/// asked.
///
/// On the host and not in the jail on purpose: the turn loop runs on the daemon, so what crosses
/// the jail boundary is the RPC and nothing else. A model the jail had to reach would be testing
/// the egress shim instead.
struct StubModel {
    base_url: String,
    _server: tokio::task::JoinHandle<()>,
}

async fn a_model_answering(answer: &str) -> StubModel {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind stub model server");
    let base_url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let answer = answer.to_string();

    let server = tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            let answer = answer.clone();
            tokio::spawn(async move {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut scratch = [0u8; 8192];
                let _ = socket.read(&mut scratch).await;
                let body = serde_json::json!({
                    "choices": [{ "message": { "role": "assistant", "content": answer },
                                  "finish_reason": "stop" }]
                })
                .to_string();
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\
                     Connection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = socket.write_all(response.as_bytes()).await;
                let _ = socket.flush().await;
            });
        }
    });

    StubModel {
        base_url,
        _server: server,
    }
}
