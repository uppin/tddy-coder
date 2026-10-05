//! The files a module spans: its own, and every file its `mod` declarations lead to below it.
//!
//! A Rust 2018 module `a` is `a.rs` or `a/mod.rs`, and the files of its children sit in the
//! directory `a/` either way. Moving the module means moving all of them, which is the part a move
//! of the module's own file alone leaves behind. Read from the text of the tree (through the
//! workspace, so an earlier operation's pending edits are seen), with no server.

use std::path::{Path, PathBuf};

use super::malformed;
use super::source_scan::items_of_module;
use crate::registry::Workspace;

use super::Result;

/// The directory the children of the module whose file is `file` live in.
///
/// A crate root and a `mod.rs` own the directory they are in; any other file `a.rs` owns `a/`.
pub(crate) fn children_directory(file: &str) -> PathBuf {
    let path = Path::new(file);
    let directory = path.parent().unwrap_or_else(|| Path::new(""));
    match path.file_stem().and_then(|stem| stem.to_str()) {
        Some("lib" | "main" | "mod") | None => directory.to_path_buf(),
        Some(stem) => directory.join(stem),
    }
}

/// Where `mod name;` declared in a module whose children live in `directory` is on disk: the file
/// that exists of `<name>.rs` and `<name>/mod.rs`, with its text.
pub(crate) fn file_of_child(
    workspace: &Workspace<'_>,
    directory: &Path,
    name: &str,
) -> Option<(String, String)> {
    [
        directory.join(format!("{name}.rs")),
        directory.join(name).join("mod.rs"),
    ]
    .iter()
    .find_map(|candidate| {
        let relative = candidate.to_string_lossy().to_string();
        workspace.read(&relative).ok().map(|text| (relative, text))
    })
}

/// `file` and every file of the modules below it, in the order a walk of the declarations meets
/// them. Inline modules are followed too: `mod a { mod b; }` leads to `<directory>/a/b.rs`.
///
/// Refused when a declaration leads to no file, which is a tree that does not compile as it stands
/// and so has nothing sound to move.
pub(crate) fn files_of(workspace: &Workspace<'_>, file: &str) -> Result<Vec<String>> {
    let text = workspace.read(file)?;
    let mut files = vec![file.to_string()];
    below(
        workspace,
        &text,
        0..text.len(),
        &children_directory(file),
        &mut files,
    )?;
    Ok(files)
}

fn below(
    workspace: &Workspace<'_>,
    text: &str,
    scope: std::ops::Range<usize>,
    directory: &Path,
    files: &mut Vec<String>,
) -> Result<()> {
    for child in items_of_module(&text[scope.clone()]).children {
        if let Some(body) = child.body {
            let inside = scope.start + body.start..scope.start + body.end;
            below(workspace, text, inside, &directory.join(&child.name), files)?;
            continue;
        }
        let (file, child_text) =
            file_of_child(workspace, directory, &child.name).ok_or_else(|| {
                malformed(format!(
                    "`mod {};` leads to neither `{}.rs` nor `{}/mod.rs` in {}",
                    child.name,
                    child.name,
                    child.name,
                    directory.display()
                ))
            })?;
        let child_directory = children_directory(&file);
        files.push(file);
        below(
            workspace,
            &child_text,
            0..child_text.len(),
            &child_directory,
            files,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay::Overlay;

    /// A tree under a temporary root holding `files`.
    fn a_tree_holding(files: &[(&str, &str)]) -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        for (path, text) in files {
            let absolute = root.path().join(path);
            std::fs::create_dir_all(absolute.parent().unwrap()).unwrap();
            std::fs::write(absolute, text).unwrap();
        }
        root
    }

    fn files_below(root: &tempfile::TempDir, file: &str) -> Result<Vec<String>> {
        let overlay = Overlay::new();
        let workspace = Workspace {
            root: root.path(),
            overlay: &overlay,
        };
        files_of(&workspace, file)
    }

    #[test]
    fn gives_a_module_file_the_directory_of_its_name_and_a_root_its_own() {
        assert_eq!(children_directory("src/a.rs"), Path::new("src/a"));
        assert_eq!(children_directory("src/a/mod.rs"), Path::new("src/a"));
        assert_eq!(children_directory("src/lib.rs"), Path::new("src"));
    }

    #[test]
    fn finds_the_children_of_both_shapes_and_the_ones_below_an_inline_module() {
        let root = a_tree_holding(&[
            ("src/a.rs", "mod b;\nmod c;\nmod inline {\n    mod d;\n}\n"),
            ("src/a/b.rs", ""),
            ("src/a/c/mod.rs", "mod e;\n"),
            ("src/a/c/e.rs", ""),
            ("src/a/inline/d.rs", ""),
        ]);

        assert_eq!(
            files_below(&root, "src/a.rs").unwrap(),
            [
                "src/a.rs",
                "src/a/b.rs",
                "src/a/c/mod.rs",
                "src/a/c/e.rs",
                "src/a/inline/d.rs"
            ]
        );
    }

    #[test]
    fn refuses_a_declaration_that_leads_to_no_file() {
        let root = a_tree_holding(&[("src/a.rs", "mod missing;\n")]);

        let refusal = files_below(&root, "src/a.rs").unwrap_err().to_string();

        assert!(refusal.contains("missing"), "{refusal}");
    }
}
