//! An [`LspExecutor`] answered by the warm code-intelligence index the daemon manages, rather than
//! by a language server of its own.
//!
//! [`crate::TddyLspExecutor`] starts a rust-analyzer per `BUILD.yaml` target and pays a cold
//! crate-graph load for it. When the daemon manages an index (`index_daemon:` in its config), the
//! session's `Lsp*` tools ask that index instead — `code_index.CodeIndexService`'s `Definition`,
//! `References`, `Hover`, `Symbols` and `Diagnostics`, rooted at the session's worktree — so a
//! session and the web's code pane ask the same index and no second server is started.
//!
//! The worktree is bound **on the host**: the `repo_dir` every method receives is the worktree the
//! host resolved from the session (`tddy_tool_engine::execute_tool_with_env`'s `worktree_root`), and
//! [`bind_to_session_worktree`] refuses any queried file outside it before the index is asked. A
//! path the jail supplies is never used as the root.
//!
//! The tools' names, argument schemas and result JSON are [`crate::TddyLspExecutor`]'s, unchanged:
//! zero-based LSP positions in and out, `file://` URIs, the same top-level keys. The coordinate
//! translation to the index's one-based byte positions is this module's.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use serde_json::{json, Value};
use tddy_core::toolcall::lsp::{LspExecutor, LspQuery};
use tddy_index_daemon::proto::code_index as index;
use tddy_index_daemon::proto::tonic_code_index::code_index_service_client::CodeIndexServiceClient;
use tddy_lsp::{Diagnostic, Location, Position, Range, SymbolInfo};
use tonic::transport::Channel;

use crate::{block_on, diagnostic_json, location_json, symbol_json};

/// Where a channel to the warm index comes from — in production the daemon's
/// `IndexDaemonRegistry::connect`, which starts the index daemon on first use.
///
/// A port rather than the registry itself because the registry lives in `tddy-daemon`, which
/// depends on this crate.
#[async_trait::async_trait]
pub trait IndexChannel: Send + Sync {
    /// A gRPC channel to the index, starting it if nothing has yet.
    async fn connect(&self) -> Result<tonic::transport::Channel, String>;
}

/// Answers the `Lsp*` tools from the warm index reached through an [`IndexChannel`].
pub struct IndexLspExecutor {
    index: Arc<dyn IndexChannel>,
}

impl IndexLspExecutor {
    /// An executor asking the index `index` connects to.
    #[must_use]
    pub fn new(index: Arc<dyn IndexChannel>) -> Self {
        Self { index }
    }

    /// A client on a fresh channel to the index.
    async fn client(&self) -> Result<CodeIndexServiceClient<Channel>, String> {
        let channel = self
            .index
            .connect()
            .await
            .map_err(|err| format!("index daemon: {err}"))?;
        Ok(CodeIndexServiceClient::new(channel))
    }
}

impl LspExecutor for IndexLspExecutor {
    /// Whether the worktree holds a Rust workspace, the one language the index serves.
    fn is_available(&self, repo_dir: &Path) -> bool {
        repo_dir.join("Cargo.toml").is_file()
    }

    fn diagnostics(&self, repo_dir: &Path, query: &LspQuery) -> Result<Value, String> {
        let file = bind_to_session_worktree(repo_dir, &query.file)?;
        let path = repo_dir.join(&file);
        block_on(async {
            let answered = self
                .client()
                .await?
                .diagnostics(index::DiagnosticsRequest {
                    workspace_root: repo_dir.display().to_string(),
                    file,
                })
                .await
                .map_err(|status| status.message().to_string())?
                .into_inner();
            let mut columns = Columns::within(repo_dir);
            let diagnostics = answered
                .diagnostics
                .iter()
                .map(|diagnostic| {
                    Ok(diagnostic_json(&Diagnostic {
                        range: columns.lsp_range(&path, diagnostic.range)?,
                        severity: severity_of(diagnostic.severity)?,
                        message: diagnostic.message.clone(),
                        source: diagnostic.source.clone(),
                    }))
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok(json!({ "diagnostics": diagnostics }))
        })
    }

    fn definition(&self, repo_dir: &Path, query: &LspQuery) -> Result<Value, String> {
        let locations = self.navigate(repo_dir, query, Navigation::Definition)?;
        Ok(json!({ "locations": locations }))
    }

    fn references(&self, repo_dir: &Path, query: &LspQuery) -> Result<Value, String> {
        let locations = self.navigate(repo_dir, query, Navigation::References)?;
        Ok(json!({ "references": locations }))
    }

    fn hover(&self, repo_dir: &Path, query: &LspQuery) -> Result<Value, String> {
        let request = self.position_request(repo_dir, query)?;
        let answered = block_on(async {
            self.client()
                .await?
                .hover(index::HoverRequest {
                    workspace_root: request.workspace_root,
                    file: request.file,
                    position: request.position,
                })
                .await
                .map_err(|status| status.message().to_string())
        })?
        .into_inner();
        Ok(json!({ "hover": answered.markdown }))
    }

    fn symbols(&self, repo_dir: &Path, query: &LspQuery) -> Result<Value, String> {
        // A workspace search names no file; the index ignores the file when it is given a query.
        let file = match &query.symbol_query {
            Some(_) => String::new(),
            None => bind_to_session_worktree(repo_dir, &query.file)?,
        };
        let answered = block_on(async {
            self.client()
                .await?
                .symbols(index::SymbolsRequest {
                    workspace_root: repo_dir.display().to_string(),
                    file,
                    query: query.symbol_query.clone(),
                })
                .await
                .map_err(|status| status.message().to_string())
        })?
        .into_inner();
        let mut columns = Columns::within(repo_dir);
        let symbols = answered
            .symbols
            .iter()
            .map(|symbol| {
                let location = symbol
                    .location
                    .as_ref()
                    .ok_or("the index answered a symbol without a location")?;
                Ok(symbol_json(&SymbolInfo {
                    name: symbol.name.clone(),
                    kind: u8::try_from(symbol.kind).map_err(|_| {
                        format!("symbol kind {} is not an LSP SymbolKind", symbol.kind)
                    })?,
                    location: columns.lsp_location(location)?,
                    container: symbol.container.clone(),
                }))
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(json!({ "symbols": symbols }))
    }

    /// The index answers diagnostics for one file at a time and has no workspace-wide query, so
    /// `ReadLints` is refused rather than answered by a second language server of this executor's
    /// own: that one would disagree with the index every other pane asks.
    fn workspace_diagnostics(&self, _repo_dir: &Path) -> Result<Value, String> {
        // TODO(docs/dev/todo/2026-10-04-read-lints-is-refused-through-the-warm-index.md): a
        // workspace-wide `Diagnostics` would let `ReadLints` use the index.
        Err("ReadLints is not available through the warm index: it reports diagnostics one file at a time — use LspDiagnostics with a file".to_string())
    }
}

/// Which location-answering navigation query to put to the index.
#[derive(Clone, Copy)]
enum Navigation {
    Definition,
    References,
}

/// A navigation request's fields, bound to the session's worktree and in the index's coordinates.
struct PositionRequest {
    workspace_root: String,
    file: String,
    position: Option<index::SourcePosition>,
}

impl IndexLspExecutor {
    /// The tool's file and zero-based position as the index's request fields: the file bound to the
    /// worktree, the position one-based with its column in bytes.
    fn position_request(
        &self,
        repo_dir: &Path,
        query: &LspQuery,
    ) -> Result<PositionRequest, String> {
        let file = bind_to_session_worktree(repo_dir, &query.file)?;
        let column = byte_column(&repo_dir.join(&file), query.line, query.character)?;
        Ok(PositionRequest {
            workspace_root: repo_dir.display().to_string(),
            file,
            position: Some(index::SourcePosition {
                line: query.line + 1,
                column: column + 1,
            }),
        })
    }

    /// The index's answer to a definition or references query, as the tool's location JSON.
    fn navigate(
        &self,
        repo_dir: &Path,
        query: &LspQuery,
        navigation: Navigation,
    ) -> Result<Vec<Value>, String> {
        let request = self.position_request(repo_dir, query)?;
        let answered = block_on(async {
            let mut client = self.client().await?;
            match navigation {
                Navigation::Definition => client
                    .definition(index::DefinitionRequest {
                        workspace_root: request.workspace_root,
                        file: request.file,
                        position: request.position,
                    })
                    .await
                    .map(|answer| answer.into_inner().locations),
                Navigation::References => client
                    .references(index::ReferencesRequest {
                        workspace_root: request.workspace_root,
                        file: request.file,
                        position: request.position,
                    })
                    .await
                    .map(|answer| answer.into_inner().locations),
            }
            .map_err(|status| status.message().to_string())
        })?;
        let mut columns = Columns::within(repo_dir);
        answered
            .iter()
            .map(|location| Ok(location_json(&columns.lsp_location(location)?)))
            .collect()
    }
}

/// The index's severity code as the LSP's one-byte one.
fn severity_of(severity: u32) -> Result<u8, String> {
    u8::try_from(severity).map_err(|_| format!("severity {severity} is not an LSP severity"))
}

/// The text of one line of `path`, without its terminator.
fn line_of(path: &Path, line: u32) -> Result<String, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|err| format!("{} cannot be read: {err}", path.display()))?;
    text.split('\n')
        .nth(line as usize)
        .map(|line| line.trim_end_matches('\r').to_string())
        .ok_or_else(|| format!("{} has no line {}", path.display(), line + 1))
}

/// The zero-based byte offset in line `line` of `path` that the zero-based UTF-16 column `column`
/// names — the tools speak LSP's UTF-16 columns, the index bytes.
fn byte_column(path: &Path, line: u32, column: u32) -> Result<u32, String> {
    let text = line_of(path, line)?;
    let mut units = 0;
    for (offset, character) in text.char_indices() {
        if units >= column {
            return u32::try_from(offset).map_err(|err| err.to_string());
        }
        units += u32::try_from(character.len_utf16()).map_err(|err| err.to_string())?;
    }
    // At or past the line's end the column is the line's length, as an editor clamps it.
    u32::try_from(text.len()).map_err(|err| err.to_string())
}

/// Translates the index's one-based byte coordinates into the tools' zero-based UTF-16 ones,
/// reading each file's lines from disk once.
struct Columns<'a> {
    worktree: &'a Path,
    lines: HashMap<(PathBuf, u32), String>,
}

impl<'a> Columns<'a> {
    fn within(worktree: &'a Path) -> Self {
        Self {
            worktree,
            lines: HashMap::new(),
        }
    }

    /// A location the index answered — relative to the worktree unless `outside_root` — as the
    /// tools' location: a `file://` URI and a zero-based range.
    fn lsp_location(&mut self, location: &index::CodeLocation) -> Result<Location, String> {
        let path = if location.outside_root {
            PathBuf::from(&location.file)
        } else {
            self.worktree.join(&location.file)
        };
        Ok(Location {
            uri: format!("file://{}", path.display()),
            range: self.lsp_range(&path, location.range)?,
        })
    }

    fn lsp_range(
        &mut self,
        path: &Path,
        range: Option<index::SourceRange>,
    ) -> Result<Range, String> {
        let range = range.ok_or("the index answered a range without bounds")?;
        Ok(Range {
            start: self.lsp_position(path, range.start)?,
            end: self.lsp_position(path, range.end)?,
        })
    }

    fn lsp_position(
        &mut self,
        path: &Path,
        position: Option<index::SourcePosition>,
    ) -> Result<Position, String> {
        let position = position
            .filter(|at| at.line >= 1 && at.column >= 1)
            .ok_or("the index answered a position below one")?;
        let line = position.line - 1;
        let key = (path.to_path_buf(), line);
        if !self.lines.contains_key(&key) {
            self.lines.insert(key.clone(), line_of(path, line)?);
        }
        let text = &self.lines[&key];
        let prefix = text.get(..(position.column - 1) as usize).ok_or_else(|| {
            format!(
                "{} line {} has no column {}",
                path.display(),
                position.line,
                position.column
            )
        })?;
        let units: usize = prefix.chars().map(char::len_utf16).sum();
        Ok(Position::at(
            line,
            u32::try_from(units).map_err(|err| err.to_string())?,
        ))
    }
}

/// The queried `file` as a path relative to the session's `worktree`, or a refusal when it names
/// anything outside it — an absolute path elsewhere, or a relative one that climbs out with `..`.
///
/// This is the host-side binding: `worktree` is what the host resolved from the session, and
/// nothing reaches the index for a file this refuses.
pub fn bind_to_session_worktree(worktree: &Path, file: &str) -> Result<String, String> {
    let refused = || format!("{file} is outside the session's worktree");
    let relative = match Path::new(file).strip_prefix(worktree) {
        Ok(inside) => inside,
        Err(_) if Path::new(file).is_absolute() => return Err(refused()),
        Err(_) => Path::new(file),
    };
    let mut bound = PathBuf::new();
    for part in relative.components() {
        match part {
            Component::Normal(name) => bound.push(name),
            Component::CurDir => {}
            _ => return Err(refused()),
        }
    }
    // Nothing left of `""` or `.`: the worktree itself, which is no file.
    if bound.as_os_str().is_empty() {
        return Err(refused());
    }
    Ok(bound.to_string_lossy().into_owned())
}

/// The executor a host registers: the index-backed one when it manages an index, `existing`
/// otherwise. A deployment switch, not a fallback — with an index, `existing` is never asked.
#[must_use]
pub fn select_lsp_executor(
    index: Option<Arc<dyn IndexChannel>>,
    existing: Arc<dyn LspExecutor>,
) -> Arc<dyn LspExecutor> {
    match index {
        Some(index) => Arc::new(IndexLspExecutor::new(index)),
        None => existing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_relative_file_inside_the_worktree_is_bound_to_it() {
        // Given a session worktree
        let worktree = Path::new("/sessions/one/worktree");

        // When a file inside it is bound
        let bound = bind_to_session_worktree(worktree, "src/lib.rs");

        // Then it is that file, relative to the worktree
        assert_eq!(bound, Ok("src/lib.rs".to_string()));
    }

    #[test]
    fn an_absolute_file_inside_the_worktree_is_made_relative_to_it() {
        // Given a session worktree
        let worktree = Path::new("/sessions/one/worktree");

        // When a file is named by its absolute path inside it
        let bound = bind_to_session_worktree(worktree, "/sessions/one/worktree/src/main.rs");

        // Then it is bound relative to the worktree
        assert_eq!(bound, Ok("src/main.rs".to_string()));
    }

    #[test]
    fn a_file_that_climbs_out_of_the_worktree_is_refused() {
        // Given a session worktree
        let worktree = Path::new("/sessions/one/worktree");

        // When a file climbs into a neighbouring session's worktree
        let bound = bind_to_session_worktree(worktree, "../../two/worktree/src/lib.rs");

        // Then it is refused, naming the file
        assert_eq!(
            bound,
            Err("../../two/worktree/src/lib.rs is outside the session's worktree".to_string())
        );
    }

    #[test]
    fn an_absolute_file_in_another_worktree_is_refused() {
        // Given a session worktree
        let worktree = Path::new("/sessions/one/worktree");

        // When a file is named by its absolute path in another session's worktree
        let bound = bind_to_session_worktree(worktree, "/sessions/two/worktree/src/lib.rs");

        // Then it is refused, naming the file
        assert_eq!(
            bound,
            Err("/sessions/two/worktree/src/lib.rs is outside the session's worktree".to_string())
        );
    }

    #[test]
    fn an_empty_file_is_refused() {
        // Given a session worktree
        let worktree = Path::new("/sessions/one/worktree");

        // When an empty file name is bound
        let bound = bind_to_session_worktree(worktree, "");

        // Then it is refused
        assert_eq!(bound, Err(" is outside the session's worktree".to_string()));
    }

    #[test]
    fn the_worktree_itself_named_as_a_dot_is_refused() {
        // Given a session worktree
        let worktree = Path::new("/sessions/one/worktree");

        // When the worktree's own directory is named
        let bound = bind_to_session_worktree(worktree, ".");

        // Then it is refused, because it is no file
        assert_eq!(
            bound,
            Err(". is outside the session's worktree".to_string())
        );
    }

    #[test]
    fn a_sibling_directory_sharing_the_worktrees_name_as_a_prefix_is_refused() {
        // Given a session worktree and a sibling whose name starts with the worktree's
        let worktree = Path::new("/sessions/one/worktree");

        // When a file in that sibling is bound
        let bound = bind_to_session_worktree(worktree, "/sessions/one/worktree-evil/src/lib.rs");

        // Then it is refused, naming the file
        assert_eq!(
            bound,
            Err(
                "/sessions/one/worktree-evil/src/lib.rs is outside the session's worktree"
                    .to_string()
            )
        );
    }

    #[test]
    fn a_file_named_with_a_leading_current_directory_is_bound_without_it() {
        // Given a session worktree
        let worktree = Path::new("/sessions/one/worktree");

        // When a file is named relative to the current directory
        let bound = bind_to_session_worktree(worktree, "./src/lib.rs");

        // Then it is bound to the file inside the worktree
        assert_eq!(bound, Ok("src/lib.rs".to_string()));
    }
}
