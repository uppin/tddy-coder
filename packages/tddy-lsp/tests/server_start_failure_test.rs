//! A language server that never came up is refused with its cause.
//!
//! `get_or_spawn` used to answer every failure to bring a server up — a program that is not there,
//! a server that exits before the handshake, a handshake the server refuses — with one bare
//! `lsp server exited`, and the cause its task had written down was thrown away. A cold
//! `tddy-tools restructure anchors` outside the dev shell printed exactly that, which reads like a
//! crash rather than "rust-analyzer is not on PATH". Driven against the deterministic `fake_lsp`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::json;
use tddy_lsp::{Language, LaunchSpec, LspAllowList, LspError, LspKey, LspRegistry};
use tddy_task::TaskRegistry;

/// How long a test waits for a server it stopped to be gone.
const LONG_ENOUGH_FOR_AN_EXIT: Duration = Duration::from_secs(5);

/// A registry whose only Rust server is `program` launched with `args`, and the task registry it
/// runs servers on — kept so a test can see when a server's task has ended.
fn a_registry_launching(program: &str, args: &[&str]) -> (LspRegistry, TaskRegistry) {
    let mut spec = LaunchSpec::new(program);
    spec.args = args.iter().map(|arg| arg.to_string()).collect();
    let mut allow = LspAllowList::new();
    allow.allow(Language::Rust, spec);
    let tasks = TaskRegistry::new();
    (
        LspRegistry::new(allow, tasks.clone(), Duration::from_secs(60)),
        tasks,
    )
}

fn the_fake() -> &'static str {
    env!("CARGO_BIN_EXE_fake_lsp")
}

/// A path under the temporary directory that names no file.
fn a_program_that_does_not_exist() -> PathBuf {
    std::env::temp_dir().join(format!("no-such-rust-analyzer-{}", std::process::id()))
}

fn the_key_of(root: &Path) -> LspKey {
    LspKey {
        root: root.to_path_buf(),
        language: Language::Rust,
    }
}

async fn a_server_requested_from(registry: &LspRegistry) -> Result<(), LspError> {
    registry
        .get_or_spawn(the_key_of(&std::env::temp_dir()))
        .await
        .map(|_| ())
}

/// Wait until every task the registry started has ended.
async fn once_every_server_has_ended(tasks: &TaskRegistry) {
    tokio::time::timeout(LONG_ENOUGH_FOR_AN_EXIT, async {
        while !tasks
            .list()
            .await
            .iter()
            .all(|task| task.status().is_terminal())
        {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the server's task ended");
}

#[tokio::test]
async fn a_server_program_that_does_not_exist_is_refused_as_not_found_naming_the_program_and_the_os_error(
) {
    // Given a registry configured with a program that is not there
    let program = a_program_that_does_not_exist();
    let (registry, _tasks) = a_registry_launching(&program.to_string_lossy(), &[]);

    // When a server is requested
    let refusal = a_server_requested_from(&registry).await;

    // Then it is refused as not found, naming the program and what the operating system said
    let Err(LspError::ServerNotFound(said)) = refusal else {
        panic!("expected ServerNotFound, got {refusal:?}");
    };
    assert!(
        said.starts_with(&format!("{}: ", program.display()))
            && said.contains("No such file or directory"),
        "{said}"
    );
}

#[tokio::test]
async fn a_server_that_exits_before_the_handshake_is_refused_as_not_started_with_its_exit_status() {
    // Given a server that exits as soon as it starts
    let (registry, _tasks) = a_registry_launching(the_fake(), &["--exit-immediately"]);

    // When a server is requested
    let refusal = a_server_requested_from(&registry).await;

    // Then it is refused as never started, with how it ended
    assert_eq!(
        refusal.map_err(|error| format!("{error:?}")),
        Err(format!(
            "{:?}",
            LspError::ServerNotStarted {
                program: the_fake().to_string(),
                reason: "exited before the initialize handshake completed (exit status: 0)"
                    .to_string(),
            }
        ))
    );
}

#[tokio::test]
async fn a_server_that_refuses_the_handshake_is_refused_as_not_started_naming_the_servers_error() {
    // Given a server that answers `initialize` with an error
    let (registry, _tasks) = a_registry_launching(the_fake(), &["--refuses-initialize"]);

    // When a server is requested
    let refusal = a_server_requested_from(&registry).await;

    // Then it is refused as never started, in the server's own words
    let Err(LspError::ServerNotStarted { program, reason }) = refusal else {
        panic!("expected ServerNotStarted, got {refusal:?}");
    };
    assert_eq!(program, the_fake());
    assert!(
        reason.starts_with("the initialize handshake failed: ") && reason.contains("refused"),
        "{reason}"
    );
}

#[tokio::test]
async fn a_server_that_did_not_start_reads_as_its_program_and_its_reason_in_one_line() {
    // Given a server that exits as soon as it starts
    let (registry, _tasks) = a_registry_launching(the_fake(), &["--exit-immediately"]);

    // When a server is requested and the refusal is read
    let read = a_server_requested_from(&registry)
        .await
        .map_err(|error| error.to_string());

    // Then it names the program and why it never came up
    assert_eq!(
        read,
        Err(format!(
            "language server `{}` did not start: exited before the initialize handshake \
             completed (exit status: 0)",
            the_fake()
        ))
    );
}

#[tokio::test]
async fn a_request_to_a_server_that_exited_after_it_came_up_is_still_refused_as_exited() {
    // Given a server that came up, and was then told to exit
    let (registry, tasks) = a_registry_launching(the_fake(), &[]);
    let service = registry
        .get_or_spawn(the_key_of(&std::env::temp_dir()))
        .await
        .expect("the fake server starts");
    service
        .client
        .notify_raw("exit", json!(null))
        .await
        .expect("the exit notification is sent");
    once_every_server_has_ended(&tasks).await;

    // When the old client asks it something
    let refusal = service
        .client
        .request_raw("textDocument/documentSymbol", json!({}))
        .await;

    // Then that is still the server that went away, not one that never started
    assert!(
        matches!(refusal, Err(LspError::ServerExited)),
        "{refusal:?}"
    );
}
