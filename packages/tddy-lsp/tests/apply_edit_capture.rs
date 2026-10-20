//! A `workspace/applyEdit` the server sends while a `workspace/executeCommand` runs reaches the
//! command's caller, answered `{ "applied": true }` and written nowhere (`#reshape`
//! extension-code-actions, R2–R3), against the deterministic `fake_lsp` started with
//! `--code-actions`, whose `command_edits` say which edits a command sends.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};
use tddy_lsp::{LanguageId, LaunchSpec, LspAllowList, LspKey, LspRegistry, LspService};
use tddy_task::TaskRegistry;

/// The command the fake server implements by asking the client to apply two edits.
const TWO_EDIT_COMMAND: &str = "fake.extract";

/// An edit inserting `text` at the start of `file:///workspace/app.toml`.
fn an_insertion_of(text: &str) -> Value {
    json!({ "changes": { "file:///workspace/app.toml": [{
        "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } },
        "newText": text,
    }] } })
}

/// A `--code-actions` file in `dir` whose [`TWO_EDIT_COMMAND`] sends two edits.
fn a_code_actions_file_in(dir: &Path) -> PathBuf {
    let path = dir.join("code-actions.json");
    std::fs::write(
        &path,
        json!({
            "command_edits": { TWO_EDIT_COMMAND: [an_insertion_of("a = 1\n"), an_insertion_of("b = 2\n")] }
        })
        .to_string(),
    )
    .expect("write the code-actions file");
    path
}

async fn a_server_answering_from(code_actions: &Path) -> Arc<LspService> {
    let language = LanguageId::new("demo-toml");
    let mut spec = LaunchSpec::new(env!("CARGO_BIN_EXE_fake_lsp"));
    spec.args = vec![
        "--code-actions".to_string(),
        code_actions.display().to_string(),
    ];
    let mut allow = LspAllowList::new();
    allow.allow(language.clone(), spec);
    LspRegistry::new(allow, TaskRegistry::new(), Duration::from_secs(60))
        .get_or_spawn(LspKey {
            root: PathBuf::from("/workspace"),
            language,
        })
        .await
        .expect("the fake server starts")
}

#[tokio::test]
async fn an_apply_edit_during_execute_command_reaches_the_caller() {
    // Given a server whose command applies two edits through the client
    let dir = tempfile_dir();
    let service = a_server_answering_from(&a_code_actions_file_in(dir.path())).await;

    // When the command is executed
    let executed = service
        .client
        .execute_command(TWO_EDIT_COMMAND, Vec::new())
        .await
        .expect("the command is answered");
    let answers = service
        .client
        .request_raw("tddy/applyEditAnswers", Value::Null)
        .await
        .expect("the fake server replays the client's answers");

    // Then both edits reach the caller in arrival order, and the server was told each was applied
    assert_eq!(
        (executed.result, executed.applied_edits, answers),
        (
            Value::Null,
            vec![an_insertion_of("a = 1\n"), an_insertion_of("b = 2\n")],
            json!([{ "applied": true }, { "applied": true }])
        )
    );
}

#[tokio::test]
async fn a_command_that_sends_no_edit_reaches_the_caller_with_none() {
    // Given a server that implements a command sending nothing
    let dir = tempfile_dir();
    let service = a_server_answering_from(&a_code_actions_file_in(dir.path())).await;

    // When a command it has no edits for is executed
    let executed = service
        .client
        .execute_command("fake.nothing", vec![json!(1)])
        .await
        .expect("the command is answered");

    // Then the caller has its result and no edit
    assert_eq!(
        (executed.result, executed.applied_edits),
        (Value::Null, Vec::<Value>::new())
    );
}

/// A temporary directory for the code-actions file; deleted on drop.
fn tempfile_dir() -> TempDir {
    TempDir::new()
}

/// A minimal self-deleting temporary directory, so this suite adds no dev-dependency.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "tddy-lsp-apply-edit-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&path).expect("create the temporary directory");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
