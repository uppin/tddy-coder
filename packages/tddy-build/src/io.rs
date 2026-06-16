//! Helpers letting recipe plugins declare cacheable inputs/outputs in open config.

use serde::Deserialize;

use crate::error::BuildError;
use crate::proto::{FileSet, OutputDecl, OutputKind};

/// A declared output in a plugin's open config: `{ path, kind }` where `kind` is
/// `file` (default) or `directory`/`dir`. Paths are repo-root-relative.
#[derive(Debug, Clone, Deserialize)]
pub struct OutputSpec {
    pub path: String,
    #[serde(default = "default_output_kind")]
    pub kind: String,
}

fn default_output_kind() -> String {
    "file".to_string()
}

/// Wrap `srcs` glob patterns into a single input [`FileSet`] rooted at `root`
/// (repo-root-relative; empty = repo root). Returns empty when there are no srcs.
pub fn srcs_to_inputs(srcs: &[String], root: &str) -> Vec<FileSet> {
    if srcs.is_empty() {
        return Vec::new();
    }
    vec![FileSet {
        include: srcs.to_vec(),
        exclude: Vec::new(),
        root: root.to_string(),
    }]
}

/// Convert declared [`OutputSpec`]s into proto [`OutputDecl`]s, validating `kind`.
pub fn outputs_to_decls(outputs: &[OutputSpec]) -> Result<Vec<OutputDecl>, BuildError> {
    outputs
        .iter()
        .map(|o| {
            let kind = match o.kind.as_str() {
                "file" => OutputKind::File,
                "directory" | "dir" => OutputKind::Directory,
                other => {
                    return Err(BuildError::Manifest(format!(
                        "invalid output kind {other:?} (expected file|directory)"
                    )))
                }
            };
            Ok(OutputDecl {
                path: o.path.clone(),
                kind: kind as i32,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srcs_become_one_rooted_fileset() {
        let sets = srcs_to_inputs(&["src/lib.rs".into(), "Cargo.toml".into()], "crate");
        assert_eq!(sets.len(), 1);
        assert_eq!(sets[0].include, vec!["src/lib.rs", "Cargo.toml"]);
        assert_eq!(sets[0].root, "crate");
    }

    #[test]
    fn empty_srcs_make_no_inputs() {
        assert!(srcs_to_inputs(&[], "").is_empty());
    }

    #[test]
    fn output_kinds_map_to_proto_and_default_to_file() {
        let specs = vec![
            OutputSpec {
                path: "bin/app".into(),
                kind: "file".into(),
            },
            OutputSpec {
                path: "dist".into(),
                kind: "directory".into(),
            },
        ];
        let decls = outputs_to_decls(&specs).expect("valid kinds");
        assert_eq!(decls[0].kind, OutputKind::File as i32);
        assert_eq!(decls[0].path, "bin/app");
        assert_eq!(decls[1].kind, OutputKind::Directory as i32);
    }

    #[test]
    fn invalid_output_kind_errors() {
        let specs = vec![OutputSpec {
            path: "x".into(),
            kind: "blob".into(),
        }];
        assert!(matches!(
            outputs_to_decls(&specs),
            Err(BuildError::Manifest(_))
        ));
    }
}
