//! A language whose server is any LSP, not rust-analyzer: `rename_symbol` through
//! `textDocument/rename`, every item anchor through the outline backend it is built over
//! (`#reshape` extension-language-server, R6).
//!
//! Nothing here knows a language. The support that builds it — an installed extension's — hands in
//! the backend that already answers its item anchors, and the server it started from the launch the
//! user configured. A support whose server has no configured command builds it
//! [`ServerBackend::without_command`], so a rename is refused in the support's words before any
//! process starts (R3) rather than as an operation the language does not have.

use std::sync::Arc;

use serde_json::{json, Value};
use tddy_lsp::client::LspClient;
use tddy_lsp::LanguageId;
use tokio_util::sync::CancellationToken;

use super::lsp_bridge::{map_lsp_error, LspClientBridge};
use crate::edit::{FileEdit, Position, Range, TextEdit, WorkspaceEdit};
use crate::item_anchor::{span_of, ItemAtResolver, ItemResolver};
use crate::language::LiveServer;
use crate::plan::{Anchor, RefactorKind, RefactorOp};
use crate::registry::{LanguageBackend, Workspace};
use crate::{Resolution, RestructureError, Result};

/// The position encoding [`Range`] columns are counted in: bytes, which is what the client asks
/// the server for (`utf-8`) and the only unit this backend converts without a text in hand.
const BYTE_ENCODING: &str = "utf-8";

/// A language's backend over a generic language server, composed with its outline backend.
pub struct ServerBackend {
    /// Answers the item anchors — the language's id, the extensions it claims, `item` / `items` /
    /// `--at` — exactly as it does with no server.
    outline: Box<dyn LanguageBackend>,
    server: Server,
    /// The kinds `apply_code_action` asks the server for (`context.only`); empty asks for all.
    code_action_kinds: Vec<String>,
}

/// What a server operation is put to.
enum Server {
    /// The language's running server.
    Live(LiveLsp),
    /// The server cannot be started — it needs a command nobody configured, or its extension
    /// refused it: every server operation is refused with exactly this.
    NoCommand { reason: String },
}

/// The language's running server, as each operation reaches it.
struct LiveLsp {
    client: Arc<LspClient>,
    /// What ends a wait on the server; the caller's.
    cancel: CancellationToken,
    /// The LSP `languageId` a document is opened as — the launch's.
    language_id: String,
}

impl LiveLsp {
    /// `file` opened on the server, once it is known to count positions in bytes; its uri.
    fn open(&self, workspace: &Workspace<'_>, file: &str) -> Result<String> {
        refuse_unless_bytes(&self.client)?;
        open_document(&self.client, &self.language_id, workspace, file)
    }

    fn bridge(&self) -> LspClientBridge {
        LspClientBridge::new(Arc::clone(&self.client), self.cancel.clone())
    }
}

impl ServerBackend {
    /// The backend over `server`, a running server of the language `outline` answers anchors for,
    /// opening documents as `language_id`.
    pub fn over(
        outline: Box<dyn LanguageBackend>,
        server: &LiveServer<'_>,
        language_id: impl Into<String>,
    ) -> Self {
        Self {
            outline,
            server: Server::Live(LiveLsp {
                client: Arc::clone(server.client),
                cancel: server.cancel.clone(),
                language_id: language_id.into(),
            }),
            code_action_kinds: Vec::new(),
        }
    }

    /// This backend, asking the server only for code actions of `kinds` — the manifest's
    /// `code_action_kinds` for the server (`#reshape` extension-code-actions); none asks for all.
    pub fn with_code_action_kinds(self, kinds: Vec<String>) -> Self {
        Self {
            code_action_kinds: kinds,
            ..self
        }
    }

    /// The backend of a language whose server cannot be started — no configured command (R3), or
    /// a code extension's refusal (K24, developer decision 2026-10-20): item anchors still answer,
    /// and every server operation is [`crate::RestructureError::ServerNotConfigured`] carrying
    /// exactly `reason`, whatever it says.
    pub fn without_command(outline: Box<dyn LanguageBackend>, reason: impl Into<String>) -> Self {
        Self {
            outline,
            server: Server::NoCommand {
                reason: reason.into(),
            },
            code_action_kinds: Vec::new(),
        }
    }
}

impl LanguageBackend for ServerBackend {
    /// The outline backend's.
    fn language(&self) -> &LanguageId {
        self.outline.language()
    }

    /// The outline backend's.
    fn handles_extension(&self, extension: &str) -> bool {
        self.outline.handles_extension(extension)
    }

    /// `rename_symbol`, and `apply_code_action` — any action the server offers.
    fn supports(&self, kind: RefactorKind) -> bool {
        matches!(
            kind,
            RefactorKind::RenameSymbol | RefactorKind::ApplyCodeAction
        )
    }

    /// The operation over the language's running server, as [`resolve_code_action`] and
    /// [`resolve_rename`] say. Without a command: [`crate::RestructureError::ServerNotConfigured`].
    fn resolve(&mut self, op: &RefactorOp, workspace: &Workspace<'_>) -> Result<Resolution> {
        let live = match &self.server {
            Server::NoCommand { reason } => {
                return Err(RestructureError::ServerNotConfigured {
                    language: self.outline.language().to_string(),
                    reason: reason.clone(),
                })
            }
            Server::Live(live) => live,
        };
        match op.op {
            RefactorKind::ApplyCodeAction => resolve_code_action(
                self.outline.as_mut(),
                live,
                &self.code_action_kinds,
                op,
                workspace,
            ),
            RefactorKind::RenameSymbol => {
                resolve_rename(self.outline.as_mut(), live, op, workspace)
            }
            _ => Err(RestructureError::UnsupportedOp {
                backend: self.outline.language().to_string(),
                op: format!("{:?}", op.op),
            }),
        }
    }

    /// The outline backend's.
    fn item_resolver(&mut self) -> Option<&mut dyn ItemResolver> {
        self.outline.item_resolver()
    }

    /// The outline backend's.
    fn item_locator(&mut self) -> Option<&mut dyn ItemAtResolver> {
        self.outline.item_locator()
    }
}

/// `apply_code_action`: the anchor's range — a range anchor's own, an item's as the outline backend
/// locates it — asked of the server for the code actions of `kinds`, and the chosen one resolved to
/// an edit by the generic path every backend with a server takes.
fn resolve_code_action(
    outline: &mut dyn LanguageBackend,
    live: &LiveLsp,
    kinds: &[String],
    op: &RefactorOp,
    workspace: &Workspace<'_>,
) -> Result<Resolution> {
    let range = anchor_range(outline, &op.anchor, workspace)?;
    live.open(workspace, op.anchor.file())?;
    super::code_action::resolve(&live.bridge(), op, range, kinds, workspace)
}

/// R6: the anchor resolves to an item through the outline backend; the document is opened as the
/// launch's `languageId`, `textDocument/rename` is sent at the item's name with `op.name`, and the
/// returned `WorkspaceEdit` — paths relative to the root — is the resolution.
fn resolve_rename(
    outline: &mut dyn LanguageBackend,
    live: &LiveLsp,
    op: &RefactorOp,
    workspace: &Workspace<'_>,
) -> Result<Resolution> {
    let new_name = op
        .name
        .clone()
        .ok_or_else(|| RestructureError::MalformedPlan("rename_symbol needs a name".to_string()))?;
    let at = name_position(outline, &op.anchor, workspace)?;
    let uri = live.open(workspace, op.anchor.file())?;
    let answer = live.bridge().request(
        "textDocument/rename",
        json!({
            "textDocument": { "uri": uri },
            "position": lsp_position(at)?,
            "newName": new_name,
        }),
    )?;
    Ok(Resolution::of(workspace_edit_of(
        &answer, workspace, op, at,
    )?))
}

/// `at` in LSP's zero-based coordinates. [`Position`] is one-based and nothing upstream of a server
/// request rejects a zero, so a zero line or column is refused here rather than wrapped.
pub(crate) fn lsp_position(at: Position) -> Result<Value> {
    match (at.line.checked_sub(1), at.col.checked_sub(1)) {
        (Some(line), Some(character)) => Ok(json!({ "line": line, "character": character })),
        _ => Err(RestructureError::MalformedPlan(format!(
            "positions are one-based: line {} column {} names no position",
            at.line, at.col
        ))),
    }
}

/// `range` in LSP's zero-based coordinates, refused as [`lsp_position`] refuses either end.
pub(crate) fn lsp_range(range: Range) -> Result<Value> {
    Ok(json!({ "start": lsp_position(range.start)?, "end": lsp_position(range.end)? }))
}

/// Where the rename is asked: a range anchor's start, an item anchor's name as the outline backend
/// locates it. A symbol anchor names no position this backend can find, and is refused.
fn name_position(
    outline: &mut dyn LanguageBackend,
    anchor: &Anchor,
    workspace: &Workspace<'_>,
) -> Result<Position> {
    Ok(anchor_range(outline, anchor, workspace)?.start)
}

/// The range an anchor names: a range anchor's own, an item anchor's as the outline backend
/// locates it. A symbol anchor names no range this backend can find, and is refused.
fn anchor_range(
    outline: &mut dyn LanguageBackend,
    anchor: &Anchor,
    workspace: &Workspace<'_>,
) -> Result<Range> {
    if let Anchor::Range { start, end, .. } = anchor {
        return Ok(Range {
            start: *start,
            end: *end,
        });
    }
    let backend = outline.language().to_string();
    let resolver = outline
        .item_resolver()
        .ok_or(RestructureError::UnsupportedOp {
            backend,
            op: "item anchors".to_string(),
        })?;
    span_of(anchor, workspace.root, resolver)
}

/// Announce `file`'s text to the server as open — as the launch's `languageId` — and return its uri.
fn open_document(
    client: &LspClient,
    language_id: &str,
    workspace: &Workspace<'_>,
    file: &str,
) -> Result<String> {
    let uri = file_uri(workspace, file);
    let text = workspace.read(file)?;
    tokio::runtime::Handle::current()
        .block_on(client.sync_document(&uri, language_id, &text))
        .map_err(map_lsp_error)?;
    Ok(uri)
}

/// Refuses a server that settled on anything but byte positions: [`Range`] columns are bytes, and
/// counting them against UTF-16 code units mis-addresses every line holding a wider character.
fn refuse_unless_bytes(client: &LspClient) -> Result<()> {
    let handshake = client.handshake();
    let encoding = handshake
        .pointer("/capabilities/positionEncoding")
        .and_then(Value::as_str)
        .unwrap_or("utf-16");
    if encoding == BYTE_ENCODING {
        return Ok(());
    }
    Err(RestructureError::ServerDefect(format!(
        "the language server counts positions in {encoding}, and this client asked for \
         {BYTE_ENCODING}: its rename would be mis-addressed"
    )))
}

/// The `file://` uri of `file` under the workspace root — spelled as the server's root uri is.
pub(super) fn file_uri(workspace: &Workspace<'_>, file: &str) -> String {
    file_uri_under(workspace.root, file)
}

/// The `file://` uri of `file`, a path relative to `root`, spelled as a server's root uri is: the
/// one spelling every front end that announces a document to a server uses.
pub fn file_uri_under(root: &std::path::Path, file: &str) -> String {
    format!("file://{}", root.join(file).display())
}

/// The server's rename answer as an edit: every document it names, paths relative to the root.
/// An answer of null (nothing renameable at `at`), a resource operation, or a document outside the
/// root is refused rather than half-applied.
fn workspace_edit_of(
    answer: &Value,
    workspace: &Workspace<'_>,
    op: &RefactorOp,
    at: Position,
) -> Result<WorkspaceEdit> {
    let documents: Vec<(&str, &Vec<Value>)> = if let Some(changes) = answer.get("documentChanges") {
        let entries = changes.as_array().map(Vec::as_slice).unwrap_or_default();
        let mut documents = Vec::new();
        for entry in entries {
            let named = entry
                .pointer("/textDocument/uri")
                .and_then(Value::as_str)
                .zip(entry.get("edits").and_then(Value::as_array));
            documents.push(named.ok_or_else(|| {
                RestructureError::ServerDefect(
                    "the rename answer holds a change that is not a text edit".to_string(),
                )
            })?);
        }
        documents
    } else {
        answer
            .get("changes")
            .and_then(Value::as_object)
            .map(|changes| {
                changes
                    .iter()
                    .filter_map(|(uri, edits)| Some((uri.as_str(), edits.as_array()?)))
                    .collect()
            })
            .unwrap_or_default()
    };
    let documents: Vec<_> = documents
        .into_iter()
        .filter(|(_, edits)| !edits.is_empty())
        .collect();
    if documents.is_empty() {
        return Err(RestructureError::MalformedPlan(format!(
            "the language server renamed nothing at {}:{}:{}",
            op.anchor.file(),
            at.line,
            at.col
        )));
    }
    let changes = documents
        .into_iter()
        .map(|(uri, edits)| {
            Ok(FileEdit::Change {
                path: relative_path(uri, workspace)?,
                edits: edits.iter().map(text_edit_of).collect::<Result<_>>()?,
            })
        })
        .collect::<Result<_>>()?;
    Ok(WorkspaceEdit { changes })
}

/// The root-relative path a `file://` uri names; one outside the root is refused.
pub(super) fn relative_path(uri: &str, workspace: &Workspace<'_>) -> Result<String> {
    uri.strip_prefix("file://")
        .and_then(|path| std::path::Path::new(path).strip_prefix(workspace.root).ok())
        .map(|relative| relative.display().to_string())
        .ok_or_else(|| {
            RestructureError::ServerDefect(format!("`{uri}` is not a file inside the workspace"))
        })
}

/// One LSP text edit in one-based coordinates, refusing a malformed one.
pub(super) fn text_edit_of(edit: &Value) -> Result<TextEdit> {
    let point = |at: &str| -> Result<Position> {
        let read = |field: &str| {
            edit.pointer(&format!("/range/{at}/{field}"))
                .and_then(Value::as_u64)
                .map(|value| value as u32 + 1)
                .ok_or_else(|| {
                    RestructureError::ServerDefect(format!("a text edit has no range {at} {field}"))
                })
        };
        Ok(Position {
            line: read("line")?,
            col: read("character")?,
        })
    };
    Ok(TextEdit {
        range: Range {
            start: point("start")?,
            end: point("end")?,
        },
        new_text: edit
            .get("newText")
            .and_then(Value::as_str)
            .ok_or_else(|| RestructureError::ServerDefect("a text edit has no newText".into()))?
            .to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay::Overlay;
    use crate::RestructureError;
    use std::path::Path;

    /// An outline backend that claims `.toml`, supports nothing and answers no anchors.
    struct AnOutlineBackend {
        id: LanguageId,
    }

    impl LanguageBackend for AnOutlineBackend {
        fn language(&self) -> &LanguageId {
            &self.id
        }

        fn handles_extension(&self, extension: &str) -> bool {
            extension == "toml"
        }

        fn supports(&self, _kind: RefactorKind) -> bool {
            false
        }

        fn resolve(&mut self, _op: &RefactorOp, _workspace: &Workspace<'_>) -> Result<Resolution> {
            unreachable!("an outline backend resolves nothing")
        }
    }

    fn an_outline_backend_for(id: &str) -> Box<dyn LanguageBackend> {
        Box::new(AnOutlineBackend {
            id: LanguageId::new(id),
        })
    }

    const NO_COMMAND: &str = "language server fake-ls (extension demo-toml) has no command: \
                              set lsp.fake-ls.binary.path in /data/extensions/settings.json";

    fn a_rename_of_port() -> RefactorOp {
        serde_json::from_str(
            r#"{"op":"rename_symbol","anchor":{"kind":"range","file":"config/app.toml","start":{"line":3,"col":1},"end":{"line":3,"col":5}},"name":"listen_port"}"#,
        )
        .expect("a rename parses")
    }

    /// A code extension's refusal (K24), not R3's missing command.
    const EXTENSION_REFUSED: &str = "extension demo-toml: language server fake-ls asked to \
                                     download https://example.com/fake-ls.tar.gz; downloads \
                                     are not allowed";

    #[test]
    fn a_rename_is_refused_with_exactly_an_extensions_refusal() {
        // Given a backend whose server a code extension refused to start
        let mut backend =
            ServerBackend::without_command(an_outline_backend_for("demo-toml"), EXTENSION_REFUSED);
        let overlay = Overlay::new();

        // When a rename is resolved
        let refused = backend.resolve(
            &a_rename_of_port(),
            &Workspace {
                root: Path::new("/workspace"),
                overlay: &overlay,
            },
        );

        // Then it is refused with the extension's words, unchanged, naming the language
        match refused {
            Err(RestructureError::ServerNotConfigured { language, reason }) => {
                assert_eq!(
                    (language.as_str(), reason.as_str()),
                    ("demo-toml", EXTENSION_REFUSED)
                )
            }
            other => panic!("expected ServerNotConfigured, got {other:?}"),
        }
    }

    #[test]
    fn rename_is_the_only_operation_it_supports() {
        // Given a backend over an outline, its server unconfigured
        let backend =
            ServerBackend::without_command(an_outline_backend_for("demo-toml"), NO_COMMAND);

        // When it is asked which operations it performs
        let supported = [
            RefactorKind::RenameSymbol,
            RefactorKind::ExtractMethod,
            RefactorKind::MoveItem,
        ]
        .map(|kind| backend.supports(kind));

        // Then rename alone
        assert_eq!(supported, [true, false, false]);
    }

    #[test]
    fn it_is_the_outline_backends_language() {
        // Given a backend over the demo-toml outline
        let backend =
            ServerBackend::without_command(an_outline_backend_for("demo-toml"), NO_COMMAND);

        // When it is routed to
        let routed = (
            backend.language().to_string(),
            backend.handles_extension("toml"),
            backend.handles_extension("rs"),
        );

        // Then it answers for the outline's language and files
        assert_eq!(routed, ("demo-toml".to_string(), true, false));
    }

    #[test]
    fn without_a_command_a_rename_is_refused_with_the_supports_reason() {
        // Given a backend whose server has no configured command
        let mut backend =
            ServerBackend::without_command(an_outline_backend_for("demo-toml"), NO_COMMAND);
        let overlay = Overlay::new();

        // When a rename is resolved
        let refused = backend.resolve(
            &a_rename_of_port(),
            &Workspace {
                root: Path::new("/workspace"),
                overlay: &overlay,
            },
        );

        // Then it is refused in the support's words, naming the language
        match refused {
            Err(RestructureError::ServerNotConfigured { language, reason }) => {
                assert_eq!(
                    (language.as_str(), reason.as_str()),
                    ("demo-toml", NO_COMMAND)
                )
            }
            other => panic!("expected ServerNotConfigured, got {other:?}"),
        }
    }
}
