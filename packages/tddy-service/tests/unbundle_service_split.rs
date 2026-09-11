//! What the `#unbundle` stack's proto split must end up having done.
//!
//! Two kinds of assertion live here. The **served-coordinate** tests pass as soon as a node
//! publishes its proto, and pin the coordinate so a later tidy-up cannot rename it. The
//! **final-shape** test fails until the methods are actually gone from
//! `connection.ConnectionService`, which is what makes it the completion criterion for a node
//! rather than a description of one.
//!
//! Reading the `.proto` text rather than the generated Rust is deliberate: the generated trait is
//! what the daemon implements, but the `.proto` is what every other language's client is generated
//! from, and a method left declared there is a coordinate somebody can still call.

use std::path::{Path, PathBuf};

fn connection_proto() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("proto/connection.proto");
    std::fs::read_to_string(path).expect("connection.proto is readable")
}

fn service_block(proto: &str, service: &str) -> String {
    let start = proto
        .find(&format!("service {service} {{"))
        .unwrap_or_else(|| panic!("{service} is declared"));
    let end = proto[start..]
        .find("\n}")
        .unwrap_or_else(|| panic!("{service}'s block is closed"));
    proto[start..start + end].to_string()
}

fn read(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("proto")
        .join(name);
    std::fs::read_to_string(path).unwrap_or_else(|_| panic!("{name} is readable"))
}

const HOST_METHODS: [&str; 8] = [
    "ListEligibleDaemons",
    "ListKnownHosts",
    "GetHostTooling",
    "StreamHostPrompts",
    "AnswerHostPrompt",
    "AddHostKey",
    "ListHostKeyCandidates",
    "StreamHostStats",
];

const WORKTREE_METHODS: [&str; 9] = [
    "ListWorktreesForProject",
    "RemoveWorktree",
    "StreamWorktreeStats",
    "CalculateWorktreeSize",
    "CleanWorktree",
    "RestoreSessionWorktree",
    "ListWorktreeDirectory",
    "ReadWorktreeFile",
    "StreamReadWorktreeFile",
];

#[test]
fn host_service_declares_every_host_method() {
    // Given
    let block = service_block(&read("host.proto"), "HostService");

    // Then
    for method in HOST_METHODS {
        assert!(
            block.contains(&format!("rpc {method}(")),
            "host.HostService is missing {method}"
        );
    }
}

#[test]
fn worktree_service_declares_every_worktree_method() {
    // Given
    let block = service_block(&read("worktree.proto"), "WorktreeService");

    // Then
    for method in WORKTREE_METHODS {
        assert!(
            block.contains(&format!("rpc {method}(")),
            "worktree.WorktreeService is missing {method}"
        );
    }
}

/// The two new protos import nothing, because the closure of messages their 17 methods reach shares
/// **nothing** with any method that stayed in `connection.proto`. That was established by walking
/// field types, and this test is what stops a later node importing a shared types file into them out
/// of habit and re-coupling them.
#[test]
fn neither_new_proto_imports_connection() {
    for name in ["host.proto", "worktree.proto"] {
        let proto = read(name);
        assert!(
            !proto.contains("import \"connection.proto\""),
            "{name} must not import connection.proto"
        );
    }
}

/// **The completion criterion for `#unbundle` node 1.** Publishing the two protos is half the job;
/// the split is only real once the coordinates are gone from the service they left, because until
/// then both coordinates answer and a client can keep calling the old one.
#[test]
fn connection_service_no_longer_declares_the_moved_methods() {
    // Given
    let block = service_block(&connection_proto(), "ConnectionService");

    // When
    let still_there: Vec<&str> = HOST_METHODS
        .into_iter()
        .chain(WORKTREE_METHODS)
        .filter(|method| block.contains(&format!("rpc {method}(")))
        .collect();

    // Then
    assert!(
        still_there.is_empty(),
        "these moved to host.HostService / worktree.WorktreeService but are still declared on \
         connection.ConnectionService: {still_there:?}"
    );
}

/// The residual is the deliberate endpoint of the whole stack, not a leftover: families C (sessions
/// lifecycle), D (projects and branches), O (demo VM) and Q (`MintLocalToken`). Pinning the count
/// makes every later node state its arithmetic out loud instead of drifting.
#[test]
fn connection_service_keeps_exactly_the_methods_node_one_leaves_behind() {
    // Given
    let block = service_block(&connection_proto(), "ConnectionService");

    // When
    let declared = block.matches("  rpc ").count();

    // Then
    assert_eq!(
        declared, 33,
        "of the original 90: node 1 moved 17, node 4 moved StreamLiveKitRooms, node 6 moved 22, \
         node 7 moved 17 (family B's 9 to session_agents, families M and N's 8 to activity). \
         Every node updates this number — a node that lands and leaves it alone turns this test red \
         for the next one, who will read a passing assertion as a promise rather than as the \
         arithmetic its own change owes. Recount, do not compute: restating the proto's own count \
         back at it would pass for removing 21 or 23 just as happily"
    );
}

const LIVEKIT_METHODS: [&str; 1] = ["StreamLiveKitRooms"];

#[test]
fn livekit_service_declares_the_rooms_stream() {
    // Given
    let block = service_block(&read("livekit.proto"), "LiveKitService");

    // Then
    for method in LIVEKIT_METHODS {
        assert!(
            block.contains(&format!("rpc {method}(")),
            "livekit.LiveKitService is missing {method}"
        );
    }
}

/// Node 4 moves one method. It moves rather than staying because leaving it would keep the daemon
/// serving a handler for a subsystem that now lives in `tddy-daemon-livekit` — the shape this stack
/// exists to remove.
#[test]
fn connection_service_no_longer_declares_the_rooms_stream() {
    // Given
    let block = service_block(&connection_proto(), "ConnectionService");

    // Then
    for method in LIVEKIT_METHODS {
        assert!(
            !block.contains(&format!("rpc {method}(")),
            "{method} moved to livekit.LiveKitService but is still on connection.ConnectionService"
        );
    }
}

const SESSION_FILES_METHODS: [&str; 13] = [
    "ListSessionWorkflowFiles",
    "ReadSessionWorkflowFile",
    "StreamContextManifest",
    "StreamReadContextFile",
    "StreamReadContextFileBatch",
    "UploadSessionFileChunk",
    "ListSessionUploads",
    "DeleteSessionUpload",
    "UploadStagedAttachmentChunk",
    "ListStagedAttachments",
    "DeleteStagedAttachment",
    "ReadHostDocument",
    "StreamReadHostDocument",
];

const TERMINAL_METHODS: [&str; 9] = [
    "StreamSessionTerminalIO",
    "StreamTerminalOutput",
    "SendTerminalInput",
    "GetTerminalHistory",
    "StartTerminalSession",
    "StopTerminalSession",
    "ListTerminalSessions",
    "ClaimTerminalControl",
    "WatchTerminalControl",
];

#[test]
fn session_files_service_declares_every_file_method() {
    let block = service_block(&read("session_files.proto"), "SessionFilesService");
    for method in SESSION_FILES_METHODS {
        assert!(
            block.contains(&format!("rpc {method}(")),
            "session_files.SessionFilesService is missing {method}"
        );
    }
}

/// The shared types file is created by node 6, **not** node 1 — and for exactly one enum.
///
/// The plan had node 1 introduce it on the strength of a planning-time list of ~25 cross-family
/// shared messages. That list was wrong for node 1's families, node 4's, and node 6's terminal
/// family: all three cuts were fully self-contained. `HostDocumentScope` is the first type that
/// genuinely crosses, because `connection.ConnectionService`'s `StartSession` needs it too.
#[test]
fn the_shared_types_file_holds_only_what_two_served_services_both_need() {
    // Given the shared file and the two served services said to both need it
    let types = read("types.proto");
    let connection = read("connection.proto");
    let session_files = read("session_files.proto");
    let session_agents = read("session_agents.proto");

    // Then it holds the one enum that crosses
    assert!(
        types.contains("enum HostDocumentScope"),
        "types.proto exists for HostDocumentScope"
    );

    // Then both services really do reach it — otherwise "two served services both need it" is a
    // claim about one, and the enum belongs in that one's own proto
    assert!(
        connection.contains("import \"types.proto\""),
        "connection.ConnectionService reaches the shared scope through StartSession's \
         HostDocumentRef, so connection.proto must import types.proto"
    );
    assert!(
        session_files.contains("import \"types.proto\""),
        "session_files.SessionFilesService reaches the shared scope through ReadHostDocument, so \
         session_files.proto must import types.proto"
    );

    // Then it holds the two types node 7 added, and both of those cross too
    assert!(
        types.contains("message SessionAgentActivity") && types.contains("enum SessionAgentStatus"),
        "types.proto holds the two types ListSessions and the session-agent roster both reach"
    );
    assert!(
        connection.contains("types.SessionAgentStatus") && connection.contains("types.SessionAgentActivity"),
        "connection.ConnectionService keeps ListSessions, whose SessionEntry carries both, so \
         connection.proto must reach them rather than redeclaring them"
    );
    assert!(
        session_agents.contains("import \"types.proto\"")
            && session_agents.contains("types.SessionAgentStatus"),
        "session_agents.SessionAgentService owns the roster rows carrying both, so \
         session_agents.proto must reach the shared types"
    );

    // Then nothing else has been parked there
    let declared = types.matches("\nenum ").count() + types.matches("\nmessage ").count();
    assert_eq!(
        declared, 3,
        "types.proto must hold only what genuinely crosses; anything added needs two really-served \
         services reaching it. Node 7 added SessionAgentStatus and SessionAgentActivity, and both \
         clear that bar: connection.ConnectionService keeps ListSessions, whose SessionEntry \
         carries an agent_status and a last_activity, while session_agents.SessionAgentService \
         reaches both through the SessionAgentEntry rows of the roster it owns"
    );
}

#[test]
fn the_session_files_proto_reaches_the_shared_scope_rather_than_copying_it() {
    // Given
    let proto = read("session_files.proto");

    // Then
    assert!(
        proto.contains("import \"types.proto\""),
        "session_files.proto imports the shared scope"
    );
    assert!(
        !proto.contains("enum HostDocumentScope"),
        "session_files.proto must reach the shared enum, not redeclare it"
    );
}

/// **The terminal family already had a service, and it was served nowhere.**
///
/// `packages/tddy-terminal-rpc/proto/terminal_session.proto` declares 9 rpcs duplicating family K
/// exactly, and `grep -rn 'TerminalSessionService'` outside that package returns zero hits. What was
/// actually shared was the *bridge*, and its two call sites hand-converted between the two message
/// sets. Node 6 serves the coordinate and deletes both converters.
#[test]
fn the_terminal_service_that_already_existed_declares_every_terminal_method() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tddy-terminal-rpc/proto/terminal_session.proto");
    let proto = std::fs::read_to_string(path).expect("terminal_session.proto is readable");
    let block = service_block(&proto, "TerminalSessionService");

    for method in TERMINAL_METHODS {
        assert!(
            block.contains(&format!("rpc {method}(")),
            "terminal_session.TerminalSessionService is missing {method}"
        );
    }
}

#[test]
fn connection_service_no_longer_declares_the_session_file_or_terminal_methods() {
    let block = service_block(&connection_proto(), "ConnectionService");
    let still_there: Vec<&str> = SESSION_FILES_METHODS
        .into_iter()
        .chain(TERMINAL_METHODS)
        .filter(|method| block.contains(&format!("rpc {method}(")))
        .collect();

    assert!(
        still_there.is_empty(),
        "these moved to session_files.SessionFilesService / \
         terminal_session.TerminalSessionService but are still on connection.ConnectionService: \
         {still_there:?}"
    );
}

const CONVERTED_MESSAGES: [&str; 2] = [
    "connection::SessionTerminalInput",
    "connection::SessionTerminalOutput",
];

/// The crate publishing the message set the converters were replaced by, named by both swept trees.
///
/// It is the positive control: a sweep that finds no mention of it swept somewhere that is not the
/// terminal code, so its own absence finding means nothing.
const TERMINAL_RPC_CRATE: &str = "tddy_terminal_rpc";

/// Every hand-written converter goes. Keeping one would leave three message shapes for one stream —
/// `connection.*`, `terminal_session.*`, and the converter between them — inside the service whose
/// whole purpose is to be the single terminal surface.
///
/// **Both message names, across both crates that held a converter.** The narrow first version of
/// this test looked for `connection::SessionTerminalInput` under `tddy-daemon/src` alone, which a
/// deleted doc comment would have satisfied: it never saw `to_connection_output`, the daemon's
/// second converter, nor the four inline ones in `tddy-coder`'s participant.
///
/// Scoped to Rust sources (see [`rust_sources_under`]), which is also what keeps `sandbox.proto`'s
/// terminal frame out of it: that frame is the sandbox's own message now, and even while it was
/// `connection.SessionTerminalOutput` the reference was a `.proto` field type rather than a Rust
/// conversion — a needle a `.rs`-only walk cannot reach.
#[test]
fn no_source_converts_between_the_two_terminal_message_sets() {
    // Given
    let package = Path::new(env!("CARGO_MANIFEST_DIR"));
    let converter_free = ["../tddy-daemon/src", "../tddy-coder/src"]
        .map(|crate_src| rust_sources_under(&package.join(crate_src)));
    for sources in &converter_free {
        sources.assert_reaches_the_terminal_code();
    }

    // When
    let hits: Vec<String> = converter_free
        .iter()
        .flat_map(|sources| {
            CONVERTED_MESSAGES
                .iter()
                .flat_map(|needle| sources.naming(needle))
        })
        .collect();

    // Then
    assert!(
        hits.is_empty(),
        "these still name a connection.* terminal message, which only a converter between it and \
         terminal_session.* can be doing: {hits:?}"
    );
}

/// Every `.rs` file under one crate's `src`, read once, with its text.
///
/// `.rs` only, deliberately: the names this looks for are also legitimate `.proto` field types, and
/// a walk that read those would fail on a declaration rather than on a conversion.
///
/// Nothing about the reading is tolerated silently. A directory that is not there and a file that
/// cannot be read both panic, because this backs an *absence* assertion: a sweep that quietly saw
/// nothing reports zero converters exactly as loudly as a tree that has none, and one renamed crate
/// directory would turn the completion criterion for this node into a test that passes by looking
/// at nothing.
struct RustSources {
    root: PathBuf,
    files: Vec<(PathBuf, String)>,
}

fn rust_sources_under(dir: &Path) -> RustSources {
    let mut files = Vec::new();
    collect_rust_sources(dir, &mut files);
    RustSources {
        root: dir.to_path_buf(),
        files,
    }
}

fn collect_rust_sources(dir: &Path, into: &mut Vec<(PathBuf, String)>) {
    let entries = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{} is a readable directory: {e}", dir.display()));
    for entry in entries {
        let path = entry
            .unwrap_or_else(|e| panic!("{} lists its entries: {e}", dir.display()))
            .path();
        if path.is_dir() {
            collect_rust_sources(&path, into);
        } else if path.extension().is_some_and(|e| e == "rs") {
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("{} is a readable Rust source: {e}", path.display()));
            into.push((path, text));
        }
    }
}

impl RustSources {
    /// The swept files whose text contains `needle`.
    fn naming(&self, needle: &str) -> Vec<String> {
        self.files
            .iter()
            .filter(|(_, text)| text.contains(needle))
            .map(|(path, _)| path.display().to_string())
            .collect()
    }

    fn assert_reaches_the_terminal_code(&self) -> &Self {
        assert!(
            !self.naming(TERMINAL_RPC_CRATE).is_empty(),
            "swept {} .rs file(s) under {} and not one named {TERMINAL_RPC_CRATE}: this is not \
             the terminal code, so an absence found in it is an absence of the sweep rather than \
             of a converter",
            self.files.len(),
            self.root.display()
        );
        self
    }
}

const SESSION_AGENT_METHODS: [&str; 9] = [
    "AttachSessionAgent",
    "DetachSessionAgent",
    "ListSessionAgents",
    "StreamSessionAgents",
    "OpenAgentConversation",
    "PromptAgentConversation",
    "CancelAgentConversation",
    "ReportAgentCloneState",
    "ReportAgentConversationState",
];

const ACTIVITY_METHODS: [&str; 8] = [
    "ReportSessionStatus",
    "StreamSessionActivity",
    "ReportAgentActivity",
    "StreamSessionNotifications",
    "StreamAgentActivityDelta",
    "StreamAcpReplay",
    "GetAcpToolCallDetail",
    "GetAcpReplayPage",
];

#[test]
fn session_agent_service_declares_every_roster_and_conversation_method() {
    let block = service_block(&read("session_agents.proto"), "SessionAgentService");
    for method in SESSION_AGENT_METHODS {
        assert!(
            block.contains(&format!("rpc {method}(")),
            "session_agents.SessionAgentService is missing {method}"
        );
    }
}

#[test]
fn activity_service_declares_every_activity_and_replay_method() {
    let block = service_block(&read("activity.proto"), "ActivityService");
    for method in ACTIVITY_METHODS {
        assert!(
            block.contains(&format!("rpc {method}(")),
            "activity.ActivityService is missing {method}"
        );
    }
}

#[test]
fn connection_service_no_longer_declares_the_agent_or_activity_methods() {
    let block = service_block(&connection_proto(), "ConnectionService");
    let still_there: Vec<&str> = SESSION_AGENT_METHODS
        .into_iter()
        .chain(ACTIVITY_METHODS)
        .filter(|method| block.contains(&format!("rpc {method}(")))
        .collect();

    assert!(
        still_there.is_empty(),
        "these moved to session_agents / activity but are still on connection.ConnectionService: \
         {still_there:?}"
    );
}

/// ⛔ The security-relevant edit. `packages/tddy-sandbox-runner/src/runner.rs` holds the
/// `(service, method)` allowlist of what an in-jail agent may relay to its host, and five family-B
/// methods are in it. Move the coordinate without the allowlist and every in-jail conversation fails
/// **closed** — silently, at runtime.
///
/// The permitted operation *set* must not change; only the service name each tuple carries.
#[test]
fn the_sandbox_relay_allowlist_names_the_new_service() {
    let runner = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../tddy-sandbox-runner/src/runner.rs"),
    )
    .expect("the sandbox runner's source is readable");

    assert!(
        !runner.contains("\"connection.ConnectionService\", \"StreamSessionAgents\"")
            && !runner.contains("\"StreamSessionAgents\""),
        "the relay allowlist still gates family B under connection.ConnectionService"
    );
    assert!(
        runner.contains("session_agents.SessionAgentService")
            || runner.contains("IN_JAIL_RELAYABLE"),
        "the relay allowlist must name the new service, or read it from tddy-session-agents"
    );
}
