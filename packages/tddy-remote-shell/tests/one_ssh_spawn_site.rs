//! OpenSSH is spawned for a remote command in one crate: this one.
//!
//! PRD `docs/ft/daemon/1-WIP/PRD-2026-11-15-remote-shell-shell-port.md`. Every other crate reaches
//! an ssh target through the [`tddy_remote_shell::RemoteShell`] port, so the connection options and
//! the meaning of a failure are decided once.
//!
//! This is a text scan, and it is scoped on purpose:
//!
//! - It reads the `.rs` files under each crate's `src/`, this crate's excepted. A crate's `tests/`
//!   directory is not read: a test may write a stand-in `ssh`.
//! - What counts is a Rust expression that names the program `ssh` — [`NAMES_THE_SSH_PROGRAM`] —
//!   not the word. A `GIT_SSH_COMMAND` value, an `ssh://` URL scheme, `ssh-add`, `ssh-keygen` and
//!   prose do not match.
//! - Comment lines are skipped, and so is test code: a file whose name ends in `tests.rs`, and
//!   everything from an inline `#[cfg(test)] mod … {` to the end of its file.
//! - [`NOT_A_REMOTE_COMMAND`] lists the files that run `ssh` for something the port does not do.

use std::path::{Path, PathBuf};

use pretty_assertions::assert_eq;

/// The crate that owns the ssh transport.
const THE_PORT: &str = "tddy-remote-shell";

/// How Rust source names `ssh` as a program to run: the argument of `Command::new`, `Path::new`,
/// `OsStr::new`, `PathBuf::from` or `String::from`, or a string made from the literal.
const NAMES_THE_SSH_PROGRAM: &[&str] = &[
    "new(\"ssh\")",
    "from(\"ssh\")",
    "\"ssh\".to_string()",
    "\"ssh\".to_owned()",
    "\"ssh\".into()",
];

/// Files that run `ssh` for something other than a command on a session's remote, by path under
/// `packages/`, each with its reason.
const NOT_A_REMOTE_COMMAND: &[(&str, &str)] = &[
    (
        "tddy-agent-launch/src/clone_planner.rs",
        "`ssh -V`: reads the local client's version, connects nowhere",
    ),
    (
        "tddy-vm/src/qemu.rs",
        "deploys into a QEMU guest it booted, on a forwarded port with a key of its own; \
         not a session's remote",
    ),
];

/// `packages/`.
fn packages_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("this crate lives in packages/")
        .to_path_buf()
}

/// Every `.rs` file under `dir`; none when `dir` is not a directory.
fn rust_files_under(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir).into_iter().flatten() {
        let path = entry.expect("a directory entry is readable").path();
        if path.is_dir() {
            files.extend(rust_files_under(&path));
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
    files
}

/// `file` as a path under `packages/`, with `/` separators.
fn under_packages(file: &Path) -> String {
    file.strip_prefix(packages_dir())
        .expect("a scanned file is under packages/")
        .to_string_lossy()
        .replace('\\', "/")
}

/// The production source files of every crate but the port, as paths under `packages/`, sorted.
fn production_sources_outside_the_port() -> Vec<String> {
    let mut sources: Vec<String> = std::fs::read_dir(packages_dir())
        .expect("packages/ is readable")
        .map(|entry| entry.expect("a directory entry is readable").path())
        .filter(|krate| krate.file_name().is_some_and(|name| name != THE_PORT))
        .flat_map(|krate| rust_files_under(&krate.join("src")))
        .map(|file| under_packages(&file))
        .filter(|file| !file.ends_with("tests.rs"))
        .collect();
    sources.sort();
    sources
}

/// The lines of `source` that are production code: comment lines dropped, and everything from an
/// inline test module on.
fn production_lines(source: &str) -> Vec<(usize, &str)> {
    let lines: Vec<&str> = source.lines().collect();
    let test_module_starts = lines
        .windows(2)
        .position(|pair| {
            pair[0].trim() == "#[cfg(test)]"
                && pair[1].trim_start().starts_with("mod ")
                && pair[1].trim_end().ends_with('{')
        })
        .unwrap_or(lines.len());
    lines
        .into_iter()
        .enumerate()
        .take(test_module_starts)
        .filter(|(_, line)| !line.trim_start().starts_with("//"))
        .map(|(index, line)| (index + 1, line))
        .collect()
}

/// Where `file` (a path under `packages/`) names the ssh program, as `path:line`.
fn ssh_spawn_sites_in(file: &str) -> Vec<String> {
    let source =
        std::fs::read_to_string(packages_dir().join(file)).expect("a source file is readable");
    production_lines(&source)
        .into_iter()
        .filter(|(_, line)| NAMES_THE_SSH_PROGRAM.iter().any(|name| line.contains(name)))
        .map(|(number, _)| format!("{file}:{number}"))
        .collect()
}

#[test]
fn the_scan_reads_the_other_crates_sources() {
    // Given the workspace's crates
    // When the production sources outside the port are listed
    let sources = production_sources_outside_the_port();

    // Then the scan saw another crate's source, and none of the port's: "no other crate spawns
    // ssh" is not vacuous
    assert!(
        sources.contains(&"tddy-core/src/lib.rs".to_string()),
        "the scan read {} files and missed tddy-core/src/lib.rs",
        sources.len()
    );
    assert_eq!(
        sources
            .iter()
            .filter(|file| file.starts_with("tddy-remote-shell/"))
            .collect::<Vec<_>>(),
        Vec::<&String>::new()
    );
}

#[test]
fn no_other_crate_spawns_ssh() {
    // Given every production source file outside the port, the listed exceptions aside
    let excepted: Vec<&str> = NOT_A_REMOTE_COMMAND.iter().map(|(file, _)| *file).collect();
    let sources: Vec<String> = production_sources_outside_the_port()
        .into_iter()
        .filter(|file| !excepted.contains(&file.as_str()))
        .collect();

    // When each is read for an expression naming the ssh program
    let spawn_sites: Vec<String> = sources
        .iter()
        .flat_map(|file| ssh_spawn_sites_in(file))
        .collect();

    // Then none does
    assert_eq!(
        spawn_sites,
        Vec::<String>::new(),
        "these lines name the ssh program: run the command through tddy_remote_shell::RemoteShell \
         (SshShell) instead"
    );
}
