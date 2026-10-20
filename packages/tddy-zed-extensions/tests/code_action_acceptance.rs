//! Acceptance: `apply_code_action` runs a code action an extension language's server offers, and
//! a run over an extension language's file is verified (`#reshape` extension-code-actions, PRD
//! acceptance criteria; changeset R1–R5).
//!
//! The fixture is `demo-toml` with `fake-ls` declared for its language and configured to run
//! `tddy-lsp`'s deterministic `fake_lsp` with `--code-actions <file>` — the actions it offers and
//! the edits its commands send back — and `--diagnoses BROKEN`, so every `BROKEN` in a document is
//! an error diagnostic. Never a real language server.

mod outline_support;
mod server_support;

use std::fs;
use std::path::Path;
use std::sync::Arc;

use outline_support::{
    a_workspace_holding_the_app_config, an_extensions_dir_with, AFixtureExtension, AWorkspace, APP,
    APP_TOML,
};
use pretty_assertions::assert_eq;
use serde_json::{json, Value};
use server_support::{resolved_and_applied, the_fake_server, the_server_of};
use tddy_code_restructuring::backends::code_action::{CodeActionRefusal, OfferedAction};
use tddy_code_restructuring::verify::{ParseNode, SourceChange, SourceFinding, SourceVerify};
use tddy_code_restructuring::{Position, RefactorOp, Resolution, RestructureError};
use tddy_lsp::LspService;
use tddy_zed_extensions::ExtensionVerify;

/// The text every diagnosed document is checked for.
const MARKER: &str = "BROKEN";

/// The server, configured from the fixture's settings, offering what `code_actions` says.
struct ARunningServer {
    workspace: AWorkspace,
    installed: outline_support::AnExtensionsDir,
    server: Arc<LspService>,
}

/// `config/app.toml`'s URI as the backend reports it to the server, under the workspace root.
fn the_app_uri(root: &Path) -> String {
    format!("file://{}/{APP}", root.display())
}

/// An LSP edit replacing zero-based `line`'s `from..to` of the app config with `text`.
fn an_edit_of_the_app(root: &Path, line: u32, from: u32, to: u32, text: &str) -> Value {
    json!({ "changes": { the_app_uri(root): [{
        "range": { "start": { "line": line, "character": from }, "end": { "line": line, "character": to } },
        "newText": text,
    }] } })
}

/// `port` → `listen_port` on line 3 of [`APP_TOML`] (zero-based line 2).
fn the_port_rename(root: &Path) -> Value {
    an_edit_of_the_app(root, 2, 0, 4, "listen_port")
}

/// demo-toml served by the fake server, offering the actions `code_actions(root)` builds.
async fn a_server_offering(code_actions: impl FnOnce(&Path) -> Value) -> ARunningServer {
    let installed = an_extensions_dir_with(&[AFixtureExtension::DemoToml]);
    installed.declare_fake_ls_for_demo_toml();
    let workspace = a_workspace_holding_the_app_config();
    let file = installed.data_dir().join("code-actions.json");
    fs::write(&file, code_actions(workspace.root()).to_string()).expect("write the code actions");
    installed.write_settings(json!({ "lsp": { "fake-ls": { "binary": {
        "path": the_fake_server(),
        "arguments": ["--code-actions", file.display().to_string(), "--diagnoses", MARKER],
    } } } }));
    let server = the_server_of(&installed.the_demo_toml_support(), workspace.root()).await;
    ARunningServer {
        workspace,
        installed,
        server,
    }
}

/// `apply_code_action` over line 3 (`port = 8080`), choosing by `action`.
fn an_apply_code_action(action: Value) -> RefactorOp {
    serde_json::from_value(json!({
        "op": "apply_code_action",
        "anchor": { "kind": "range", "file": APP, "start": { "line": 3, "col": 1 }, "end": { "line": 3, "col": 12 } },
        "action": action,
    }))
    .expect("the op parses")
}

impl ARunningServer {
    /// `op` resolved through demo-toml's backend over this server, and applied.
    async fn applying(&self, op: RefactorOp) -> Result<Resolution, RestructureError> {
        resolved_and_applied(
            self.installed.the_demo_toml_support(),
            Some(self.server.clone()),
            self.workspace.root(),
            op,
        )
        .await
    }

    fn the_app_config(&self) -> String {
        fs::read_to_string(self.workspace.root().join(APP)).expect("read the app config")
    }

    /// What demo-toml's verify finds in the app config's change from [`APP_TOML`] to what is on
    /// disk now.
    async fn verifying_the_app_config(&self) -> Vec<SourceFinding> {
        let verify = ExtensionVerify::new(
            self.installed.the_demo_toml_support(),
            Some(self.server.client.clone()),
        );
        let root = self.workspace.root().to_path_buf();
        let change = SourceChange {
            path: APP.to_string(),
            before: APP_TOML.to_string(),
            after: self.the_app_config(),
        };
        tokio::task::spawn_blocking(move || verify.verify(&root, &change))
            .await
            .expect("the blocking half of the verify joins")
            .expect("the change is verified")
    }
}

fn offered(title: &str, kind: &str) -> OfferedAction {
    OfferedAction {
        title: title.to_string(),
        kind: Some(kind.to_string()),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn an_action_carrying_an_edit_is_applied_by_title() {
    // Given a server offering two actions, one of which carries the rename of `port`
    let running = a_server_offering(|root| {
        json!({ "actions": [
            { "title": "Sort keys", "kind": "source.sort", "edit": an_edit_of_the_app(root, 1, 0, 0, "") },
            { "title": "Rename to listen_port", "kind": "refactor.rewrite", "edit": the_port_rename(root) },
        ] })
    })
    .await;

    // When the second is applied by its title
    let applied = running
        .applying(an_apply_code_action(
            json!({ "title": "Rename to listen_port" }),
        ))
        .await;

    // Then its edit, and only its, is on disk (R1, R2)
    assert!(applied.is_ok(), "the action was refused: {applied:?}");
    assert_eq!(
        running.the_app_config(),
        APP_TOML.replace("port = 8080", "listen_port = 8080")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_command_action_is_applied_from_the_servers_apply_edit() {
    // Given a server whose action is a command that sends the rename back through applyEdit
    let running = a_server_offering(|root| {
        json!({
            "actions": [{ "title": "Rename to listen_port", "kind": "refactor.rewrite",
                          "command": { "title": "Rename", "command": "fake.rename-port", "arguments": [] } }],
            "command_edits": { "fake.rename-port": [the_port_rename(root)] },
        })
    })
    .await;

    // When it is applied by its kind
    let applied = running
        .applying(an_apply_code_action(json!({ "kind": "refactor.rewrite" })))
        .await;
    let answers = running
        .server
        .client
        .request_raw("tddy/applyEditAnswers", Value::Null)
        .await
        .expect("the fake server replays what the client answered");

    // Then the edit the server sent is on disk, written by the run, and the server was told it was
    // applied (R2, R3)
    assert!(applied.is_ok(), "the action was refused: {applied:?}");
    assert_eq!(
        (running.the_app_config(), answers),
        (
            APP_TOML.replace("port = 8080", "listen_port = 8080"),
            json!([{ "applied": true }])
        )
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn two_kind_matches_are_refused_with_both_titles() {
    // Given two actions under `refactor.extract`
    let running = a_server_offering(|root| {
        json!({ "actions": [
            { "title": "Extract table", "kind": "refactor.extract.table", "edit": the_port_rename(root) },
            { "title": "Extract key", "kind": "refactor.extract.key", "edit": the_port_rename(root) },
        ] })
    })
    .await;

    // When an action is applied by that kind
    let refused = running
        .applying(an_apply_code_action(json!({ "kind": "refactor.extract" })))
        .await;

    // Then it is refused listing both, and nothing is written (R1)
    match refused {
        Err(RestructureError::CodeActionRefused(CodeActionRefusal::Ambiguous {
            matches, ..
        })) => {
            assert_eq!(
                (matches, running.the_app_config()),
                (
                    vec![
                        offered("Extract table", "refactor.extract.table"),
                        offered("Extract key", "refactor.extract.key")
                    ],
                    APP_TOML.to_string()
                )
            )
        }
        other => panic!("expected an ambiguous selection, got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn no_match_lists_what_the_server_offered() {
    // Given a server offering one action
    let running = a_server_offering(|root| {
        json!({ "actions": [
            { "title": "Sort keys", "kind": "source.sort", "edit": the_port_rename(root) },
        ] })
    })
    .await;

    // When an action it does not offer is applied
    let refused = running
        .applying(an_apply_code_action(json!({ "title": "Extract function" })))
        .await;

    // Then the refusal lists what it offered (R1)
    match refused {
        Err(RestructureError::CodeActionRefused(CodeActionRefusal::NotOffered {
            offered: listed,
            ..
        })) => assert_eq!(listed, vec![offered("Sort keys", "source.sort")]),
        other => panic!("expected the selection to be refused, got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_command_that_sends_no_edit_is_refused() {
    // Given an action whose command sends nothing back
    let running = a_server_offering(|_| {
        json!({
            "actions": [{ "title": "Do nothing", "kind": "refactor.rewrite",
                          "command": { "title": "Nothing", "command": "fake.nothing", "arguments": [] } }],
            "command_edits": { "fake.other": [] },
        })
    })
    .await;

    // When it is applied
    let refused = running
        .applying(an_apply_code_action(json!({ "title": "Do nothing" })))
        .await;

    // Then it is refused naming the action, and nothing is written (R2)
    match refused {
        Err(RestructureError::CodeActionRefused(CodeActionRefusal::NoEdit { title })) => {
            assert_eq!(
                (title, running.the_app_config()),
                ("Do nothing".to_string(), APP_TOML.to_string())
            )
        }
        other => panic!("expected the edit-less action to be refused, got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn verify_fails_on_a_new_error_diagnostic() {
    // Given an action that writes a value the server reports as an error, though it parses
    let running = a_server_offering(|root| {
        json!({ "actions": [
            { "title": "Quote port", "kind": "refactor.rewrite", "edit": an_edit_of_the_app(root, 2, 7, 11, "\"BROKEN\"") },
        ] })
    })
    .await;
    running
        .applying(an_apply_code_action(json!({ "title": "Quote port" })))
        .await
        .expect("the action applies");

    // When the run is verified
    let findings = running.verifying_the_app_config().await;

    // Then the new error is the one finding, where the server reported it (R5)
    assert_eq!(
        findings,
        vec![SourceFinding::NewErrorDiagnostic {
            path: APP.to_string(),
            at: Position { line: 3, col: 9 },
            message: "unexpected BROKEN".to_string(),
        }]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn verify_fails_on_a_parse_error_node() {
    // Given an action that leaves `port = = 8080`, which TOML cannot parse
    let running = a_server_offering(|root| {
        json!({ "actions": [
            { "title": "Break port", "kind": "refactor.rewrite", "edit": an_edit_of_the_app(root, 2, 7, 7, "= ") },
        ] })
    })
    .await;
    running
        .applying(an_apply_code_action(json!({ "title": "Break port" })))
        .await
        .expect("the action applies");

    // When the run is verified
    let findings = running.verifying_the_app_config().await;

    // Then a parse error on line 3 is found (R5)
    assert!(
        findings.iter().any(|finding| matches!(
            finding,
            SourceFinding::ParseError { path, node: ParseNode::Error | ParseNode::Missing, at }
                if path == APP && at.line == 3
        )),
        "no parse error on line 3 was found: {findings:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn verify_holds_for_a_clean_action() {
    // Given an action that renames a key and leaves valid, error-free TOML
    let running = a_server_offering(|root| {
        json!({ "actions": [
            { "title": "Rename to listen_port", "kind": "refactor.rewrite", "edit": the_port_rename(root) },
        ] })
    })
    .await;
    running
        .applying(an_apply_code_action(
            json!({ "title": "Rename to listen_port" }),
        ))
        .await
        .expect("the action applies");

    // When the run is verified
    let findings = running.verifying_the_app_config().await;

    // Then it holds (R5)
    assert_eq!(findings, Vec::new());
}
