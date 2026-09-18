use super::Move;

use crate::{
    apply::byte_offset,
    crate_move::{destination, manifest_edits, module_home},
};

use crate::edit::Position;

use super::Result;

use crate::registry::Workspace;

use std::collections::BTreeSet;

use crate::edit::TextEdit;

/// What the moved file's `use` header says about the crates it needs.
pub(crate) struct Header {
    /// The edits that re-point it, empty when it names nothing that moved.
    pub(crate) edits: Vec<TextEdit>,
    /// Every crate the header names once re-pointed, by extern name.
    pub(crate) crates_named: BTreeSet<String>,
    /// The paths that now name the crate the module left, for a refusal that has to list them.
    pub(crate) origin_paths: Vec<String>,
}

/// The moved file's own `use` header, re-pointed at the crate it left.
///
/// Inside the module, `crate::` and a top-level `super::` both named the crate it is leaving; in the
/// destination they would name the destination. Only the qualifier is rewritten, and only in a `use`
/// declaration — see this module's own documentation for why that is the whole of the header pass.
pub(crate) fn repointed_header(
    workspace: &Workspace<'_>,
    text: &str,
    origin: &destination::Destination,
) -> Result<Header> {
    let mut header = Header {
        edits: Vec::new(),
        crates_named: BTreeSet::new(),
        origin_paths: Vec::new(),
    };

    let mut offset = 0usize;
    for line in text.split_inclusive('\n') {
        let start = offset;
        offset += line.len();

        let Some((at, path)) = use_path(line) else {
            continue;
        };
        let (qualifier, rest) = match path.split_once("::") {
            Some(split) => split,
            None => (path, ""),
        };

        if matches!(qualifier, "crate" | "super") {
            let at = start + at;
            let as_origin = format!("{}::{}", origin.extern_name, rest);
            let (written_as, named) =
                match module_home::defining_crate(workspace, origin, &as_origin)? {
                    Some(defining) if defining != origin.extern_name => {
                        (format!("{defining}::{rest}"), defining)
                    }
                    _ => (as_origin.clone(), origin.extern_name.clone()),
                };
            let new_qualifier = written_as
                .split("::")
                .next()
                .unwrap_or(origin.extern_name.as_str());
            header.edits.push(manifest_edits::replacement(
                text,
                at..at + qualifier.len(),
                new_qualifier,
            ));
            header.crates_named.insert(named);
            header.origin_paths.push(written_as);
            continue;
        }
        if !matches!(qualifier, "self" | "std" | "core" | "alloc") {
            header.crates_named.insert(qualifier.to_string());
        }
    }

    Ok(header)
}

/// The path a top-level `use` declaration names, and where on the line it starts.
///
/// An indented `use` belongs to a nested module or a function body; only the file's own header is
/// this operation's to rewrite.
fn use_path(line: &str) -> Option<(usize, &str)> {
    let trimmed = line.trim_end();
    if trimmed.starts_with(char::is_whitespace) {
        return None;
    }
    for keyword in ["pub use ", "use "] {
        if let Some(path) = trimmed.strip_prefix(keyword) {
            return Some((keyword.len(), path.trim_end_matches(';')));
        }
    }
    None
}

/// The path this caller writes, ending at the identifier the server reported.
pub(crate) struct WrittenPath {
    pub(crate) text: String,
    pub(crate) span: std::ops::Range<usize>,
}

/// Read the whole `a::b::C` a reference sits at the end of.
///
/// The server reports where the item's own name is; what has to be replaced is everything leading
/// to it, which is only readable from the caller's text.
pub(crate) fn written_path_at(text: &str, at: Position) -> Result<WrittenPath> {
    let start = byte_offset(text, at.line, at.col)?;
    let end = start
        + text[start..]
            .find(|character: char| !is_path_character(character))
            .unwrap_or(text.len() - start);

    let mut first = start;
    while let Some(head) = text[..first].strip_suffix("::") {
        let segment = head.trim_end_matches(is_path_character);
        if segment.len() == head.len() {
            break;
        }
        first = segment.len();
    }

    Ok(WrittenPath {
        text: text[first..end].to_string(),
        span: first..end,
    })
}

fn is_path_character(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

/// The same path, written from outside the crate the module is leaving.
///
/// `None` when the path never names the module: the caller reaches the item through a name bound
/// somewhere else in its own file, and that binding is a reference of its own.
pub(crate) fn repointed(written: &str, moving: &Move) -> Option<String> {
    let segments: Vec<&str> = written.split("::").collect();
    let at = segments
        .iter()
        .position(|segment| *segment == moving.module)?;
    Some(format!(
        "{}::{}",
        moving.destination.extern_name,
        segments[at..].join("::")
    ))
}
