//! Go-to-definition, find-references and hover, answered by a root's warm language server.
//!
//! Each answer is one `tddy_lsp::LspClient` query (`definition`, `references`, `hover`) on the
//! client [`WorkspaceIndex::client_for`] hands out for the request's root. What this module owns is
//! the translation either side of that query: the service's one-based byte `SourcePosition` into the
//! server's zero-based position, and each answered location back into a path relative to the root —
//! or an absolute path marked `outside_root` when the server points outside it.

use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use tddy_lsp::{Location, LspClient, Position};
use tddy_rpc::Status;

use crate::activity::Activity;
use crate::index::WorkspaceIndex;
use crate::proto::code_index::{
    CodeLocation, DefinitionRequest, DefinitionResponse, HoverRequest, HoverResponse,
    ReferencesRequest, ReferencesResponse, SourcePosition, SourceRange,
};
use crate::status::status_of_lsp;

/// The language id a document of this host's one language server is announced under.
const LANGUAGE_ID: &str = "rust";

/// Where the symbol at the request's position is defined.
pub(crate) async fn serve_definition(
    index: &WorkspaceIndex,
    request: DefinitionRequest,
) -> Result<DefinitionResponse, Status> {
    let (activity, root) = Activity::arrived("definition", index, &request.workspace_root).await?;
    let outcome = locations_at(
        index,
        &root,
        &request.file,
        request.position,
        Query::Definition,
    )
    .await
    .map(|locations| DefinitionResponse { locations });
    activity.recorded(outcome)
}

/// Every reference to the symbol at the request's position.
pub(crate) async fn serve_references(
    index: &WorkspaceIndex,
    request: ReferencesRequest,
) -> Result<ReferencesResponse, Status> {
    let (activity, root) = Activity::arrived("references", index, &request.workspace_root).await?;
    let outcome = locations_at(
        index,
        &root,
        &request.file,
        request.position,
        Query::References,
    )
    .await
    .map(|locations| ReferencesResponse { locations });
    activity.recorded(outcome)
}

/// The hover text of the symbol at the request's position.
pub(crate) async fn serve_hover(
    index: &WorkspaceIndex,
    request: HoverRequest,
) -> Result<HoverResponse, Status> {
    let (activity, root) = Activity::arrived("hover", index, &request.workspace_root).await?;
    let outcome = hover_at(index, &root, &request.file, request.position).await;
    activity.recorded(outcome)
}

/// Which location-answering query to put to the server.
#[derive(Clone, Copy)]
enum Query {
    Definition,
    References,
}

async fn locations_at(
    index: &WorkspaceIndex,
    root: &Path,
    file: &str,
    position: Option<SourcePosition>,
    query: Query,
) -> Result<Vec<CodeLocation>, Status> {
    let asked = asked_document(index, root, file, position).await?;
    let answered = match query {
        Query::Definition => asked.client.definition(&asked.uri, asked.at).await,
        Query::References => asked.client.references(&asked.uri, asked.at).await,
    }
    .map_err(|failure| status_of_lsp(&failure))?;
    answered
        .iter()
        .map(|location| code_location(root, location))
        .collect()
}

async fn hover_at(
    index: &WorkspaceIndex,
    root: &Path,
    file: &str,
    position: Option<SourcePosition>,
) -> Result<HoverResponse, Status> {
    let asked = asked_document(index, root, file, position).await?;
    let markdown = asked
        .client
        .hover(&asked.uri, asked.at)
        .await
        .map_err(|failure| status_of_lsp(&failure))?;
    Ok(HoverResponse { markdown })
}

/// What a navigation query is put to: the root's warm server, which has been told the file's
/// current contents, the file's URI and the position in the server's coordinates.
struct AskedDocument {
    client: Arc<LspClient>,
    uri: String,
    at: Position,
}

/// Validate the request's file and position, then bring the server up to date on that file so its
/// answer is about what the caller read.
async fn asked_document(
    index: &WorkspaceIndex,
    root: &Path,
    file: &str,
    position: Option<SourcePosition>,
) -> Result<AskedDocument, Status> {
    let path = file_within(root, file)?;
    let position =
        position.ok_or_else(|| Status::invalid_argument("the request names no position"))?;
    let at = server_position(position)?;
    let text = std::fs::read_to_string(&path)
        .map_err(|err| Status::not_found(format!("`{file}` cannot be read: {err}")))?;
    let uri = format!("file://{}", path.display());
    let client = index.client_for(root).await?;
    client
        .sync_document(&uri, LANGUAGE_ID, &text)
        .await
        .map_err(|failure| status_of_lsp(&failure))?;
    Ok(AskedDocument { client, uri, at })
}

/// `file` resolved against `root`, refused when it is empty, absolute or climbs out of the root.
fn file_within(root: &Path, file: &str) -> Result<PathBuf, Status> {
    if file.trim().is_empty() {
        return Err(Status::invalid_argument("the request names no file"));
    }
    let relative = Path::new(file);
    let stays_inside = relative
        .components()
        .all(|part| matches!(part, Component::Normal(_) | Component::CurDir));
    if !stays_inside {
        return Err(Status::invalid_argument(format!(
            "file `{file}` must be a path inside workspace_root, without `..` or a root"
        )));
    }
    Ok(root.join(relative))
}

/// One-based line and byte column to the server's zero-based position.
///
/// The server speaks bytes, not UTF-16 units: the client negotiates `utf-8` position encoding and
/// refuses a server that does not answer in it, so a column converts by its offset alone.
fn server_position(position: SourcePosition) -> Result<Position, Status> {
    if position.line == 0 || position.column == 0 {
        return Err(Status::invalid_argument(
            "positions are one-based: line and column must be at least 1",
        ));
    }
    Ok(Position::at(position.line - 1, position.column - 1))
}

fn wire_position(position: Position) -> SourcePosition {
    SourcePosition {
        line: position.line + 1,
        column: position.character + 1,
    }
}

/// A server location as the wire carries it: relative to `root` when inside it, the absolute path
/// marked `outside_root` otherwise.
fn code_location(root: &Path, location: &Location) -> Result<CodeLocation, Status> {
    let path = path_of_uri(&location.uri)?;
    let range = Some(SourceRange {
        start: Some(wire_position(location.range.start)),
        end: Some(wire_position(location.range.end)),
    });
    Ok(match path.strip_prefix(root) {
        Ok(inside) => CodeLocation {
            file: inside.to_string_lossy().into_owned(),
            range,
            outside_root: false,
        },
        Err(_) => CodeLocation {
            file: path.to_string_lossy().into_owned(),
            range,
            outside_root: true,
        },
    })
}

/// The filesystem path a `file://` URI names, its percent-escapes decoded.
fn path_of_uri(uri: &str) -> Result<PathBuf, Status> {
    let escaped = uri.strip_prefix("file://").ok_or_else(|| {
        Status::internal(format!(
            "the language server answered a non-file URI `{uri}`"
        ))
    })?;
    let bytes = escaped.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%' {
            let digits = escaped
                .get(at + 1..at + 3)
                .and_then(|pair| u8::from_str_radix(pair, 16).ok())
                .ok_or_else(|| {
                    Status::internal(format!(
                        "the language server answered a malformed URI `{uri}`"
                    ))
                })?;
            decoded.push(digits);
            at += 3;
        } else {
            decoded.push(bytes[at]);
            at += 1;
        }
    }
    String::from_utf8(decoded).map(PathBuf::from).map_err(|_| {
        Status::internal(format!(
            "the language server answered a non-UTF-8 URI `{uri}`"
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_location(uri: &str) -> Location {
        Location {
            uri: uri.to_string(),
            range: tddy_lsp::client::Range {
                start: Position::at(10, 0),
                end: Position::at(10, 3),
            },
        }
    }

    #[test]
    fn a_file_climbing_out_of_the_root_is_refused() {
        let refusal = file_within(Path::new("/work"), "../secret.rs").expect_err("traversal");

        assert_eq!(refusal.code, tddy_rpc::Code::InvalidArgument);
    }

    #[test]
    fn an_absolute_file_is_refused() {
        let refusal = file_within(Path::new("/work"), "/etc/passwd").expect_err("absolute");

        assert_eq!(refusal.code, tddy_rpc::Code::InvalidArgument);
    }

    #[test]
    fn a_file_inside_the_root_resolves_under_it() {
        assert_eq!(
            file_within(Path::new("/work"), "src/lib.rs").expect("inside"),
            PathBuf::from("/work/src/lib.rs")
        );
    }

    #[test]
    fn a_location_inside_the_root_is_relative_and_one_based() {
        let located = code_location(Path::new("/work"), &a_location("file:///work/src/lib.rs"))
            .expect("a file uri");

        assert_eq!(
            (located.file.as_str(), located.outside_root),
            ("src/lib.rs", false)
        );
        assert_eq!(
            located.range.and_then(|range| range.start),
            Some(SourcePosition {
                line: 11,
                column: 1
            })
        );
    }

    #[test]
    fn a_location_outside_the_root_is_absolute_and_marked() {
        let located = code_location(
            Path::new("/work"),
            &a_location("file:///home/u/.cargo/registry/my%20crate/lib.rs"),
        )
        .expect("a file uri");

        assert_eq!(
            (located.file.as_str(), located.outside_root),
            ("/home/u/.cargo/registry/my crate/lib.rs", true)
        );
    }

    #[test]
    fn a_position_below_one_is_refused() {
        let refusal = server_position(SourcePosition { line: 1, column: 0 }).expect_err("zero");

        assert_eq!(refusal.code, tddy_rpc::Code::InvalidArgument);
    }
}
