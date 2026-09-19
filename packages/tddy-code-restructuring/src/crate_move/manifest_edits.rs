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

/// Whether a manifest already declares a dependency on the crate with this extern name.
pub(crate) fn declares_dependency(manifest: &str, extern_name: &str) -> bool {
    dependency_line(manifest, extern_name).is_some()
}

/// The line declaring one dependency, verbatim, out of a manifest that has it.
pub(crate) fn dependency_line(manifest: &str, extern_name: &str) -> Option<String> {
    dependencies_of(manifest)
        .into_iter()
        .find(|line| {
            line.split('=')
                .next()
                .map(|key| key.trim().replace('-', "_") == extern_name)
                .unwrap_or_default()
        })
        .map(str::to_string)
}

/// The lines of a manifest's `[dependencies]` table.
fn dependencies_of(manifest: &str) -> Vec<&str> {
    manifest
        .lines()
        .map(str::trim_end)
        .skip_while(|line| line.trim() != "[dependencies]")
        .skip(1)
        .take_while(|line| !line.trim_start().starts_with('['))
        .filter(|line| line.contains('='))
        .collect()
}

/// The manifest with `lines` added to its `[dependencies]` table.
pub(crate) fn with_dependencies(manifest: &str, lines: &[String]) -> Vec<TextEdit> {
    if lines.is_empty() {
        return Vec::new();
    }

    let added = lines.join("\n") + "\n";
    match end_of_dependencies(manifest) {
        Some(at) => vec![replacement(manifest, at..at, &added)],
        // A manifest with no `[dependencies]` gains the table the moved code needs, at the end
        // where a table cannot land inside another one.
        None => vec![replacement(
            manifest,
            manifest.len()..manifest.len(),
            &format!("\n[dependencies]\n{added}"),
        )],
    }
}

/// Where a manifest's `[dependencies]` table ends, or `None` when it declares none.
fn end_of_dependencies(manifest: &str) -> Option<usize> {
    let mut offset = 0usize;
    let mut found = None;
    let mut inside = false;

    for line in manifest.split_inclusive('\n') {
        offset += line.len();

        let trimmed = line.trim();
        if trimmed == "[dependencies]" {
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
    let Some((head, rest)) = declared.split_once("path = \"") else {
        return declared.to_string();
    };
    let Some((path, tail)) = rest.split_once('"') else {
        return declared.to_string();
    };
    if path.starts_with('/') {
        return declared.to_string();
    }

    let target = normalized(&format!("{from}/{path}"));
    format!("{head}path = \"{}\"{tail}", relative_from(to, &target))
}

/// A slash-separated path with its `.` and `..` components resolved.
fn normalized(path: &str) -> String {
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
