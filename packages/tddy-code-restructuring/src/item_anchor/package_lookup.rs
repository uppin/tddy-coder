use crate::item_anchor::malformed;
use crate::Result;
use std::path::{Path, PathBuf};

/// The directory (relative to `root`) and `[package] name` of the nearest package that holds `file`.
///
/// A workspace manifest declares no package, so it is walked past rather than mistaken for one.
pub(crate) fn owning_package(root: &Path, file: &str) -> Result<(PathBuf, String)> {
    let mut directory = Path::new(file).parent();
    while let Some(candidate) = directory {
        if let Ok(manifest) = std::fs::read_to_string(root.join(candidate).join("Cargo.toml")) {
            if let Some(name) = crate::crate_move::declared_package_name(&manifest) {
                return Ok((candidate.to_path_buf(), name.to_string()));
            }
        }
        directory = candidate.parent();
    }
    Err(malformed(format!(
        "{file} is in no package: no `Cargo.toml` declaring a `[package]` sits above it{}",
        repo_root_hint(root, file)
    )))
}

/// The sentence naming the repo-root path(s) to write, when `file` exists below some package.
///
/// Every path a plan carries is relative to the repo root; an author standing in a package writes
/// the path relative to it. The path is never resolved for them (that would be a fallback): the
/// refusal only says what to write. Empty when no package holds `file`.
pub(super) fn repo_root_hint(root: &Path, file: &str) -> String {
    let mut candidates = Vec::new();
    collect_package_files(root, Path::new(""), Path::new(file), &mut candidates);
    candidates.sort();
    match candidates.as_slice() {
        [] => String::new(),
        [only] => format!("; write it from the repo root: `{only}`"),
        several => format!(
            "; several packages hold such a file, write it from the repo root as one of: {}",
            several
                .iter()
                .map(|path| format!("`{path}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

/// Every `<package dir>/<file>` that exists below `directory` (relative to `root`), as repo-root paths.
pub(super) fn collect_package_files(
    root: &Path,
    directory: &Path,
    file: &Path,
    found: &mut Vec<String>,
) {
    let Ok(entries) = std::fs::read_dir(root.join(directory)) else {
        return;
    };
    let is_package = std::fs::read_to_string(root.join(directory).join("Cargo.toml"))
        .is_ok_and(|manifest| crate::crate_move::declared_package_name(&manifest).is_some());
    if is_package && root.join(directory).join(file).is_file() {
        found.push(directory.join(file).to_string_lossy().replace('\\', "/"));
    }
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_text = name.to_string_lossy();
        let is_build_or_hidden =
            name_text.starts_with('.') || matches!(&*name_text, "target" | "node_modules");
        if !is_build_or_hidden && entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            collect_package_files(root, &directory.join(&name), file, found);
        }
    }
}
