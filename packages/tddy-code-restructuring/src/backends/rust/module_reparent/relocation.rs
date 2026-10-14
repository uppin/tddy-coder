//! Where the files of a moved module go: each keeps its place relative to the module, under the new
//! parent's directory.

use std::path::{Path, PathBuf};

use super::super::item_move::destination::Module;
use crate::crate_move::module_files::{children_directory, relocated, MovedFile};
use crate::item_anchor::module_path_of;
use crate::Result;

/// The directory the children of `module` live in: the directory of its file's children, then the
/// inline modules the module sits in below that file.
pub(in crate::backends::rust) fn directory_of(root: &Path, module: &Module) -> Result<PathBuf> {
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
    relocated(old, new, name, files)
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
