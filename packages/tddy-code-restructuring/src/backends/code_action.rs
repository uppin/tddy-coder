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

use serde_json::Value;

use crate::edit::{Range, Resolution};
use crate::plan::{ActionSelector, RefactorOp};
use crate::registry::Workspace;
use crate::Result;

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
    let _ = (offered, selector);
    // TODO(reshape-extension-code-actions): implement
    todo!("code_action::select")
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
    let _ = (server, op, range, only, workspace);
    // TODO(reshape-extension-code-actions): implement
    todo!("code_action::resolve")
}

/// The server half of the generic path over the client every live backend already holds.
impl CodeActionServer for crate::backends::LspClientBridge {
    fn code_actions(&self, uri: &str, range: Range, only: &[String]) -> Result<Vec<Value>> {
        let _ = (uri, range, only);
        // TODO(reshape-extension-code-actions): implement
        todo!("LspClientBridge::code_actions")
    }

    fn resolves_code_actions(&self) -> bool {
        // TODO(reshape-extension-code-actions): implement
        todo!("LspClientBridge::resolves_code_actions")
    }

    fn resolve_code_action(&self, action: &Value) -> Result<Value> {
        let _ = action;
        // TODO(reshape-extension-code-actions): implement
        todo!("LspClientBridge::resolve_code_action")
    }

    fn execute_command(&self, command: &Value) -> Result<Vec<Value>> {
        let _ = command;
        // TODO(reshape-extension-code-actions): implement
        todo!("LspClientBridge::execute_command")
    }
}

#[cfg(test)]
mod tests;
