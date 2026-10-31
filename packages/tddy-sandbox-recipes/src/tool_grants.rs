//! Lowering resolved tools into jail grants.
//!
//! PRD: `docs/ft/coder/1-WIP/PRD-2026-10-31-declared-tools.md` (`#agent-tools 1/12`).
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

use tddy_actions::tool::ResolvedTool;
use tddy_sandbox::{EgressPolicy, ReadSpec, SymlinkSpec};

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
        // TODO(tool-defs): implement — move `binary_exec_reads`' library walk and
        // `binary_dependency_traversal_dirs` (`tddy-daemon-sandbox`) here.
        let _ = binary;
        todo!("tool-defs: HostBinaryLibraries::loader_read_dirs")
    }
}

/// Lower `tools` into the grants a jail needs to run each of them by name.
pub fn lower_tools(
    tools: &[ResolvedTool],
    jail_bin_dir: &Path,
    libraries: &dyn BinaryLibraries,
) -> ToolGrants {
    // TODO(tool-defs): implement.
    let _ = (tools, jail_bin_dir, libraries);
    todo!("tool-defs: lower_tools")
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
    // TODO(egress-allowlist): implement
    let _ = tools;
    todo!("egress-allowlist: lower_network")
}
