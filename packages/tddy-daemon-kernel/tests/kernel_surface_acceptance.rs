//! The kernel's surface as a *consumer* sees it.
//!
//! An integration test is compiled as its own crate, so everything below is reached exactly the way
//! `tddy-host-service` and `tddy-worktree-service` will reach it: through `pub` items only, from a
//! crate that has never heard of `tddy-daemon`. That is the whole point of the file. The inline
//! `#[cfg(test)] mod tests` in `src/lib.rs` can see private items and would keep passing if the
//! surface were too narrow to build anything with — these cannot.
//!
//! So what belongs here is what only a foreign crate can say: that a symbol is reachable, and that
//! the pieces a consumer assembles out of several of them fit together. Semantics each symbol owns
//! alone are asserted once, inline, where the implementation is — restating them here in weaker
//! form would buy a second failure for one defect and no coverage for any other.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_core::agent_activity::{AgentActivityRecord, STATUS_RUNNING};
use tddy_daemon_kernel::user_paths::projects_path_for_user;
use tddy_daemon_kernel::{
    now_unix_ms, trim_to_option, AgentActivityHub, SessionUserResolver, SessionsBaseResolver,
    HOST_DOCUMENT_FRAME_BYTES,
};

/// Reachability for the three symbols no consumer here has to *call* to depend on.
///
/// Naming them in a type-checked binding is the whole claim this file makes about them: a
/// consumer's crate can see them, with these signatures. Their behaviour is pinned inline in
/// `src/lib.rs`, against the implementation — a stamp assertion or a `trim_to_option("  main  ")`
/// repeated out here would fail for the same single defect and catch nothing the inline suite
/// misses.
const _: fn() -> u64 = now_unix_ms;
const _: fn(&str) -> Option<String> = trim_to_option;
const _: usize = HOST_DOCUMENT_FRAME_BYTES;

/// The extraction is only real if these symbols are not re-exports. A kernel that reached back into
/// `tddy-daemon` would compile just as happily and prove nothing — every subsystem crate importing
/// from it would still be dragging the 23,099-line RPC entry point along transitively — so the
/// absence of that edge is asserted rather than assumed.
#[test]
fn declares_no_dependency_on_the_daemon_it_was_extracted_from() {
    // Given
    let manifest =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"))
            .expect("the kernel's own manifest is readable");

    // When
    let dependencies = dependency_names(&manifest);

    // Then
    assert!(
        !dependencies.iter().any(|name| name == "tddy-daemon"),
        "the kernel declares {dependencies:?}; a `tddy-daemon` edge would make every symbol here a \
         re-export and leave the subsystems it exists to free still bound to the daemon"
    );
}

/// A manifest can declare a dependency in shapes the plain `name = …` row does not cover, and the
/// edge this crate must not have is exactly the one someone would add in a hurry. A scan that
/// missed either shape would report a clean dependency list for a kernel that had grown the edge.
#[test]
fn reads_a_dependency_declared_as_a_sub_table_or_a_workspace_inherit() {
    // Given every shape cargo accepts for the forbidden edge
    let manifest = "\
[package]\n\
name = \"tddy-daemon-kernel\"\n\
\n\
[dependencies]\n\
anyhow = \"1\"\n\
tddy-core = { path = \"../tddy-core\" }\n\
tddy-rpc.workspace = true\n\
\n\
[dependencies.tddy-daemon]\n\
path = \"../tddy-daemon\"\n\
\n\
[target.'cfg(unix)'.dependencies]\n\
libc = \"0.2\"\n";

    // When
    let dependencies = dependency_names(manifest);

    // Then — the sub-table header names a crate, and an inherited row is its name before the dot
    assert!(
        dependencies.iter().any(|name| name == "tddy-daemon"),
        "a `[dependencies.tddy-daemon]` sub-table declares the edge; the scan reported \
         {dependencies:?}"
    );
    assert!(
        dependencies.iter().any(|name| name == "tddy-rpc"),
        "`tddy-rpc.workspace = true` declares `tddy-rpc`; the scan reported {dependencies:?}"
    );
    assert!(
        dependencies.iter().any(|name| name == "libc"),
        "a `[target.'…'.dependencies]` table is still a dependency table; the scan reported \
         {dependencies:?}"
    );
    assert!(
        !dependencies.iter().any(|name| name == "tddy-daemon-kernel"),
        "`[package] name` is not a dependency; the scan reported {dependencies:?}"
    );
}

/// The chain every service runs on an authenticated call, assembled the way a consumer assembles
/// it: the wiring layer supplies both resolvers as closures over its own config, and the kernel
/// turns what they answer into the path on disk. Both aliases have to be nameable *and* the path
/// they feed has to be the kernel's, or a service cannot be built outside the daemon.
#[test]
fn locates_an_authenticated_callers_projects_directory_through_resolvers_it_built_itself() {
    // Given — the two shapes `auth::build_auth_entries` hands every service
    let resolve_user: SessionUserResolver =
        Arc::new(|token| (token == "a-live-token").then(|| "octocat".to_string()));
    let resolve_sessions_base: SessionsBaseResolver =
        Arc::new(|token| (token == "a-live-token").then(|| PathBuf::from("/srv/tddy")));

    // When
    let os_user = (resolve_user)("a-live-token").expect("a live token names its owner");
    let data_root = (resolve_sessions_base)("a-live-token").expect("a live token names its root");
    let projects = projects_path_for_user(&os_user, Some(&data_root));

    // Then — the subdirectory is the kernel's to decide, not the consumer's
    assert_eq!(projects, Some(PathBuf::from("/srv/tddy/projects")));
    assert_eq!(
        (resolve_user)("an-expired-token"),
        None,
        "an unknown token must name no owner, or every path below it is resolved for nobody"
    );
}

/// The sandbox subsystem holds an `Arc` of the hub and publishes into it from spawned work while the
/// activity handlers subscribe elsewhere. Both halves have to be reachable from outside the daemon,
/// or the subsystem cannot leave it — and the `Arc` is the part the inline suite, which publishes
/// through the one hub it owns, does not exercise.
#[test]
fn delivers_a_record_published_through_one_arc_to_a_subscriber_holding_another() {
    // Given
    let hub = Arc::new(AgentActivityHub::default());
    let mut activity = hub.subscribe("session-a");

    // When — a second handle publishes, as a spawned publisher would
    Arc::clone(&hub).publish("session-a", a_running_record("Read"));

    // Then
    let seen = activity
        .try_recv()
        .expect("the subscriber sees what the other handle published");
    assert_eq!(seen.tool_name, "Read");
}

/// Every crate named in the manifest's dependency tables, in any shape cargo accepts.
///
/// Hand-rolled rather than pulling in a TOML parser: adding a dependency to check a crate's
/// dependency list would be its own small joke. The shapes it has to understand are `name = …` and
/// `name.workspace = true` under a `[…dependencies]` header, and the `[….dependencies.name]`
/// sub-table — so a candidate is read up to its first `.`, and a header is in scope from the
/// segment that ends with `dependencies` onwards.
fn dependency_names(manifest: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut in_dependencies = false;
    for line in manifest.lines().map(str::trim) {
        if line.starts_with('[') {
            let mut segments = line.trim_matches(['[', ']']).split('.').map(str::trim);
            // `any` stops on the matching segment, so whatever follows it — present only in the
            // `[dependencies.tddy-daemon]` shape — is the crate the header itself declares.
            in_dependencies = segments.any(|segment| segment.ends_with("dependencies"));
            if in_dependencies {
                names.extend(segments.next().map(str::to_string));
            }
            continue;
        }
        if in_dependencies {
            if let Some((key, _)) = line.split_once('=') {
                names.extend(first_segment(key).map(str::to_string));
            }
        }
    }
    names
}

/// A dependency key up to its first `.`: `tddy-daemon.workspace` is a declaration of `tddy-daemon`,
/// and `path`/`version`/`features` rows inside a sub-table have no dot and are filtered by the
/// sub-table's own header instead.
fn first_segment(key: &str) -> Option<&str> {
    key.trim().split('.').next().map(str::trim)
}

/// A record with only the tool name set to something meaningful. Every other field is at its zero
/// value, which is what `AgentActivityRecord` means by "not yet known".
fn a_running_record(tool: &str) -> AgentActivityRecord {
    AgentActivityRecord {
        call_id: format!("call-for-{tool}"),
        tool_name: tool.to_string(),
        input: serde_json::Value::Null,
        status: STATUS_RUNNING.to_string(),
        result: serde_json::Value::Null,
        error_message: String::new(),
        started_unix_ms: 0,
        completed_unix_ms: 0,
        source: "sandbox".to_string(),
        head_commit: String::new(),
        activity_seq: 0,
        changed_paths: Vec::new(),
    }
}
