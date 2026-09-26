use crate::{edit::Position, Range};

use crate::edit::TextEdit;

/// The span of the `mod <module>;` line in a crate root, newline included.
pub(crate) fn module_declaration(text: &str, module: &str) -> Option<std::ops::Range<usize>> {
    let mut offset = 0usize;
    for line in text.split_inclusive('\n') {
        let start = offset;
        offset += line.len();

        let declaration = line.trim();
        let declaration = declaration.strip_prefix("pub ").unwrap_or(declaration);
        if declaration == format!("mod {module};") {
            return Some(start..offset);
        }
    }
    None
}

/// Where a new `mod` declaration goes in a crate root: after the last one already there, and after
/// the file's own header when it declares none.
///
/// Placed rather than sorted in, because a crate root's `mod` order is the author's and nothing
/// here knows what it means.
/// The edit that declares `line` (`pub mod host_registry;`) among the root's existing `mod` lines
/// in sorted position, rather than after the last of them.
#[allow(dead_code)] // TODO(move-facades): replaces `after_last_module_declaration` at its callers.
pub(crate) fn insert_module_declaration_sorted(text: &str, line: &str) -> TextEdit {
    // TODO(move-facades): implement
    let _ = (text, line);
    todo!("move-facades: declare a module in sorted position")
}

pub(crate) fn after_last_module_declaration(text: &str) -> usize {
    let mut offset = 0usize;
    let mut header_ends = 0usize;
    let mut in_header = true;
    let mut last_declaration = None;

    for line in text.split_inclusive('\n') {
        offset += line.len();
        let trimmed = line.trim();

        if in_header
            && (trimmed.is_empty() || trimmed.starts_with("//!") || trimmed.starts_with("#!"))
        {
            header_ends = offset;
        } else {
            in_header = false;
        }

        let declaration = trimmed.strip_prefix("pub ").unwrap_or(trimmed);
        if declaration.starts_with("mod ") && declaration.ends_with(';') {
            last_declaration = Some(offset);
        }
    }

    last_declaration.unwrap_or(header_ends)
}

/// Which dependency table of a manifest an edit reads or writes.
///
/// A module arrives in its destination's `src/`, so what it names is a `[dependencies]` entry; a
/// test binary arrives in its `tests/`, which cargo compiles against `[dev-dependencies]` as well.
/// The distinction belongs to the manifest rather than to either operation, so the helpers take it
/// instead of each growing a copy shaped for its own table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Table {
    Dependencies,
    DevDependencies,
}

impl Table {
    /// The table header, as it stands on its own line in a manifest.
    fn header(self) -> &'static str {
        match self {
            Table::Dependencies => "[dependencies]",
            Table::DevDependencies => "[dev-dependencies]",
        }
    }
}

/// Whether a manifest already declares a dependency on the crate with this extern name.
pub(crate) fn declares_dependency(manifest: &str, table: Table, extern_name: &str) -> bool {
    dependency_line(manifest, table, extern_name).is_some()
}

/// The line declaring one dependency, verbatim, out of a manifest that has it.
pub(crate) fn dependency_line(manifest: &str, table: Table, extern_name: &str) -> Option<String> {
    dependencies_of(manifest, table)
        .into_iter()
        .find(|line| {
            line.split('=')
                .next()
                .map(|key| key.trim().replace('-', "_") == extern_name)
                .unwrap_or_default()
        })
        .map(str::to_string)
}

/// Whether a manifest declares a dependency on this crate in **either** of its tables.
///
/// The question a test binary asks, because cargo compiles one against `[dependencies]` and
/// `[dev-dependencies]` alike: a crate declared in either is already reachable from the test, and
/// declaring it a second time would be a second version to keep in step.
pub(crate) fn declares_dependency_in_either_table(manifest: &str, extern_name: &str) -> bool {
    dependency_line_from_either_table(manifest, extern_name).is_some()
}

/// The line declaring one dependency, verbatim, out of **either** of a manifest's tables.
///
/// `[dependencies]` is asked first and answers alone where it has the crate: a manifest declaring
/// the same crate in both tables is declaring one dependency twice, and the ordinary table is the
/// one a reader means by it.
pub(crate) fn dependency_line_from_either_table(
    manifest: &str,
    extern_name: &str,
) -> Option<String> {
    dependency_line(manifest, Table::Dependencies, extern_name)
        .or_else(|| dependency_line(manifest, Table::DevDependencies, extern_name))
}

/// The lines of one of a manifest's dependency tables.
fn dependencies_of(manifest: &str, table: Table) -> Vec<&str> {
    manifest
        .lines()
        .map(str::trim_end)
        .skip_while(|line| line.trim() != table.header())
        .skip(1)
        .take_while(|line| !line.trim_start().starts_with('['))
        .filter(|line| line.contains('='))
        .collect()
}

/// The manifest with `lines` added to one of its dependency tables.
pub(crate) fn with_dependencies(manifest: &str, table: Table, lines: &[String]) -> Vec<TextEdit> {
    if lines.is_empty() {
        return Vec::new();
    }

    let added = lines.join("\n") + "\n";
    match end_of_dependencies(manifest, table) {
        Some(at) => vec![replacement(manifest, at..at, &added)],
        // A manifest without the table gains it, at the end where a table cannot land inside
        // another one.
        None => vec![replacement(
            manifest,
            manifest.len()..manifest.len(),
            &format!("\n{}\n{added}", table.header()),
        )],
    }
}

/// Where one of a manifest's dependency tables ends, or `None` when it declares none.
fn end_of_dependencies(manifest: &str, table: Table) -> Option<usize> {
    let mut offset = 0usize;
    let mut found = None;
    let mut inside = false;

    for line in manifest.split_inclusive('\n') {
        offset += line.len();

        let trimmed = line.trim();
        if trimmed == table.header() {
            inside = true;
            found = Some(offset);
            continue;
        }
        if inside {
            if trimmed.starts_with('[') {
                return found;
            }
            if !trimmed.is_empty() {
                found = Some(offset);
            }
        }
    }
    found
}

/// The span of a workspace manifest's `members` array, between its brackets.
pub(crate) fn members_list(manifest: &str) -> Option<std::ops::Range<usize>> {
    let opened = manifest.find("members")?;
    let start = manifest[opened..].find('[')? + opened + 1;
    let end = manifest[start..].find(']')? + start;
    Some(start..end)
}

/// A copied dependency line, with a relative `path` re-anchored on the crate receiving it.
///
/// `path = "../tddy-lsp"` means different crates read from different directories. Copying the line
/// verbatim is how a manifest ends up pointing at a crate that is not the one it was copied from —
/// or at nothing, which at least fails loudly.
pub(crate) fn re_anchored(declared: &str, from: &str, to: &str) -> String {
    let Some(path) = declared_path(declared) else {
        return declared.to_string();
    };

    let target = normalized(&format!("{from}/{path}"));
    declared.replacen(
        &format!("path = \"{path}\""),
        &format!("path = \"{}\"", relative_from(to, &target)),
        1,
    )
}

/// The directory a dependency line reaches by `path`, relative to the crate declaring it.
///
/// `None` when the line declares no path — a registry dependency is the same crate wherever it is
/// read from — and when it declares an absolute one, which is already the same directory for every
/// reader and so is nothing to re-anchor or to follow.
pub(crate) fn declared_path(declared: &str) -> Option<&str> {
    let (_, rest) = declared.split_once("path = \"")?;
    let (path, _) = rest.split_once('"')?;
    (!path.starts_with('/')).then_some(path)
}

/// A slash-separated path with its `.` and `..` components resolved.
pub(crate) fn normalized(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            part => parts.push(part),
        }
    }
    parts.join("/")
}

/// The path from one directory to another, both relative to the repository root.
pub(crate) fn relative_from(from: &str, to: &str) -> String {
    let from: Vec<&str> = from.split('/').filter(|part| !part.is_empty()).collect();
    let to: Vec<&str> = to.split('/').filter(|part| !part.is_empty()).collect();
    let shared = from
        .iter()
        .zip(&to)
        .take_while(|(left, right)| left == right)
        .count();

    let up = std::iter::repeat_n("..", from.len() - shared);
    up.chain(to[shared..].iter().copied())
        .collect::<Vec<_>>()
        .join("/")
}

/// A replacement of one byte span, in the coordinates [`crate::apply`] reads edits back in.
pub(crate) fn replacement(text: &str, span: std::ops::Range<usize>, new_text: &str) -> TextEdit {
    TextEdit {
        range: Range {
            start: position_of(text, span.start),
            end: position_of(text, span.end),
        },
        new_text: new_text.to_string(),
    }
}

/// The one-based line/column a byte offset sits at.
pub(crate) fn position_of(text: &str, offset: usize) -> Position {
    let before = &text[..offset];
    Position {
        line: before.matches('\n').count() as u32 + 1,
        col: before
            .rsplit('\n')
            .next()
            .map_or(0, |line| line.chars().count()) as u32
            + 1,
    }
}

#[cfg(test)]
mod sorted_declaration_tests {
    use super::*;
    use crate::apply::edited;

    fn apply_text_edits(text: &str, edits: &[TextEdit]) -> String {
        edited(text.to_string(), edits).unwrap()
    }

    #[test]
    fn a_module_is_declared_between_the_ones_it_sorts_between() {
        // Given a root declaring `alpha` and `zeta`
        let root = "//! The crate.\n\npub mod alpha;\npub mod zeta;\n";

        // When `host_registry` is declared
        let edit = insert_module_declaration_sorted(root, "pub mod host_registry;");

        // Then it lands between them
        assert_eq!(
            apply_text_edits(root, &[edit]),
            "//! The crate.\n\npub mod alpha;\npub mod host_registry;\npub mod zeta;\n"
        );
    }

    #[test]
    fn a_module_that_sorts_last_is_declared_after_the_last() {
        let root = "pub mod alpha;\npub mod beta;\n";

        let edit = insert_module_declaration_sorted(root, "pub mod gamma;");

        assert_eq!(
            apply_text_edits(root, &[edit]),
            "pub mod alpha;\npub mod beta;\npub mod gamma;\n"
        );
    }
}
