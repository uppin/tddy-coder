//! Claude CLI argv + MCP config for sandboxed sessions (remote-codebase tool model).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::builder::{CopySpec, MachPolicy, PolicySpec, ReadReason, ReadSpec};
use crate::workspace_exec_tool_names;

/// The single, reviewable list of read grants a sandboxed Claude jail needs: the dyld-cache root
/// (`(literal "/")`), system libraries, the toolchain, the Claude binary's dynamic-library
/// directories (`otool -L claude_binary`), OS caches, and the PTY devices. This is the one audit
/// point for what a Claude jail may read; it composes the platform detectors internally so callers
/// don't reassemble the set.
pub fn claude_required_reads(claude_binary: &Path) -> Vec<ReadSpec> {
    let mut reads = system_baseline_reads();
    reads.extend(detect_toolchain_reads());
    reads.extend(binary_exec_reads(claude_binary));
    reads
}

/// The always-needed OS read set for a V8/Node CLI under the jail. Kept explicit (no wildcard) so
/// the surface is auditable; growth is a one-line, reason-tagged addition here.
#[cfg(target_os = "macos")]
pub fn system_baseline_reads() -> Vec<ReadSpec> {
    let exec_subpath = |p: &str| ReadSpec::subpath(p, ReadReason::SystemLibs).executable();
    let subpath = |p: &str| ReadSpec::subpath(p, ReadReason::SystemLibs);
    let cache = |p: &str| ReadSpec::subpath(p, ReadReason::OsCaches);
    vec![
        // dyld4 CacheFinder reads the root node to locate the shared cache; without it the child
        // SIGABRTs in dyld before main().
        ReadSpec::literal("/", ReadReason::DyldRoot),
        subpath("/usr/lib"),
        exec_subpath("/usr/libexec"),
        subpath("/System"),
        subpath("/Library"),
        subpath("/private/var/db/dyld"),
        subpath("/private/etc"),
        exec_subpath("/usr/bin"),
        exec_subpath("/bin"),
        exec_subpath("/sbin"),
        cache("/private/var/folders"),
        cache("/usr/share/zoneinfo"),
        cache("/private/var/db/timezone"),
        // ICU locale data — the V8/Node `claude` binary SIGTRAPs at startup without it.
        subpath("/usr/share/icu"),
        // PTY master + allocated slaves (openpty opens these O_RDWR).
        ReadSpec::literal("/dev/ptmx", ReadReason::Pty),
        ReadSpec::regex("^/dev/ttys[0-9]+$", ReadReason::Pty),
    ]
}

/// Linux baseline read set (consumed by the cgroups bind-mount path).
#[cfg(not(target_os = "macos"))]
pub fn system_baseline_reads() -> Vec<ReadSpec> {
    let exec_subpath = |p: &str| ReadSpec::subpath(p, ReadReason::SystemLibs).executable();
    let subpath = |p: &str| ReadSpec::subpath(p, ReadReason::SystemLibs);
    [
        exec_subpath("/usr/bin"),
        exec_subpath("/bin"),
        subpath("/usr/lib"),
        subpath("/lib"),
        subpath("/lib64"),
        subpath("/usr/lib64"),
        subpath("/etc/ssl/certs"),
        ReadSpec::literal("/etc/resolv.conf", ReadReason::SystemLibs),
        ReadSpec::literal("/etc/ld.so.cache", ReadReason::SystemLibs),
        subpath("/usr/share/zoneinfo"),
    ]
    .into_iter()
    .filter(|r| std::path::Path::new(&r.host).exists())
    .collect()
}

/// Read grants needed to exec a Mach-O binary inside the jail: its directory (exec) plus the parent
/// dirs of every dylib reported by `otool -L` (read-only).
#[cfg(target_os = "macos")]
pub fn binary_exec_reads(binary: &Path) -> Vec<ReadSpec> {
    let mut reads = Vec::new();
    if let Some(parent) = binary.parent() {
        reads.push(ReadSpec::subpath(parent, ReadReason::BinaryDeps).executable());
    }
    if let Ok(output) = std::process::Command::new("otool")
        .args(["-L"])
        .arg(binary)
        .output()
    {
        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout);
            for line in text.lines().skip(1) {
                let lib = line.split_whitespace().next().unwrap_or("");
                if lib.is_empty() || !lib.starts_with('/') {
                    continue;
                }
                if let Some(parent) = Path::new(lib).parent() {
                    reads.push(ReadSpec::subpath(parent, ReadReason::BinaryDeps));
                }
            }
        }
    }
    reads
}

/// Read grants needed to exec a binary inside the jail (its directory).
#[cfg(not(target_os = "macos"))]
pub fn binary_exec_reads(binary: &Path) -> Vec<ReadSpec> {
    binary
        .parent()
        .map(|parent| vec![ReadSpec::subpath(parent, ReadReason::BinaryDeps).executable()])
        .unwrap_or_default()
}

/// Detected toolchain directories (node, Homebrew, Xcode) the agent may shell out to.
#[cfg(target_os = "macos")]
pub fn detect_toolchain_reads() -> Vec<ReadSpec> {
    let mut reads = Vec::new();
    let mut push_dir = |dir: PathBuf| {
        if !reads.iter().any(|r: &ReadSpec| r.host == dir) {
            reads.push(ReadSpec::subpath(dir, ReadReason::Toolchain).executable());
        }
    };
    let run = |program: &str, args: &[&str]| -> Option<String> {
        let out = std::process::Command::new(program)
            .args(args)
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if s.is_empty() {
            None
        } else {
            Some(s)
        }
    };
    if let Some(dev) = run("xcode-select", &["-p"]) {
        push_dir(PathBuf::from(dev));
    }
    if let Some(node) = run("which", &["node"]) {
        if let Some(parent) = Path::new(&node).parent() {
            push_dir(parent.to_path_buf());
        }
    }
    if let Some(brew) = run("brew", &["--prefix"]) {
        push_dir(PathBuf::from(brew));
    }
    reads
}

#[cfg(not(target_os = "macos"))]
pub fn detect_toolchain_reads() -> Vec<ReadSpec> {
    Vec::new()
}

/// The files copied into the jail HOME for Claude — `.credentials.json` only. Host `settings.json`
/// (and its hooks) are intentionally NOT copied: they reference host scripts/`node` and fail in the
/// jail.
pub fn claude_required_copies(host_home: &Path, scratch_home: &Path) -> Vec<CopySpec> {
    vec![CopySpec {
        src: host_home.join(".claude").join(".credentials.json"),
        dest: scratch_home.join(".claude").join(".credentials.json"),
        optional: true,
        mode: Some(0o600),
    }]
}

/// Non-file policy a Claude jail needs: V8 JIT (dynamic-code-generation), process-fork (PTY child),
/// mach-lookup, sysctl-read, pseudo-tty, and the process-exec paths.
pub fn claude_policy() -> PolicySpec {
    PolicySpec {
        allow_dynamic_code_generation: true,
        allow_process_fork: true,
        mach_lookup: MachPolicy::All,
        sysctl_read: true,
        pseudo_tty: true,
        exec_paths: [
            "/usr/bin",
            "/usr/libexec",
            "/bin",
            "/sbin",
            "/System",
            "/Library",
            "/Applications",
            "/opt/homebrew",
            "/opt/local",
        ]
        .into_iter()
        .map(PathBuf::from)
        .collect(),
    }
}

/// Clean environment for the sandbox runner (HOME/TMPDIR/PATH/TERM/RUST_LOG/TDDY_*). Moved to the
/// shared crate so the daemon and the standalone app share one definition.
pub fn default_runner_env(
    scratch_home: &Path,
    scratch_tmp: &Path,
    session_id: &str,
    tool_ipc_socket: &Path,
    egress_dir: &Path,
) -> BTreeMap<String, String> {
    let mut env = BTreeMap::new();
    env.insert("HOME".into(), scratch_home.to_string_lossy().to_string());
    env.insert("TMPDIR".into(), scratch_tmp.to_string_lossy().to_string());
    env.insert("TDDY_SANDBOX_SESSION_ID".into(), session_id.to_string());
    env.insert(
        "TDDY_SANDBOX_TOOL_IPC".into(),
        tool_ipc_socket.to_string_lossy().to_string(),
    );
    env.insert(
        "TDDY_SANDBOX_EGRESS_DIR".into(),
        egress_dir.to_string_lossy().to_string(),
    );
    env.insert(
        "RUST_LOG".into(),
        std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()),
    );
    env.insert("TERM".into(), "xterm-256color".into());
    env.insert("PATH".into(), "/usr/bin:/bin:/usr/sbin:/sbin".into());
    for key in [
        "TDDY_EGRESS_PROBE_HOST",
        "TDDY_EGRESS_PROBE_PORT",
        "TDDY_EGRESS_PROBE_URL",
    ] {
        if let Ok(value) = std::env::var(key) {
            if !value.trim().is_empty() {
                env.insert(key.into(), value);
            }
        }
    }
    if let Ok(probe_target) = std::env::var("TDDY_EGRESS_PROBE_TARGET") {
        if !probe_target.trim().is_empty() {
            env.insert("TDDY_EGRESS_PROBE_TARGET".into(), probe_target);
        }
    }
    env
}

const PERMISSION_PROMPT_TOOL: &str = "mcp__tddy-tools__approval_prompt";
const MCP_CONFIG_FILENAME: &str = "claude-mcp-config.json";

/// `--allowedTools` entries for sandbox claude: `mcp__tddy-tools__*` exec tools + `AskUserQuestion`.
pub fn build_sandbox_claude_allowlist() -> Vec<String> {
    tddy_workflow_recipes::permissions::build_remote_allowlist(workspace_exec_tool_names())
}

/// Write MCP config registering `tddy-tools --mcp` under a writable scratch directory.
pub fn write_sandbox_mcp_config(scratch_dir: &Path, tddy_tools_path: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(scratch_dir).with_context(|| {
        format!(
            "create scratch dir for MCP config: {}",
            scratch_dir.display()
        )
    })?;
    let path = scratch_dir.join(MCP_CONFIG_FILENAME);
    let config = serde_json::json!({
        "mcpServers": {
            "tddy-tools": {
                "command": tddy_tools_path.to_string_lossy(),
                "args": ["--mcp"]
            }
        }
    });
    std::fs::write(&path, config.to_string())
        .with_context(|| format!("write MCP config: {}", path.display()))?;
    Ok(path)
}

/// Append `--allowedTools`, `--permission-prompt-tool`, and `--mcp-config` for sandbox spawn.
pub fn append_sandbox_claude_mcp_args(
    argv: &mut Vec<String>,
    scratch_dir: &Path,
    tddy_tools_path: &Path,
) -> Result<()> {
    let mcp_path = write_sandbox_mcp_config(scratch_dir, tddy_tools_path)?;
    for tool in build_sandbox_claude_allowlist() {
        argv.push("--allowedTools".into());
        argv.push(tool);
    }
    argv.push("--permission-prompt-tool".into());
    argv.push(PERMISSION_PROMPT_TOOL.into());
    argv.push("--mcp-config".into());
    argv.push(mcp_path.to_string_lossy().into_owned());
    Ok(())
}

/// Writable directory for sandbox MCP config (context dir is read-only).
pub fn sandbox_claude_scratch_dir(fallback: &Path) -> PathBuf {
    std::env::var("TMPDIR")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| fallback.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    const NATIVE_TOOLS_EXCLUDED: &[&str] = &[
        "Read",
        "Write",
        "Edit",
        "Glob",
        "Grep",
        "Bash",
        "Shell",
        "SemanticSearch",
    ];

    #[test]
    fn sandbox_claude_allowlist_uses_mcp_prefix_and_excludes_native_tools() {
        let allowlist = build_sandbox_claude_allowlist();
        let allowset: HashSet<_> = allowlist.iter().cloned().collect();

        for name in workspace_exec_tool_names() {
            let prefixed = format!("mcp__tddy-tools__{name}");
            assert!(
                allowset.contains(&prefixed),
                "allowlist must contain {prefixed}; got: {allowlist:?}"
            );
        }
        assert!(allowset.contains("AskUserQuestion"));
        for native in NATIVE_TOOLS_EXCLUDED {
            assert!(
                !allowset.contains(*native),
                "native tool {native} must not appear un-prefixed in sandbox allowlist"
            );
        }
    }

    #[test]
    fn claude_required_reads_include_the_dyld_root_literal() {
        use crate::builder::{ReadKind, ReadReason};

        // When
        let reads = claude_required_reads(Path::new("/usr/bin/true"));

        // Then — the dyld-cache root literal "/" is present and tagged DyldRoot
        assert!(
            reads.iter().any(|r| {
                r.kind == ReadKind::Literal
                    && r.host == Path::new("/")
                    && r.reason == ReadReason::DyldRoot
            }),
            "claude reads must include the dyld root literal: {reads:?}"
        );
    }

    #[test]
    fn claude_required_copies_seed_only_the_credentials_file() {
        // When
        let copies = claude_required_copies(Path::new("/home/user"), Path::new("/jail/home"));

        // Then — exactly one copy, the credentials file; no settings.json (its hooks break in-jail)
        assert_eq!(
            copies.len(),
            1,
            "expected only the credentials copy: {copies:?}"
        );
        assert!(
            copies[0].src.ends_with(".claude/.credentials.json"),
            "copy src must be the credentials file: {:?}",
            copies[0].src
        );
        assert!(
            copies
                .iter()
                .all(|c| !c.src.to_string_lossy().contains("settings")),
            "settings.json must not be seeded into the jail: {copies:?}"
        );
    }

    #[test]
    fn write_sandbox_mcp_config_registers_tddy_tools_mcp_server() {
        let dir = tempfile::tempdir().unwrap();
        let tools = dir.path().join("tddy-tools");
        let path =
            write_sandbox_mcp_config(dir.path(), &tools).expect("write MCP config must succeed");
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let server = &json["mcpServers"]["tddy-tools"];
        assert_eq!(server["command"].as_str().unwrap(), tools.to_string_lossy());
        assert_eq!(server["args"], serde_json::json!(["--mcp"]));
    }
}
