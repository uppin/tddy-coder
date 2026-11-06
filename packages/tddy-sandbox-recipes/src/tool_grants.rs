//! Lowering resolved tools into jail grants.
//!
//! PRD: `docs/ft/coder/sandbox-builder.md` § Declared tools (`#agent-tools 1/12`).
//!
//! A [`ResolvedTool`] says which file a tool's binary is. A jail needs three things to run it, and
//! this module produces all three for a whole tool set:
//!
//! - **the binary as a file** — a literal executable read on the canonical path, never its
//!   directory, so a Homebrew `rg` does not drag every other binary in `/opt/homebrew/bin` into the
//!   jail's exec surface;
//! - **what the loader reads to start it** — its dynamic libraries, and the nearest non-symlinked
//!   ancestor of each (the loader asks through a symlink farm the profile renderer canonicalizes
//!   away);
//! - **a name on `PATH`** — a jail-local `bin/` directory of links, one per tool, placed first on
//!   the jail's `PATH`.
//!
//! [`lower_network`] lowers the same tool set's `network:` sections into the jail's egress policy
//! (`#agent-tools 3/12`).

use std::path::{Path, PathBuf};
use std::process::Command;

use tddy_actions::tool::{Egress, ResolvedTool};
use tddy_sandbox::{EgressPolicy, HostPattern, ReadReason, ReadSpec, SymlinkSpec};

/// What a resolved tool set adds to a jail's plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolGrants {
    pub reads: Vec<ReadSpec>,
    /// `<jail_bin_dir>/<command>` → the tool's canonical binary, one per tool.
    pub symlinks: Vec<SymlinkSpec>,
    /// The directory to put first on the jail's `PATH` — `jail_bin_dir` itself.
    pub path_prefix: PathBuf,
}

/// The directories the loader reads to start a binary: each dynamic library's directory, and the
/// nearest non-symlinked ancestor of each (the loader asks through a symlink farm the profile
/// renderer canonicalizes away).
///
/// A seam rather than a direct `otool -L` so lowering stays pure under test; production uses
/// [`HostBinaryLibraries`].
pub trait BinaryLibraries {
    fn loader_read_dirs(&self, binary: &Path) -> Vec<PathBuf>;
}

/// This host's loader view: `otool -L` on macOS. Elsewhere none — the cgroups backend mounts the
/// system library directories with its baseline reads.
#[derive(Debug, Clone, Copy, Default)]
pub struct HostBinaryLibraries;

impl BinaryLibraries for HostBinaryLibraries {
    fn loader_read_dirs(&self, binary: &Path) -> Vec<PathBuf> {
        let mut dirs = Vec::new();
        for library in linked_libraries(binary) {
            let Some(parent) = library.parent() else {
                continue;
            };
            push_unique(&mut dirs, parent.to_path_buf());
            if let Some(anchor) = nearest_canonical_ancestor(parent) {
                push_unique(&mut dirs, anchor);
            }
        }
        dirs
    }
}

/// The absolute library paths `otool -L` reports for `binary`. Empty where `otool` is absent or
/// fails, which is every host but macOS.
fn linked_libraries(binary: &Path) -> Vec<PathBuf> {
    let Ok(output) = Command::new("otool").arg("-L").arg(binary).output() else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .skip(1)
        .filter_map(|line| line.split_whitespace().next())
        .filter(|lib| lib.starts_with('/'))
        .map(PathBuf::from)
        .collect()
}

/// The first of `dir` and its ancestors that is its own canonical form, never `/`.
///
/// Homebrew reports a library through `<prefix>/opt/<formula>`, a symlink into `Cellar`. A grant on
/// that path is rewritten to its canonical form, so it never authorises the walk that reaches it.
/// Walking up to the first directory that is its own canonical form gives a path the renderer
/// leaves alone.
fn nearest_canonical_ancestor(dir: &Path) -> Option<PathBuf> {
    dir.ancestors()
        .take_while(|ancestor| *ancestor != Path::new("/"))
        .find(|ancestor| std::fs::canonicalize(ancestor).is_ok_and(|real| real == *ancestor))
        .map(Path::to_path_buf)
}

/// Lower `tools` into the grants a jail needs to run each of them by name.
///
/// `tools` is taken as given. A closure from `resolve_closure` already holds every dep, so each
/// dep's grants are lowered here with the rest.
pub fn lower_tools(
    tools: &[ResolvedTool],
    jail_bin_dir: &Path,
    libraries: &dyn BinaryLibraries,
) -> ToolGrants {
    let mut grants = ToolGrants {
        reads: Vec::new(),
        symlinks: Vec::new(),
        path_prefix: jail_bin_dir.to_path_buf(),
    };
    for tool in tools {
        lower_tool(&mut grants, tool, jail_bin_dir, libraries);
    }
    grants
}

fn lower_tool(
    grants: &mut ToolGrants,
    tool: &ResolvedTool,
    jail_bin_dir: &Path,
    libraries: &dyn BinaryLibraries,
) {
    push_unique(
        &mut grants.reads,
        ReadSpec::literal(tool.binary.clone(), ReadReason::BinaryDeps).executable(),
    );
    for dir in libraries.loader_read_dirs(&tool.binary) {
        push_unique(
            &mut grants.reads,
            ReadSpec::subpath(dir, ReadReason::BinaryDeps),
        );
    }
    for dir in &tool.reads {
        push_unique(
            &mut grants.reads,
            ReadSpec::subpath(dir.clone(), ReadReason::Custom),
        );
    }
    grants.symlinks.push(SymlinkSpec {
        link: jail_bin_dir.join(&tool.command),
        target: tool.binary.clone(),
    });
}

/// Append `item` unless an equal one is already granted: a loader directory two tools share is
/// granted once.
fn push_unique<T: PartialEq>(items: &mut Vec<T>, item: T) {
    if !items.contains(&item) {
        items.push(item);
    }
}

/// Lower `tools`' `network:` sections into one jail's egress policy and whether it takes an OAuth
/// callback (`#agent-tools 3/12`).
///
/// The union rule: one tool declaring `any` makes the whole jail [`EgressPolicy::Any`] — the honest
/// reading of a jail that hosts one unrestricted tool; `hosts` lists concatenate, in tool order and
/// without repeats; `none` contributes nothing, so a set of only `none` tools is
/// [`EgressPolicy::None`] and its jail starts no shim. `oauth_inbound` is true when any tool
/// declares it.
pub fn lower_network(tools: &[ResolvedTool]) -> (EgressPolicy, bool) {
    let oauth_inbound = tools.iter().any(|tool| tool.network.oauth_inbound);
    // Stays `None` unless a tool declares hosts; networkless tools keep no host list.
    let mut hosts: Option<Vec<HostPattern>> = None;
    for tool in tools {
        match &tool.network.egress {
            Egress::None => {}
            Egress::Any => return (EgressPolicy::Any, oauth_inbound),
            Egress::Hosts(declared) => {
                let lowered = hosts.get_or_insert_with(Vec::new);
                for declared_pattern in declared {
                    // `HostPattern::parse` lowercases and applies the 443 default, so equal hosts
                    // compare equal here.
                    let pattern =
                        HostPattern::new(declared_pattern.host(), declared_pattern.port());
                    if !lowered.contains(&pattern) {
                        lowered.push(pattern);
                    }
                }
            }
        }
    }
    let policy = match hosts {
        Some(hosts) => EgressPolicy::Hosts(hosts),
        None => EgressPolicy::None,
    };
    (policy, oauth_inbound)
}
