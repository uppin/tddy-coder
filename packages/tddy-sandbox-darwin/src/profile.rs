use std::path::Path;

use tddy_sandbox::{MachPolicy, NetworkSpec, ReadKind, ReadSpec, SandboxError, SandboxPlan};

/// Render the SBPL profile from an explicit [`SandboxPlan`].
///
/// Emits explicit read rules (`plan.reads`, always including the `(literal "/")` dyld-cache root),
/// process-exec rules (`plan.policy.exec_paths` + exec reads), the policy block, and the network
/// policy — and **never** the blanket `(allow file-read*)` wildcard.
pub fn render_plan(plan: &SandboxPlan) -> Result<String, SandboxError> {
    let spec = &plan.spec;
    let project_root = canonical_rule_path(&spec.project_root);
    let scratch_dir = canonical_rule_path(&spec.scratch_dir);
    let egress_dir = canonical_rule_path(&spec.egress_dir);
    let darwin_base = canonical_rule_path(Path::new(&darwin_user_temp_base()?));
    let writable_tree = [
        project_root.clone(),
        scratch_dir.clone(),
        egress_dir.clone(),
        darwin_base.clone(),
    ];

    let mut out = String::new();
    out.push_str("(version 1)\n\n");
    out.push_str(";; Tight Seatbelt profile for sandboxed Claude Code CLI sessions.\n");
    out.push_str(";; Write confinement: project + egress + OS per-user scratch only.\n");
    out.push_str(";; Read confinement: explicit allow-list (no blanket file-read*).\n\n");

    out.push_str("(deny file-write*)\n\n");

    out.push_str("(allow file-write*\n");
    for p in &writable_tree {
        out.push_str(&format!("  (subpath \"{p}\")\n"));
    }
    for m in &plan.mounts {
        if m.writable {
            out.push_str(&format!(
                "  (subpath \"{}\")\n",
                canonical_rule_path(&m.host)
            ));
        }
    }
    out.push_str("  (subpath \"/var/folders\"))\n\n");

    out.push_str(
        "(allow file-write*\n  (literal \"/dev/null\")\n  (literal \"/dev/zero\")\n  \
         (literal \"/dev/random\")\n  (literal \"/dev/urandom\")\n  (literal \"/dev/dtracehelper\")\n  \
         (literal \"/dev/stdin\")\n  (literal \"/dev/stdout\")\n  (literal \"/dev/stderr\")\n  \
         (literal \"/dev/ptmx\")\n  (regex #\"^/dev/tty.*\")\n  (regex #\"^/dev/ttys[0-9]+$\")\n  \
         (regex #\"^/dev/fd/[0-9]+$\"))\n\n",
    );

    // Explicit read allow-list — the writable tree plus every declared read. No wildcard.
    out.push_str("(allow file-read*\n");
    for p in &writable_tree {
        out.push_str(&format!("  (subpath \"{p}\")\n"));
    }
    out.push_str("  (subpath \"/var/folders\")\n");
    for r in &plan.reads {
        out.push_str(&render_read_rule(r));
    }
    for sec in &plan.env.secrets {
        let path = spec.scratch_dir.join(".secrets").join(&sec.env_name);
        out.push_str(&format!("  (literal \"{}\")\n", canonical_rule_path(&path)));
    }
    for m in &plan.mounts {
        out.push_str(&format!(
            "  (subpath \"{}\")\n",
            canonical_rule_path(&m.host)
        ));
    }
    out.push_str(")\n\n");

    // Non-file policy.
    if plan.policy.allow_dynamic_code_generation {
        out.push_str("(allow dynamic-code-generation)\n");
    }
    if plan.policy.allow_process_fork {
        out.push_str("(allow process-fork)\n");
    }
    match &plan.policy.mach_lookup {
        MachPolicy::All => out.push_str("(allow mach-lookup)\n"),
        MachPolicy::Names(names) => {
            for n in names {
                out.push_str(&format!("(allow mach-lookup (global-name \"{n}\"))\n"));
            }
        }
    }
    if plan.policy.sysctl_read {
        out.push_str("(allow sysctl-read)\n");
    }
    if plan.policy.pseudo_tty {
        out.push_str("(allow pseudo-tty)\n");
    }
    out.push_str(
        "(allow file-ioctl\n  (literal \"/dev/ptmx\")\n  (regex #\"^/dev/ttys[0-9]+$\"))\n",
    );

    // process-exec*: the project tree, the declared exec paths, and exec-marked subpath reads.
    out.push_str("(allow process-exec*\n");
    out.push_str(&format!("  (subpath \"{project_root}\")\n"));
    for p in &plan.policy.exec_paths {
        out.push_str(&format!("  (subpath \"{}\")\n", canonical_rule_path(p)));
    }
    for r in &plan.reads {
        if r.exec && r.kind == ReadKind::Subpath {
            out.push_str(&format!(
                "  (subpath \"{}\")\n",
                canonical_rule_path(&r.host)
            ));
        }
    }
    // Mounted working dirs may hold scripts/binaries the agent runs.
    for m in &plan.mounts {
        out.push_str(&format!(
            "  (subpath \"{}\")\n",
            canonical_rule_path(&m.host)
        ));
    }
    out.push_str(")\n");

    out.push_str(&render_network(&plan.network, spec.ipc_socket.as_deref()));

    Ok(out)
}

/// Render a single read grant as its SBPL rule.
fn render_read_rule(r: &ReadSpec) -> String {
    match &r.kind {
        ReadKind::Subpath => format!("  (subpath \"{}\")\n", canonical_rule_path(&r.host)),
        ReadKind::Literal => format!("  (literal \"{}\")\n", canonical_rule_path(&r.host)),
        ReadKind::Regex(pattern) => format!("  (regex #\"{pattern}\")\n"),
    }
}

/// Render the loopback network policy: AF_UNIX always; loopback TCP per declared port; ephemeral
/// inbound for the Claude OAuth callback when requested.
fn render_network(network: &NetworkSpec, ipc_socket: Option<&Path>) -> String {
    let mut out = String::new();
    out.push_str("(deny network*)\n");
    out.push_str("(allow network-bind (local unix-socket))\n");
    out.push_str("(allow network-inbound (local unix-socket))\n");
    out.push_str("(allow network-outbound (remote unix-socket))\n");
    if !network.loopback_allow_ports.is_empty() || network.allow_oauth_inbound {
        out.push_str("(allow network-bind (local tcp \"localhost:*\"))\n");
    }
    if network.allow_oauth_inbound {
        out.push_str("(allow network-inbound (local tcp \"localhost:*\"))\n");
    }
    for port in &network.loopback_allow_ports {
        out.push_str(&format!(
            "(allow network-outbound (remote tcp \"localhost:{port}\"))\n"
        ));
        out.push_str(&format!(
            "(allow network-inbound (local tcp \"localhost:{port}\"))\n"
        ));
    }
    if let Some(sock) = ipc_socket {
        let p = canonical_rule_path(sock);
        out.push_str(&format!("(allow file-read* (literal \"{p}\"))\n"));
        out.push_str(&format!("(allow file-write* (literal \"{p}\"))\n"));
    }
    out
}

/// Canonicalize a path for use in an SBPL rule.
///
/// Seatbelt evaluates file rules against the **fully symlink-resolved** path. On macOS
/// `/tmp`, `/etc`, `/var` are symlinks into `/private/...`, so a rule spelled
/// `(subpath "/tmp/…")` never matches an access the kernel reports as `/private/tmp/…`.
/// This bit creating an AF_UNIX socket file under a `/tmp` project root: the write was
/// denied even though the project subpath was "allowed". Canonicalize best-effort and
/// fall back to the original spelling when the path does not yet exist (e.g. unit tests).
fn canonical_rule_path(path: &std::path::Path) -> String {
    std::fs::canonicalize(path)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| path.to_string_lossy().into_owned())
}

fn darwin_user_temp_base() -> Result<String, SandboxError> {
    let darwin_tmp = std::env::var("TMPDIR").unwrap_or_else(|_| "/tmp/Name".to_string());
    let path = Path::new(darwin_tmp.trim_end_matches('/'));
    let mut base = path
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "/private/var/folders".to_string());
    // TMPDIR=/tmp makes parent^2 collapse to `/`, which would allow writes anywhere.
    if base == "/" || base.is_empty() {
        base = "/private/var/folders".to_string();
    }
    Ok(base)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tddy_sandbox::SandboxSpec;

    #[test]
    fn rendered_plan_denies_writes_and_allows_the_project_tree() {
        // Given
        let plan = a_plan(
            vec![ReadSpec::literal("/", ReadReason::DyldRoot)],
            NetworkSpec::default(),
        );

        // When
        let profile = render_plan(&plan).expect("render must succeed");

        // Then
        assert!(profile.contains("(deny file-write*)"));
        assert!(profile.contains("/tmp/tddy-render-test"));
        assert!(profile.contains("/var/folders"));
        assert!(profile.contains("(deny network*)"));
    }

    use tddy_sandbox::{
        EnvSpec, NetworkSpec, PolicySpec, ReadReason, ReadSpec, ResourceLimits, SandboxPlan,
    };

    fn a_plan(reads: Vec<ReadSpec>, network: NetworkSpec) -> SandboxPlan {
        let spec = SandboxSpec {
            project_root: PathBuf::from("/tmp/tddy-render-test"),
            scratch_dir: PathBuf::from("/tmp/tddy-render-test/.work"),
            egress_dir: PathBuf::from("/tmp/tddy-render-test/out"),
            allow_read_paths: vec![],
            command: vec!["/bin/echo".into()],
            env: Default::default(),
            profile_path: PathBuf::from("/tmp/tddy-render-test/profile.sb"),
            loopback_allow_ports: vec![],
            ipc_socket: None,
            cwd: None,
        };
        SandboxPlan {
            spec,
            reads,
            mounts: vec![],
            copies: vec![],
            symlinks: vec![],
            env: EnvSpec::default(),
            policy: PolicySpec::default(),
            network,
            limits: ResourceLimits::default(),
            stdin: None,
            cgroup: Default::default(),
        }
    }

    #[test]
    fn rendered_profile_grants_write_and_read_for_a_writable_mount() {
        // Given
        let mut plan = a_plan(
            vec![ReadSpec::literal("/", ReadReason::DyldRoot)],
            NetworkSpec::default(),
        );
        plan.mounts = vec![tddy_sandbox::MountSpec::read_write("/Users/me/proj")];

        // When
        let profile = render_plan(&plan).expect("render must succeed");

        // Then — the mount is writable (appears before the file-read* block) and readable
        let write_section = profile.split("(allow file-read*").next().unwrap();
        assert!(
            write_section.contains("(subpath \"/Users/me/proj\")"),
            "writable mount must be in the write block:\n{profile}"
        );
        assert!(profile.contains("(subpath \"/Users/me/proj\")"));
    }

    #[test]
    fn rendered_profile_does_not_grant_write_for_a_read_only_mount() {
        // Given
        let mut plan = a_plan(
            vec![ReadSpec::literal("/", ReadReason::DyldRoot)],
            NetworkSpec::default(),
        );
        plan.mounts = vec![tddy_sandbox::MountSpec::read_only("/Users/me/ro-proj")];

        // When
        let profile = render_plan(&plan).expect("render must succeed");

        // Then — readable but not in the write block
        let write_section = profile.split("(allow file-read*").next().unwrap();
        assert!(
            !write_section.contains("/Users/me/ro-proj"),
            "read-only mount must not be writable:\n{profile}"
        );
        assert!(
            profile.contains("(subpath \"/Users/me/ro-proj\")"),
            "read-only mount must still be readable:\n{profile}"
        );
    }

    #[test]
    fn rendered_profile_omits_the_blanket_file_read_wildcard() {
        // Given
        let plan = a_plan(
            vec![ReadSpec::literal("/", ReadReason::DyldRoot)],
            NetworkSpec::default(),
        );

        // When
        let profile = render_plan(&plan).expect("render must succeed");

        // Then — the standalone blanket allow is gone (explicit rules only)
        assert!(
            !profile.contains("(allow file-read*)"),
            "strict profile must not contain the blanket file-read wildcard:\n{profile}"
        );
    }

    #[test]
    fn rendered_profile_emits_each_declared_read_as_an_explicit_rule() {
        // Given
        let plan = a_plan(
            vec![
                ReadSpec::subpath("/opt/toolchain", ReadReason::Toolchain),
                ReadSpec::literal("/", ReadReason::DyldRoot),
                ReadSpec::regex("^/dev/ttys[0-9]+$", ReadReason::Pty),
            ],
            NetworkSpec::default(),
        );

        // When
        let profile = render_plan(&plan).expect("render must succeed");

        // Then — each kind renders as its explicit SBPL rule
        assert!(
            profile.contains("(subpath \"/opt/toolchain\")"),
            "{profile}"
        );
        assert!(profile.contains("(literal \"/\")"), "{profile}");
        assert!(
            profile.contains("(regex #\"^/dev/ttys[0-9]+$\")"),
            "{profile}"
        );
    }

    #[test]
    fn rendered_profile_emits_the_dyld_root_literal() {
        // Given
        let plan = a_plan(
            vec![ReadSpec::literal("/", ReadReason::DyldRoot)],
            NetworkSpec::default(),
        );

        // When
        let profile = render_plan(&plan).expect("render must succeed");

        // Then
        assert!(profile.contains("(literal \"/\")"), "{profile}");
    }

    #[test]
    fn rendered_profile_emits_oauth_loopback_inbound_when_requested() {
        // Given
        let plan = a_plan(
            vec![ReadSpec::literal("/", ReadReason::DyldRoot)],
            NetworkSpec {
                loopback_allow_ports: vec![],
                allow_oauth_inbound: true,
            },
        );

        // When
        let profile = render_plan(&plan).expect("render must succeed");

        // Then — the Claude OAuth callback (ephemeral loopback port) is permitted to listen
        assert!(
            profile.contains("(allow network-inbound (local tcp \"localhost:*\"))"),
            "{profile}"
        );
    }
}
