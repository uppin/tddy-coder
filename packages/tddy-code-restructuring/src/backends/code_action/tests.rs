//! R1–R2 against a server double: which action is chosen, how it becomes an edit, and every
//! refusal — no process, no document.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::Path;

use pretty_assertions::assert_eq;
use serde_json::{json, Value};

use super::*;
use crate::edit::{FileEdit, Position, TextEdit, WorkspaceEdit};
use crate::overlay::Overlay;
use crate::RestructureError;

const FILE: &str = "config/app.toml";
const URI: &str = "file:///workspace/config/app.toml";

/// A server that offers `actions`, resolves the titles in `resolved` to their edits, and answers
/// each command in `command_edits` by sending those edits back.
#[derive(Default)]
struct AServer {
    actions: Vec<Value>,
    resolved: BTreeMap<String, Value>,
    command_edits: BTreeMap<String, Vec<Value>>,
    asked_for: RefCell<Vec<(String, Range, Vec<String>)>>,
}

impl AServer {
    fn offering(actions: Vec<Value>) -> Self {
        Self {
            actions,
            ..Self::default()
        }
    }

    fn resolving(mut self, title: &str, edit: Value) -> Self {
        self.resolved.insert(title.to_string(), edit);
        self
    }

    fn running(mut self, command: &str, edits: Vec<Value>) -> Self {
        self.command_edits.insert(command.to_string(), edits);
        self
    }
}

impl CodeActionServer for AServer {
    fn code_actions(&self, uri: &str, range: Range, only: &[String]) -> Result<Vec<Value>> {
        self.asked_for
            .borrow_mut()
            .push((uri.to_string(), range, only.to_vec()));
        Ok(self.actions.clone())
    }

    fn resolves_code_actions(&self) -> bool {
        !self.resolved.is_empty()
    }

    fn resolve_code_action(&self, action: &Value) -> Result<Value> {
        let mut action = action.clone();
        if let Some(edit) = action["title"]
            .as_str()
            .and_then(|title| self.resolved.get(title))
        {
            action["edit"] = edit.clone();
        }
        Ok(action)
    }

    fn execute_command(&self, command: &Value) -> Result<Vec<Value>> {
        Ok(command["command"]
            .as_str()
            .and_then(|name| self.command_edits.get(name))
            .cloned()
            .unwrap_or_default())
    }
}

/// An LSP edit replacing zero-based `line`'s `from..to` of the file with `text`.
fn an_lsp_edit(line: u32, from: u32, to: u32, text: &str) -> Value {
    json!({ "changes": { URI: [{
        "range": { "start": { "line": line, "character": from }, "end": { "line": line, "character": to } },
        "newText": text,
    }] } })
}

/// The same edit as this crate's one-based change of [`FILE`].
fn a_change(line: u32, from: u32, to: u32, text: &str) -> FileEdit {
    FileEdit::Change {
        path: FILE.to_string(),
        edits: vec![TextEdit {
            range: Range {
                start: Position {
                    line: line + 1,
                    col: from + 1,
                },
                end: Position {
                    line: line + 1,
                    col: to + 1,
                },
            },
            new_text: text.to_string(),
        }],
    }
}

fn an_action(title: &str, kind: &str) -> Value {
    json!({ "title": title, "kind": kind })
}

fn an_action_with_an_edit(title: &str, kind: &str, edit: Value) -> Value {
    json!({ "title": title, "kind": kind, "edit": edit })
}

fn an_action_with_a_command(title: &str, kind: &str, command: &str) -> Value {
    json!({ "title": title, "kind": kind, "command": { "title": title, "command": command, "arguments": [1] } })
}

fn by_title(title: &str) -> ActionSelector {
    ActionSelector {
        title: Some(title.to_string()),
        kind: None,
    }
}

fn by_kind(kind: &str) -> ActionSelector {
    ActionSelector {
        title: None,
        kind: Some(kind.to_string()),
    }
}

fn an_apply_code_action(selector: ActionSelector) -> RefactorOp {
    let mut op: RefactorOp = serde_json::from_value(json!({
        "op": "apply_code_action",
        "anchor": { "kind": "range", "file": FILE, "start": { "line": 2, "col": 1 }, "end": { "line": 3, "col": 12 } },
    }))
    .expect("the op parses");
    op.action = Some(selector);
    op
}

fn the_anchored_range() -> Range {
    Range {
        start: Position { line: 2, col: 1 },
        end: Position { line: 3, col: 12 },
    }
}

/// `apply_code_action` of `selector` resolved against `server`, asking for `only`.
fn resolved_against(
    server: &AServer,
    selector: ActionSelector,
    only: &[String],
) -> Result<Resolution> {
    let overlay = Overlay::new();
    resolve(
        server,
        &an_apply_code_action(selector),
        the_anchored_range(),
        only,
        &Workspace {
            root: Path::new("/workspace"),
            overlay: &overlay,
        },
    )
}

fn offered(title: &str, kind: &str) -> OfferedAction {
    OfferedAction {
        title: title.to_string(),
        kind: Some(kind.to_string()),
    }
}

#[test]
fn a_title_selects_exactly_that_action() {
    // Given two actions whose titles share a prefix
    let actions = vec![
        an_action("Extract function", "refactor.extract.function"),
        an_action("Extract function to module", "refactor.extract.function"),
    ];

    // When one is selected by its full title
    let selected = select(&actions, &by_title("Extract function")).expect("one action matches");

    // Then that one, not the one whose title merely starts with it
    assert_eq!(selected, &actions[0]);
}

#[test]
fn a_kind_selects_the_action_nested_under_it_and_not_a_sibling_prefix() {
    // Given one action under `refactor.extract`, one whose kind merely starts with its letters,
    // and one under another kind
    let actions = vec![
        an_action("Extractor", "refactor.extractor"),
        an_action("Extract table", "refactor.extract.table"),
        an_action("Inline table", "refactor.inline"),
    ];

    // When one is selected by kind
    let selected = select(&actions, &by_kind("refactor.extract")).expect("one action matches");

    // Then the nested kind is chosen
    assert_eq!(selected, &actions[1]);
}

#[test]
fn two_kind_matches_are_refused_listing_both() {
    // Given two actions under the same kind
    let actions = vec![
        an_action("Extract table", "refactor.extract.table"),
        an_action("Extract key", "refactor.extract.key"),
        an_action("Inline table", "refactor.inline"),
    ];

    // When an action is selected by that kind
    let refused = select(&actions, &by_kind("refactor.extract"));

    // Then it is refused with both matches, in the server's order
    match refused {
        Err(RestructureError::CodeActionRefused(CodeActionRefusal::Ambiguous {
            matches, ..
        })) => {
            assert_eq!(
                matches,
                vec![
                    offered("Extract table", "refactor.extract.table"),
                    offered("Extract key", "refactor.extract.key")
                ]
            )
        }
        other => panic!("expected an ambiguous selection, got {other:?}"),
    }
}

#[test]
fn no_match_is_refused_listing_everything_offered() {
    // Given two actions, neither titled as asked
    let actions = vec![
        an_action("Extract table", "refactor.extract.table"),
        an_action("Inline table", "refactor.inline"),
    ];

    // When an absent title is selected
    let refused = select(&actions, &by_title("Extract function"));

    // Then it is refused with every action offered
    match refused {
        Err(RestructureError::CodeActionRefused(CodeActionRefusal::NotOffered {
            offered: listed,
            ..
        })) => assert_eq!(
            listed,
            vec![
                offered("Extract table", "refactor.extract.table"),
                offered("Inline table", "refactor.inline")
            ]
        ),
        other => panic!("expected the selection to be refused, got {other:?}"),
    }
}

#[test]
fn an_actions_edit_is_its_resolution_with_paths_relative_to_the_root() {
    // Given a server offering an action that carries its edit
    let server = AServer::offering(vec![an_action_with_an_edit(
        "Extract table",
        "refactor.extract.table",
        an_lsp_edit(2, 0, 4, "listen_port"),
    )]);

    // When it is applied by title
    let resolution =
        resolved_against(&server, by_title("Extract table"), &[]).expect("the action resolves");

    // Then its edit is the resolution, in this crate's one-based, root-relative terms
    assert_eq!(
        resolution.edit,
        WorkspaceEdit {
            changes: vec![a_change(2, 0, 4, "listen_port")]
        }
    );
}

#[test]
fn the_server_is_asked_over_the_anchored_range_for_the_supports_kinds() {
    // Given a server offering one action
    let server = AServer::offering(vec![an_action_with_an_edit(
        "Extract table",
        "refactor.extract.table",
        an_lsp_edit(2, 0, 4, "listen_port"),
    )]);

    // When it is applied, the support asking for `refactor` actions only
    resolved_against(
        &server,
        by_title("Extract table"),
        &["refactor".to_string()],
    )
    .expect("the action resolves");

    // Then the server was asked once, over the file's range, for those kinds
    assert_eq!(
        server.asked_for.borrow().clone(),
        vec![(
            URI.to_string(),
            the_anchored_range(),
            vec!["refactor".to_string()]
        )]
    );
}

#[test]
fn an_action_without_an_edit_is_resolved_by_a_server_that_resolves() {
    // Given an action offered bare, which the server resolves to an edit
    let server = AServer::offering(vec![an_action("Extract table", "refactor.extract.table")])
        .resolving("Extract table", an_lsp_edit(2, 0, 4, "listen_port"));

    // When it is applied by kind
    let resolution =
        resolved_against(&server, by_kind("refactor.extract"), &[]).expect("the action resolves");

    // Then the resolved edit is the resolution
    assert_eq!(
        resolution.edit.changes,
        vec![a_change(2, 0, 4, "listen_port")]
    );
}

#[test]
fn a_command_actions_edits_are_its_resolution_in_arrival_order() {
    // Given an action that is a command, which sends two edits back while it runs
    let server = AServer::offering(vec![an_action_with_a_command(
        "Extract table",
        "refactor.extract.table",
        "fake.extract",
    )])
    .running(
        "fake.extract",
        vec![
            an_lsp_edit(2, 0, 4, "listen_port"),
            an_lsp_edit(0, 1, 7, "http"),
        ],
    );

    // When it is applied by title
    let resolution =
        resolved_against(&server, by_title("Extract table"), &[]).expect("the action resolves");

    // Then both edits are the resolution, in the order the server sent them
    assert_eq!(
        resolution.edit.changes,
        vec![a_change(2, 0, 4, "listen_port"), a_change(0, 1, 7, "http")]
    );
}

#[test]
fn a_command_that_sends_no_edit_is_refused_naming_the_action() {
    // Given an action that is a command which sends nothing back
    let server = AServer::offering(vec![an_action_with_a_command(
        "Extract table",
        "refactor.extract.table",
        "fake.nothing",
    )]);

    // When it is applied
    let refused = resolved_against(&server, by_title("Extract table"), &[]);

    // Then it is refused, naming the action
    match refused {
        Err(RestructureError::CodeActionRefused(CodeActionRefusal::NoEdit { title })) => {
            assert_eq!(title, "Extract table")
        }
        other => panic!("expected the edit-less action to be refused, got {other:?}"),
    }
}

#[test]
fn a_refusal_lists_what_was_offered_by_title_and_kind() {
    // Given a selection the server offered nothing for but two actions
    let refusal = CodeActionRefusal::NotOffered {
        wanted: "title `Extract function`".to_string(),
        offered: vec![
            offered("Extract table", "refactor.extract.table"),
            OfferedAction {
                title: "Run".to_string(),
                kind: None,
            },
        ],
    };

    // When it is written for the author
    let words = refusal.to_string();

    // Then it names the selection and every offered action with its kind
    assert_eq!(
        words,
        "no code action matches title `Extract function`; the server offered `Extract table` \
         (refactor.extract.table), `Run`"
    );
}
