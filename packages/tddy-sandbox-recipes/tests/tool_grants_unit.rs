//! Unit: lowering resolved tools into jail grants — the binary as a file, the loader's reads, and
//! a jail-local `bin/` on `PATH`.
//!
//! PRD: `docs/ft/coder/1-WIP/PRD-2026-10-31-declared-tools.md` (`#agent-tools 1/12`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use pretty_assertions::assert_eq;
use tddy_actions::tool::ResolvedTool;
use tddy_sandbox::{ReadReason, ReadSpec, SymlinkSpec};
use tddy_sandbox_recipes::{lower_tools, BinaryLibraries};

const JAIL_BIN: &str = "/sessions/s1/sandbox/.work/bin";

fn a_resolved_tool(id: &str, binary: &str) -> ResolvedTool {
    ResolvedTool {
        id: id.into(),
        command: id.into(),
        binary: PathBuf::from(binary),
        reads: vec![],
    }
}

/// The loader's view of each binary, declared by the test.
#[derive(Default)]
struct TheLoaderReads(BTreeMap<PathBuf, Vec<PathBuf>>);

impl TheLoaderReads {
    fn for_binary(mut self, binary: &str, dirs: &[&str]) -> Self {
        self.0.insert(
            PathBuf::from(binary),
            dirs.iter().map(PathBuf::from).collect(),
        );
        self
    }
}

impl BinaryLibraries for TheLoaderReads {
    fn loader_read_dirs(&self, binary: &Path) -> Vec<PathBuf> {
        self.0.get(binary).cloned().unwrap_or_default()
    }
}

const RG: &str = "/opt/homebrew/Cellar/ripgrep/14.1.1/bin/rg";

#[test]
fn the_binary_is_granted_as_a_literal_executable_file_not_its_directory() {
    // Given
    let tools = [a_resolved_tool("rg", RG)];

    // When
    let grants = lower_tools(&tools, Path::new(JAIL_BIN), &TheLoaderReads::default());

    // Then
    assert_eq!(
        grants.reads,
        vec![ReadSpec::literal(RG, ReadReason::BinaryDeps).executable()]
    );
}

#[test]
fn each_tool_is_reachable_by_its_command_through_a_link_in_the_jail_bin_dir() {
    // Given
    let tools = [a_resolved_tool("rg", RG)];

    // When
    let grants = lower_tools(&tools, Path::new(JAIL_BIN), &TheLoaderReads::default());

    // Then
    assert_eq!(
        grants.symlinks,
        vec![SymlinkSpec {
            link: PathBuf::from(JAIL_BIN).join("rg"),
            target: PathBuf::from(RG),
        }]
    );
    assert_eq!(grants.path_prefix, PathBuf::from(JAIL_BIN));
}

#[test]
fn the_loaders_directories_are_readable_but_not_executable() {
    // Given — `rg` links Homebrew's pcre2, reached through `/opt/homebrew/opt`
    let tools = [a_resolved_tool("rg", RG)];
    let loader = TheLoaderReads::default().for_binary(
        RG,
        &["/opt/homebrew/Cellar/pcre2/10.44/lib", "/opt/homebrew/opt"],
    );

    // When
    let grants = lower_tools(&tools, Path::new(JAIL_BIN), &loader);

    // Then
    assert_eq!(
        grants.reads,
        vec![
            ReadSpec::literal(RG, ReadReason::BinaryDeps).executable(),
            ReadSpec::subpath(
                "/opt/homebrew/Cellar/pcre2/10.44/lib",
                ReadReason::BinaryDeps
            ),
            ReadSpec::subpath("/opt/homebrew/opt", ReadReason::BinaryDeps),
        ]
    );
}

#[test]
fn a_tools_declared_reads_become_plain_subpath_reads() {
    // Given
    let mut tool = a_resolved_tool("node", "/usr/local/bin/node");
    tool.reads = vec![PathBuf::from("/usr/local/lib/node_modules")];

    // When
    let grants = lower_tools(&[tool], Path::new(JAIL_BIN), &TheLoaderReads::default());

    // Then
    assert_eq!(
        grants.reads,
        vec![
            ReadSpec::literal("/usr/local/bin/node", ReadReason::BinaryDeps).executable(),
            ReadSpec::subpath("/usr/local/lib/node_modules", ReadReason::Custom),
        ]
    );
}

#[test]
fn a_tool_set_with_deps_grants_every_tool_in_the_order_given() {
    // Given — the closure as `resolve_closure` returns it, deps first
    let tools = [
        a_resolved_tool("node", "/usr/local/bin/node"),
        a_resolved_tool("eslint", "/usr/local/bin/eslint"),
    ];

    // When
    let grants = lower_tools(&tools, Path::new(JAIL_BIN), &TheLoaderReads::default());

    // Then
    assert_eq!(
        grants.symlinks,
        vec![
            SymlinkSpec {
                link: PathBuf::from(JAIL_BIN).join("node"),
                target: PathBuf::from("/usr/local/bin/node"),
            },
            SymlinkSpec {
                link: PathBuf::from(JAIL_BIN).join("eslint"),
                target: PathBuf::from("/usr/local/bin/eslint"),
            },
        ]
    );
}

#[test]
fn a_loader_directory_shared_by_two_tools_is_granted_once() {
    // Given
    let tools = [
        a_resolved_tool("a", "/opt/x/bin/a"),
        a_resolved_tool("b", "/opt/x/bin/b"),
    ];
    let loader = TheLoaderReads::default()
        .for_binary("/opt/x/bin/a", &["/opt/x/lib"])
        .for_binary("/opt/x/bin/b", &["/opt/x/lib"]);

    // When
    let grants = lower_tools(&tools, Path::new(JAIL_BIN), &loader);

    // Then
    assert_eq!(
        grants.reads,
        vec![
            ReadSpec::literal("/opt/x/bin/a", ReadReason::BinaryDeps).executable(),
            ReadSpec::subpath("/opt/x/lib", ReadReason::BinaryDeps),
            ReadSpec::literal("/opt/x/bin/b", ReadReason::BinaryDeps).executable(),
        ]
    );
}
