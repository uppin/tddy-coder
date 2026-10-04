//! Where the files of a moved module go: each keeps its place relative to the module, under the new
//! parent's directory.

use std::path::{Path, PathBuf};

use super::super::item_move::destination::Module;
use crate::crate_move::module_files::children_directory;
use crate::item_anchor::module_path_of;
use crate::Result;

/// One file of the moved module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MovedFile {
    pub(super) from: String,
    pub(super) to: String,
    /// The module this file is, below the moved module (empty for the module's own file).
    pub(super) below: Vec<String>,
}

/// The directory the children of `module` live in: the directory of its file's children, then the
/// inline modules the module sits in below that file.
pub(super) fn directory_of(root: &Path, module: &Module) -> Result<PathBuf> {
    let file_depth = module_path_of(root, &module.file)?.len() - 1;
    Ok(module.path[file_depth..]
        .iter()
        .fold(children_directory(&module.file), |directory, inline| {
            directory.join(inline)
        }))
}

/// The files of the module `name` (`files` as found, the module's own first), moved from the
/// children directory `old` of its old parent to `new` of its new one.
pub(super) fn plan(old: &Path, new: &Path, name: &str, files: &[String]) -> Vec<MovedFile> {
    let (old_stem, new_stem) = (old.join(name), new.join(name));
    let own_file = format!("{}.rs", old_stem.to_string_lossy());
    files
        .iter()
        .filter_map(|file| {
            if *file == own_file {
                return Some(MovedFile {
                    from: file.clone(),
                    to: format!("{}.rs", new_stem.to_string_lossy()),
                    below: Vec::new(),
                });
            }
            let inside = Path::new(file).strip_prefix(&old_stem).ok()?;
            Some(MovedFile {
                from: file.clone(),
                to: new_stem.join(inside).to_string_lossy().to_string(),
                below: module_below(inside),
            })
        })
        .collect()
}

/// The modules between the moved module and the file at `inside` (relative to its directory).
fn module_below(inside: &Path) -> Vec<String> {
    let mut modules: Vec<String> = inside
        .components()
        .map(|part| part.as_os_str().to_string_lossy().to_string())
        .collect();
    let file = modules.pop().unwrap_or_default();
    if file != "mod.rs" {
        modules.push(file.trim_end_matches(".rs").to_string());
    }
    modules
}

#[cfg(test)]
mod tests {
    use super::*;

    fn moved(files: &[&str]) -> Vec<(String, String, Vec<String>)> {
        let files: Vec<String> = files.iter().map(|file| file.to_string()).collect();
        plan(Path::new("src/host"), Path::new("src/split"), "a", &files)
            .into_iter()
            .map(|file| (file.from, file.to, file.below))
            .collect()
    }

    #[test]
    fn moves_a_module_file_and_its_children_keeping_their_places() {
        assert_eq!(
            moved(&["src/host/a.rs", "src/host/a/b.rs", "src/host/a/c/mod.rs"]),
            [
                ("src/host/a.rs".into(), "src/split/a.rs".into(), vec![]),
                (
                    "src/host/a/b.rs".into(),
                    "src/split/a/b.rs".into(),
                    vec!["b".to_string()]
                ),
                (
                    "src/host/a/c/mod.rs".into(),
                    "src/split/a/c/mod.rs".into(),
                    vec!["c".to_string()]
                ),
            ]
        );
    }

    #[test]
    fn moves_a_mod_rs_module_as_a_mod_rs() {
        assert_eq!(
            moved(&["src/host/a/mod.rs"]),
            [(
                "src/host/a/mod.rs".into(),
                "src/split/a/mod.rs".into(),
                vec![]
            )]
        );
    }
}
