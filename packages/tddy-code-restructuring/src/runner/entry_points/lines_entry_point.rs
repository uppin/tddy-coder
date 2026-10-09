//! `restructure lines <file>...`: the production lines of each named file, by the repository's one
//! count ([`crate::production_lines_of_file`]).
//!
//! Answered in process from the files alone — no language server, no daemon, no journal and no
//! process started — so `/pr-wrap`'s file-length gate can call it for a working-tree file and for a
//! merge-base blob alike.

use std::path::Path;

use super::super::{FileLines, Options};
use crate::Result;

/// Measure every file `options.files` names, relative to `root`, in the order they were named.
///
/// A file that cannot be read is refused by its path, and nothing is reported for the others: a
/// gate that compared a partial answer would pass vacuously on the file it could not see.
pub fn lines(root: &Path, options: &Options) -> Result<Vec<FileLines>> {
    // TODO(reshape-oversized-files): implement
    let _ = (root, options);
    todo!("restructure lines: measure each named file with production_lines_of_file")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn a_tree_holding(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("a temp dir");
        for (path, text) in files {
            std::fs::write(dir.path().join(path), text).expect("the fixture is written");
        }
        dir
    }

    fn measuring(files: &[&str]) -> Options {
        Options {
            command: super::super::super::Command::Lines,
            files: files.iter().map(PathBuf::from).collect(),
            ..Options::default()
        }
    }

    fn a_file_of(path: &str, lines: usize) -> FileLines {
        FileLines {
            path: path.to_string(),
            lines,
        }
    }

    /// What the gate reads: one count per file, in the order it asked, Rust counted past its
    /// out-of-line test module and anything else counted whole.
    #[test]
    fn prints_the_production_lines_of_each_file_in_argument_order() {
        // Given a Rust file whose extracted test module is declared above two production items,
        // and a TypeScript file of three lines
        let tree = a_tree_holding(&[
            (
                "lib.rs",
                "mod parts;\n#[cfg(test)]\nmod lib_tests;\nfn a() {}\nfn b() {}\n",
            ),
            ("widget.ts", "a\nb\nc\n"),
        ]);

        // When both are measured, the TypeScript file named first
        let measured = lines(tree.path(), &measuring(&["widget.ts", "lib.rs"])).expect("measured");

        // Then
        assert_eq!(
            measured,
            [a_file_of("widget.ts", 3), a_file_of("lib.rs", 3)]
        );
    }

    /// A file the gate cannot read is a refusal naming it, never a zero.
    #[test]
    fn refuses_a_file_it_cannot_read_naming_it() {
        // Given a tree with one readable file and no `missing.rs`
        let tree = a_tree_holding(&[("lib.rs", "fn a() {}\n")]);

        // When both are measured
        let refusal = lines(tree.path(), &measuring(&["lib.rs", "missing.rs"]))
            .expect_err("an unreadable file is refused");

        // Then the refusal names the file it could not read
        assert!(refusal.to_string().contains("missing.rs"), "{refusal}");
    }
}
