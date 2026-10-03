//! Formatting the files a run wrote, with the edition of the package that owns each.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::backends::rust::ProgressSink;
use crate::{RestructureError, Result};

/// How a manifest declares its edition.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Declared {
    Literal(String),
    Workspace,
}

/// The edition `manifest` declares in `section` (`package`, or `workspace.package` for what a
/// member inherits). A line scan, not a TOML parse: the crate has no TOML dependency and the three
/// spellings of this one key are all there is to read.
pub(super) fn declared_edition(manifest: &str, section: &str) -> Option<Declared> {
    let mut current = "";
    for line in manifest.lines() {
        let line = line.split('#').next().unwrap_or_default().trim();
        if let Some(header) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            current = header.trim();
        } else if current == section {
            if let Some(declared) = declaration(line) {
                return Some(declared);
            }
        }
    }
    None
}

fn declaration(line: &str) -> Option<Declared> {
    let (key, value) = line.split_once('=')?;
    let value = value.trim();
    match key.trim() {
        "edition" if value.starts_with('{') => {
            (value.contains("workspace") && value.contains("true")).then_some(Declared::Workspace)
        }
        "edition" => Some(Declared::Literal(value.trim_matches('"').to_string())),
        "edition.workspace" => (value == "true").then_some(Declared::Workspace),
        _ => None,
    }
}

/// Format every touched `.rs` file that still exists, one rustfmt per file so a failure names it,
/// and report each file whose bytes changed. Returns how many changed.
///
/// rustfmt also formats the child modules a file declares; the plan wrote them or they were
/// already formatted, so that is accepted.
pub(super) fn format_touched(
    root: &Path,
    touched: &BTreeSet<String>,
    progress: &ProgressSink,
) -> Result<usize> {
    let mut changed = 0;
    for file in touched.iter().filter(|file| file.ends_with(".rs")) {
        let path = root.join(file);
        if !path.is_file() {
            continue;
        }
        let edition = edition_of(root, file)?;
        let before = std::fs::read(&path)?;
        run_rustfmt(root, file, &edition)?;
        if std::fs::read(&path)? != before {
            progress(&format!("formatted: {file}"));
            changed += 1;
        }
    }
    Ok(changed)
}

fn run_rustfmt(root: &Path, file: &str, edition: &str) -> Result<()> {
    let output = Command::new("rustfmt")
        .args(["--edition", edition, file])
        .current_dir(root)
        .stdin(Stdio::null())
        .output()?;
    if output.status.success() {
        return Ok(());
    }
    Err(RestructureError::FormatFailed {
        file: file.to_string(),
        errors: String::from_utf8_lossy(&output.stderr).trim().to_string(),
    })
}

/// The edition of the package owning `file`, refusing when it cannot be determined.
fn edition_of(root: &Path, file: &str) -> Result<String> {
    let refusal = |why: &str| RestructureError::FormatFailed {
        file: file.to_string(),
        errors: format!("cannot determine the edition to format with: {why}"),
    };
    let (directory, manifest) =
        owning_manifest(root, file).ok_or_else(|| refusal("no package manifest owns it"))?;
    match declared_edition(&manifest, "package") {
        Some(Declared::Literal(edition)) => Ok(edition),
        Some(Declared::Workspace) => workspace_edition(root, &directory)
            .ok_or_else(|| refusal("it inherits the workspace's edition and none declares one")),
        None => Err(refusal("its package declares no edition")),
    }
}

/// The directory and text of the nearest manifest above `file` that declares a `[package]`.
fn owning_manifest(root: &Path, file: &str) -> Option<(PathBuf, String)> {
    let mut directory = root.join(file);
    while directory.pop() && directory.starts_with(root) {
        let text = std::fs::read_to_string(directory.join("Cargo.toml")).ok();
        if let Some(text) = text.filter(|text| text.contains("[package]")) {
            return Some((directory, text));
        }
    }
    None
}

/// The `[workspace.package] edition` of the nearest manifest above `from` that declares one.
fn workspace_edition(root: &Path, from: &Path) -> Option<String> {
    let mut directory = from.to_path_buf();
    while directory.starts_with(root) {
        let text = std::fs::read_to_string(directory.join("Cargo.toml")).ok();
        if let Some(Declared::Literal(edition)) =
            text.and_then(|text| declared_edition(&text, "workspace.package"))
        {
            return Some(edition);
        }
        if !directory.pop() {
            break;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    struct AWorkspace {
        directory: tempfile::TempDir,
    }

    fn a_package_with(edition: &str, source: &str) -> AWorkspace {
        let directory = tempfile::tempdir().expect("a temporary directory");
        std::fs::create_dir_all(directory.path().join("src")).expect("src");
        std::fs::write(
            directory.path().join("Cargo.toml"),
            format!("[package]\nname = \"demo\"\nversion = \"0.1.0\"\n{edition}\n"),
        )
        .expect("manifest");
        std::fs::write(directory.path().join("src/lib.rs"), source).expect("source");
        AWorkspace { directory }
    }

    impl AWorkspace {
        fn read(&self) -> String {
            std::fs::read_to_string(self.directory.path().join("src/lib.rs")).expect("source")
        }

        fn formatted(&self) -> (Result<usize>, Vec<String>) {
            let lines = Arc::new(Mutex::new(Vec::new()));
            let sink: ProgressSink = {
                let lines = Arc::clone(&lines);
                Arc::new(move |line: &str| lines.lock().expect("lines").push(line.to_string()))
            };
            let touched: BTreeSet<String> = ["src/lib.rs".to_string()].into();
            let outcome = format_touched(self.directory.path(), &touched, &sink);
            let said = lines.lock().expect("lines").clone();
            (outcome, said)
        }
    }

    const EDITION_2021: &str = "edition = \"2021\"";

    #[test]
    fn formats_an_unformatted_use_group_the_engine_wrote() {
        // Given a use group of one name, as the engine writes it
        let workspace = a_package_with(
            EDITION_2021,
            "mod inner {\n    pub struct Resolver;\n}\nuse inner::{Resolver};\n\n\
             pub fn make() -> Resolver {\n    Resolver\n}\n",
        );

        // When the file is formatted
        let (outcome, said) = workspace.formatted();

        // Then rustfmt's single-name form is written and the file is reported
        assert_eq!(outcome.expect("rustfmt runs"), 1);
        assert!(workspace.read().contains("use inner::Resolver;"));
        assert_eq!(said, vec!["formatted: src/lib.rs".to_string()]);
    }

    #[test]
    fn leaves_an_already_formatted_file_byte_identical_and_reports_nothing() {
        // Given a file rustfmt would not change
        let source = "pub fn two() -> u32 {\n    2\n}\n";
        let workspace = a_package_with(EDITION_2021, source);

        // When the file is formatted
        let (outcome, said) = workspace.formatted();

        // Then nothing changed and nothing is reported
        assert_eq!(outcome.expect("rustfmt runs"), 0);
        assert_eq!(workspace.read(), source);
        assert!(said.is_empty(), "{said:?}");
    }

    #[test]
    fn fails_the_run_naming_the_file_when_rustfmt_cannot_parse_it() {
        // Given a file that is not Rust
        let workspace = a_package_with(EDITION_2021, "pub fn (\n");

        // When the file is formatted
        let (outcome, _) = workspace.formatted();

        // Then the run fails naming the file
        let error = outcome
            .expect_err("an unparsable file fails the run")
            .to_string();
        assert!(error.contains("src/lib.rs"), "{error}");
    }

    #[test]
    fn refuses_to_format_when_no_edition_can_be_determined() {
        // Given a package that declares no edition
        let workspace = a_package_with("", "pub fn two() -> u32 {\n    2\n}\n");

        // When the file is formatted
        let (outcome, _) = workspace.formatted();

        // Then it is refused rather than formatted with a guess
        let error = outcome.expect_err("no edition is a refusal").to_string();
        assert!(error.contains("edition"), "{error}");
    }

    #[test]
    fn reads_a_literal_edition() {
        assert_eq!(
            declared_edition(
                "[package]\nname = \"x\"\nedition = \"2021\" # now\n",
                "package"
            ),
            Some(Declared::Literal("2021".to_string()))
        );
    }

    #[test]
    fn reads_an_edition_inherited_from_the_workspace_in_either_spelling() {
        assert_eq!(
            declared_edition("[package]\nedition.workspace = true\n", "package"),
            Some(Declared::Workspace)
        );
        assert_eq!(
            declared_edition("[package]\nedition = { workspace = true }\n", "package"),
            Some(Declared::Workspace)
        );
    }

    #[test]
    fn reads_the_edition_a_workspace_declares_for_its_members() {
        assert_eq!(
            declared_edition(
                "[workspace.package]\nedition = \"2024\"\n",
                "workspace.package"
            ),
            Some(Declared::Literal("2024".to_string()))
        );
    }
}
