use crate::crate_move::manifest_edits::declares_dependency_in_either_table;
use crate::crate_move::manifest_edits::end_of_table;
use crate::crate_move::manifest_edits::replacement;
use crate::edit::TextEdit;

/// One declaration of a dependency in a target-specific table: the table's header verbatim
/// (`[target.'cfg(unix)'.dependencies]`) and the declaring line verbatim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DeclaredIn {
    pub(crate) header: String,
    pub(crate) line: String,
}

/// Every `[target.'<cfg>'.dependencies]` table — `[target.'<cfg>'.dev-dependencies]` when `dev` —
/// that declares the crate with this extern name, each with its declaring line.
///
/// Headers are matched as written, quotes included, so the table a line is carried to is spelled
/// exactly as its origin spelled it.
pub(crate) fn target_declarations(manifest: &str, extern_name: &str, dev: bool) -> Vec<DeclaredIn> {
    let suffix = if dev {
        ".dev-dependencies]"
    } else {
        ".dependencies]"
    };
    let mut header: Option<&str> = None;
    let mut declared = Vec::new();

    for line in manifest.lines().map(str::trim_end) {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            header =
                (trimmed.starts_with("[target.") && trimmed.ends_with(suffix)).then_some(trimmed);
            continue;
        }
        if let Some(header) = header.filter(|_| names_dependency(line, extern_name)) {
            declared.push(DeclaredIn {
                header: header.to_string(),
                line: line.to_string(),
            });
        }
    }
    declared
}

/// Whether a table line declares the crate with this extern name.
fn names_dependency(line: &str, extern_name: &str) -> bool {
    line.split_once('=')
        .is_some_and(|(key, _)| key.trim().replace('-', "_") == extern_name)
}

/// Whether a manifest declares the crate in any dependency table: plain, dev, or target-specific.
pub(crate) fn declares_dependency_in_any_table(manifest: &str, extern_name: &str) -> bool {
    declares_dependency_in_either_table(manifest, extern_name)
        || [false, true]
            .into_iter()
            .any(|dev| !target_declarations(manifest, extern_name, dev).is_empty())
}

/// The manifest with `lines` appended to the table whose header is `header` verbatim, or with that
/// table appended at the end when the manifest has none.
pub(crate) fn with_lines_under(manifest: &str, header: &str, lines: &[String]) -> Vec<TextEdit> {
    if lines.is_empty() {
        return Vec::new();
    }

    let added = lines.join("\n") + "\n";
    match end_of_table(manifest, header) {
        Some(at) => vec![replacement(manifest, at..at, &added)],
        // A manifest without the table gains it, at the end where a table cannot land inside
        // another one.
        None => vec![replacement(
            manifest,
            manifest.len()..manifest.len(),
            &format!("\n{header}\n{added}"),
        )],
    }
}
