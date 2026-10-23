//! `apply_code_action`: one code action a language server offers, chosen by title or kind, run to
//! the edit it makes (`#reshape` extension-code-actions, R1–R3).
//!
//! The one path every backend with a server takes — [`crate::backends::ServerBackend`] for an
//! extension language and [`crate::backends::RustBackend`] for Rust alike. The backend resolves the
//! anchor to a range and opens the document; this asks `textDocument/codeAction` over that range,
//! selects one action (R1), and resolves it to an edit (R2): its `edit`; else, when it has none and
//! the server resolves actions, `codeAction/resolve`'s; else the edits its `command` sends back
//! through `workspace/applyEdit` while `workspace/executeCommand` runs, merged in arrival order.
//! Nothing is written here or by the server (R3): the edit is the operation's resolution, applied
//! by the run like any other.

use serde_json::{json, Value};

use super::server::{file_uri, lsp_range, relative_path, text_edit_of};
use crate::edit::{FileEdit, Range, Resolution, WorkspaceEdit};
use crate::plan::{ActionSelector, RefactorOp, Wanted};
use crate::registry::Workspace;
use crate::{RestructureError, Result};

/// An LSP `CodeAction`'s (and a `Command`'s) title.
const TITLE: &str = "title";
/// An LSP `CodeAction`'s kind.
const KIND: &str = "kind";
/// An LSP `CodeAction`'s `WorkspaceEdit`.
const EDIT: &str = "edit";
/// An LSP `CodeAction`'s `Command` — or, on a bare `Command` entry, the command's name.
const COMMAND: &str = "command";

/// What `apply_code_action` asks of a language server. Every method is one request, made on a
/// document the backend has already opened.
pub trait CodeActionServer {
    /// `textDocument/codeAction` over `range` of `uri`, asking for the kinds in `only` when it is
    /// not empty; the actions offered, verbatim (a `Command` entry is offered too).
    fn code_actions(&self, uri: &str, range: Range, only: &[String]) -> Result<Vec<Value>>;

    /// Whether the server resolves code actions (`codeActionProvider.resolveProvider`).
    fn resolves_code_actions(&self) -> bool;

    /// `codeAction/resolve` of `action`; the resolved action, verbatim.
    fn resolve_code_action(&self, action: &Value) -> Result<Value>;

    /// `workspace/executeCommand` of `command` (an LSP `Command`): the `WorkspaceEdit` of every
    /// `workspace/applyEdit` the server sent during the call, in arrival order.
    fn execute_command(&self, command: &Value) -> Result<Vec<Value>>;
}

/// One action a server offered, as a refusal lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfferedAction {
    pub title: String,
    /// The action's LSP kind; a bare `Command` has none.
    pub kind: Option<String>,
}

impl std::fmt::Display for OfferedAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.kind {
            Some(kind) => write!(f, "`{}` ({kind})", self.title),
            None => write!(f, "`{}`", self.title),
        }
    }
}

/// Why an `apply_code_action` was refused once the server had answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodeActionRefusal {
    /// No offered action matched `wanted`; `offered` is everything the server offered.
    NotOffered {
        wanted: String,
        offered: Vec<OfferedAction>,
    },
    /// More than one offered action matched `wanted`; `matches` are those, in the server's order.
    Ambiguous {
        wanted: String,
        matches: Vec<OfferedAction>,
    },
    /// The chosen action resolved to no edit: no `edit`, none from `codeAction/resolve`, and its
    /// command sent no `workspace/applyEdit`.
    NoEdit { title: String },
}

impl std::fmt::Display for CodeActionRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let list = |actions: &[OfferedAction]| {
            actions
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        };
        match self {
            CodeActionRefusal::NotOffered { wanted, offered } if offered.is_empty() => {
                write!(
                    f,
                    "no code action matches {wanted}: the server offered none"
                )
            }
            CodeActionRefusal::NotOffered { wanted, offered } => write!(
                f,
                "no code action matches {wanted}; the server offered {}",
                list(offered)
            ),
            CodeActionRefusal::Ambiguous { wanted, matches } => write!(
                f,
                "{} code actions match {wanted}: {} — name one by `title`",
                matches.len(),
                list(matches)
            ),
            CodeActionRefusal::NoEdit { title } => write!(
                f,
                "code action `{title}` made no edit: it carried none, resolved to none, and its \
                 command sent none"
            ),
        }
    }
}

/// R1: the one action of `offered` that `selector` picks — `title` equal to the action's title,
/// or `kind` a prefix of the action's kind as LSP kinds nest (`refactor.extract` picks
/// `refactor.extract.function`, not `refactor.extractor`). None matching, or more than one, is
/// [`crate::RestructureError::CodeActionRefused`].
pub fn select<'a>(offered: &'a [Value], selector: &ActionSelector) -> Result<&'a Value> {
    let (wanted, matches): (String, Vec<&Value>) = match selector.wanted()? {
        Wanted::Title(title) => (
            format!("title `{title}`"),
            offered
                .iter()
                .filter(|action| action[TITLE].as_str() == Some(title))
                .collect(),
        ),
        Wanted::Kind(kind) => (
            format!("kind `{kind}`"),
            offered
                .iter()
                .filter(|action| action[KIND].as_str().is_some_and(|k| nests_under(k, kind)))
                .collect(),
        ),
    };
    match matches.as_slice() {
        [one] => Ok(one),
        [] => Err(refused(CodeActionRefusal::NotOffered {
            wanted,
            offered: offered.iter().map(offered_action).collect(),
        })),
        _ => Err(refused(CodeActionRefusal::Ambiguous {
            wanted,
            matches: matches.into_iter().map(offered_action).collect(),
        })),
    }
}

/// Whether the LSP kind `actual` is `wanted` or nested under it: `refactor.extract.function` is
/// under `refactor.extract`, `refactor.extractor` is not.
fn nests_under(actual: &str, wanted: &str) -> bool {
    actual == wanted
        || actual
            .strip_prefix(wanted)
            .is_some_and(|rest| rest.starts_with('.'))
}

fn offered_action(action: &Value) -> OfferedAction {
    OfferedAction {
        title: action[TITLE].as_str().unwrap_or_default().to_string(),
        kind: action[KIND].as_str().map(str::to_string),
    }
}

fn refused(refusal: CodeActionRefusal) -> RestructureError {
    RestructureError::CodeActionRefused(refusal)
}

/// `op` — an `apply_code_action` — over `range` of its anchor's file, asking `server` for the kinds
/// in `only` (the support's `code_action_kinds`, empty for none): the chosen action resolved to an
/// edit (R1–R2), its paths relative to `workspace.root`.
pub fn resolve(
    server: &dyn CodeActionServer,
    op: &RefactorOp,
    range: Range,
    only: &[String],
    workspace: &Workspace<'_>,
) -> Result<Resolution> {
    let selector = op.action.as_ref().ok_or_else(|| {
        RestructureError::MalformedPlan("`apply_code_action` needs `action`".to_string())
    })?;
    let uri = file_uri(workspace, op.anchor.file());
    let offered = server.code_actions(&uri, range, only)?;
    let chosen = select(&offered, selector)?;
    let action = if edit_of(chosen).is_none() && server.resolves_code_actions() {
        server.resolve_code_action(chosen)?
    } else {
        chosen.clone()
    };
    let edits = edits_of_action(server, &action)?;
    let changes = edits
        .iter()
        .map(|edit| file_edits_of(edit, workspace))
        .collect::<Result<Vec<_>>>()?
        .concat();
    if changes.is_empty() {
        return Err(refused(CodeActionRefusal::NoEdit {
            title: action[TITLE].as_str().unwrap_or_default().to_string(),
        }));
    }
    Ok(Resolution::of(WorkspaceEdit { changes }))
}

/// R2: the `WorkspaceEdit`s `action` stands for — its own `edit`, else what its command sends back
/// through `workspace/applyEdit` while it runs, in arrival order; none for an action with neither.
fn edits_of_action(server: &dyn CodeActionServer, action: &Value) -> Result<Vec<Value>> {
    if let Some(edit) = edit_of(action) {
        return Ok(vec![edit.clone()]);
    }
    match action.get(COMMAND) {
        Some(command) if command.is_object() => server.execute_command(command),
        // A bare `Command` entry: its `command` is the name, and the entry is the command.
        Some(Value::String(_)) => server.execute_command(action),
        _ => Ok(Vec::new()),
    }
}

/// The `edit` an action carries; an `"edit": null` carries none, as an absent one does.
fn edit_of(action: &Value) -> Option<&Value> {
    action.get(EDIT).filter(|edit| !edit.is_null())
}

/// Every document of an LSP `WorkspaceEdit` as one change each, paths relative to the root, in the
/// order the edit lists them. A change that is not a text edit — a file operation — is refused
/// rather than half-applied.
fn file_edits_of(edit: &Value, workspace: &Workspace<'_>) -> Result<Vec<FileEdit>> {
    let not_text = || {
        RestructureError::ServerDefect(
            "the code action's edit holds a change that is not a text edit".to_string(),
        )
    };
    let mut documents: Vec<(&str, &Vec<Value>)> = Vec::new();
    if let Some(changes) = edit.get("documentChanges").and_then(Value::as_array) {
        for change in changes {
            let named = change
                .pointer("/textDocument/uri")
                .and_then(Value::as_str)
                .zip(change.get("edits").and_then(Value::as_array));
            documents.push(named.ok_or_else(not_text)?);
        }
    } else if let Some(changes) = edit.get("changes").and_then(Value::as_object) {
        for (uri, edits) in changes {
            documents.push((uri.as_str(), edits.as_array().ok_or_else(not_text)?));
        }
    }
    documents
        .into_iter()
        .filter(|(_, edits)| !edits.is_empty())
        .map(|(uri, edits)| {
            Ok(FileEdit::Change {
                path: relative_path(uri, workspace)?,
                edits: edits.iter().map(text_edit_of).collect::<Result<_>>()?,
            })
        })
        .collect()
}

/// The server half of the generic path over the client every live backend already holds.
impl CodeActionServer for crate::backends::LspClientBridge {
    fn code_actions(&self, uri: &str, range: Range, only: &[String]) -> Result<Vec<Value>> {
        let mut context = json!({ "diagnostics": [] });
        if !only.is_empty() {
            context["only"] = json!(only);
        }
        let answer = self.request(
            "textDocument/codeAction",
            json!({
                "textDocument": { "uri": uri },
                "range": lsp_range(range)?,
                "context": context,
            }),
        )?;
        match answer {
            Value::Array(actions) => Ok(actions),
            Value::Null => Ok(Vec::new()),
            other => Err(RestructureError::ServerDefect(format!(
                "`textDocument/codeAction` answered neither a list of actions nor null: {other}"
            ))),
        }
    }

    fn resolves_code_actions(&self) -> bool {
        self.handshake()
            .pointer("/capabilities/codeActionProvider/resolveProvider")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }

    fn resolve_code_action(&self, action: &Value) -> Result<Value> {
        self.request("codeAction/resolve", action.clone())
    }

    fn execute_command(&self, command: &Value) -> Result<Vec<Value>> {
        let name = command[COMMAND].as_str().ok_or_else(|| {
            RestructureError::ServerDefect("a code action's command names no command".to_string())
        })?;
        let arguments = command["arguments"].as_array().cloned().unwrap_or_default();
        Ok(self.execute(name, arguments)?.applied_edits)
    }
}

#[cfg(test)]
mod tests;
