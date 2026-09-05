use std::path::{Path, PathBuf};

use tddy_sandbox::{MachPolicy, NetworkSpec, ReadKind, ReadSpec, SandboxError, SandboxPlan};

/// Render the SBPL profile from an explicit [`SandboxPlan`].
///
/// Emits explicit read rules (`plan.reads`, always including the `(literal "/")` dyld-cache root),
/// process-exec rules (`plan.policy.exec_paths` + exec reads), the policy block, and the network
/// policy — and **never** the blanket `(allow file-read*)` wildcard.
///
/// Reads are split by what they grant, not merely listed: a [`tddy_sandbox::ReadKind::Metadata`]
/// grant renders into its own `(allow file-read-metadata …)` block and is kept out of the
/// `file-read*` one, because on a directory `file-read*` also permits listing its entries.
///
/// The writable tree is the plan's own `project_root`, `scratch_dir` and `egress_dir` plus its
/// writable mounts, and nothing else. In particular it holds no part of the host's per-user temp
/// base (`/var/folders/…`): that is shared with every other session and application on the machine,
/// and a confined process keeps its `HOME` and `TMPDIR` inside `scratch_dir`
/// ([`tddy_sandbox::scratch_runner_env`]) rather than there.
///
/// Rule order is load-bearing, not cosmetic: Seatbelt evaluates a profile top-to-bottom and the
/// **last matching rule wins**. That is why the blanket `(deny file-write*)` can be followed by
/// targeted allows, and equally why the agent context-dir carve-out below must be emitted *after*
/// the project-root allow it overrides.
pub fn render_plan(plan: &SandboxPlan) -> Result<String, SandboxError> {
    let spec = &plan.spec;
    let project_root = canonical_rule_path(&spec.project_root);
    let scratch_dir = canonical_rule_path(&spec.scratch_dir);
    let egress_dir = canonical_rule_path(&spec.egress_dir);
    let writable_tree = [
        project_root.clone(),
        scratch_dir.clone(),
        egress_dir.clone(),
    ];

    let mut out = String::new();
    out.push_str("(version 1)\n\n");
    out.push_str(";; Tight Seatbelt profile for sandboxed Claude Code CLI sessions.\n");
    out.push_str(";; Write confinement: the plan's own project + scratch + egress tree and its\n");
    out.push_str(";; writable mounts only — no share of the host's per-user temp base.\n");
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
    out.push_str(")\n\n");

    out.push_str(
        "(allow file-write*\n  (literal \"/dev/null\")\n  (literal \"/dev/zero\")\n  \
         (literal \"/dev/random\")\n  (literal \"/dev/urandom\")\n  (literal \"/dev/dtracehelper\")\n  \
         (literal \"/dev/stdin\")\n  (literal \"/dev/stdout\")\n  (literal \"/dev/stderr\")\n  \
         (literal \"/dev/ptmx\")\n  (regex #\"^/dev/tty.*\")\n  (regex #\"^/dev/ttys[0-9]+$\")\n  \
         (regex #\"^/dev/fd/[0-9]+$\"))\n\n",
    );

    // The jail's agent-context dir is carved back out of the writable tree it lives in.
    if let Some(context_dir) = context_dir_from_runner_argv(&spec.command) {
        out.push_str(
            ";; The agent's context dir is read-only *inside* the jail: it holds the guidance\n\
             ;; files (CLAUDE.md / AGENTS.md and their managed-codebase preamble) that tell the\n\
             ;; confined agent where the real codebase is. It sits under the project root, so the\n\
             ;; allow above would otherwise let the agent rewrite its own instructions. This deny\n\
             ;; comes last, so it wins. The host-side context syncer runs outside the jail and is\n\
             ;; unaffected — that asymmetry is the whole point.\n",
        );
        out.push_str(&format!(
            "(deny file-write* (subpath \"{}\"))\n\n",
            canonical_rule_path(&context_dir)
        ));
    }

    // Explicit read allow-list — the writable tree plus every declared read. No wildcard.
    out.push_str("(allow file-read*\n");
    for p in &writable_tree {
        out.push_str(&format!("  (subpath \"{p}\")\n"));
    }
    for r in plan.reads.iter().filter(|r| r.kind != ReadKind::Metadata) {
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

    // Lookup-only grants. `file-read-metadata` is the `lstat` half of `file-read*`: enough to
    // resolve a path through a directory, not enough to list what else is in it. They get their own
    // block because folding them into the allow-list above would hand over exactly the listing they
    // exist to withhold — a jail resolving `/a/b/checkout` would learn the name of every other
    // session's tree beside it. Order relative to that block is immaterial (both are allows, and
    // Seatbelt's last-match-wins settles deny-against-allow); after it is where it reads as the
    // narrower grant it is. Emitted only when something asked for it: `(allow file-read-metadata)`
    // with an empty body is a blanket allow over the whole filesystem.
    let metadata_reads: Vec<&ReadSpec> = plan
        .reads
        .iter()
        .filter(|r| r.kind == ReadKind::Metadata)
        .collect();
    if !metadata_reads.is_empty() {
        out.push_str("(allow file-read-metadata\n");
        for r in metadata_reads {
            out.push_str(&render_read_rule(r));
        }
        out.push_str(")\n\n");
    }

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

/// The agent-context directory the confined process will use, read off the command line the plan
/// launches (`plan.spec.command`).
///
/// Why derive it from argv instead of carrying it as its own plan field: `--context-dir <path>` is
/// how the path reaches the sandboxed process in the first place — every launcher (the daemon's
/// Claude/Cursor/split session paths, its plan builder, and `tddy-sandbox-app`) spells it exactly
/// that way, and `tddy-sandbox-runner` learns the directory from nowhere else. Reading the same
/// argv the child is executed with makes it impossible for the profile's carve-out to name a
/// different directory than the one the agent actually reads its guidance from; a parallel field
/// could silently drift. [`crate::spawn::spawn_plan`] already inspects this argv the same way to
/// detect `--stdio`.
///
/// Plans that run something other than the runner (action orchestration, plain shells) carry no
/// such flag and therefore get no carve-out — they have no context dir to protect. An empty value
/// is treated as absent rather than rendered as `(subpath "")`, which would be a rule matching
/// nothing useful in a profile that must stay strict.
fn context_dir_from_runner_argv(argv: &[String]) -> Option<PathBuf> {
    for (index, arg) in argv.iter().enumerate() {
        if let Some(inline) = arg.strip_prefix("--context-dir=") {
            return (!inline.is_empty()).then(|| PathBuf::from(inline));
        }
        if arg == "--context-dir" {
            return argv
                .get(index + 1)
                .filter(|value| !value.is_empty())
                .map(PathBuf::from);
        }
    }
    None
}

/// Render a single read grant as its SBPL path filter.
///
/// The filter only says *which* paths a rule matches; what may be done with them is the enclosing
/// block's business. That is why [`ReadKind::Metadata`] renders the same `(literal …)` filter as
/// [`ReadKind::Literal`] — the two differ in the operation they are granted (`file-read-metadata`
/// vs `file-read*`), and [`render_plan`] emits each in its own block.
fn render_read_rule(r: &ReadSpec) -> String {
    match &r.kind {
        ReadKind::Subpath => format!("  (subpath \"{}\")\n", canonical_rule_path(&r.host)),
        ReadKind::Literal | ReadKind::Metadata => {
            format!("  (literal \"{}\")\n", canonical_rule_path(&r.host))
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tddy_sandbox::SandboxSpec;

    // ─── Path lookup without listing ────────────────────────────────────────────
    //
    // Feature: docs/ft/coder/sandboxed-codebase-mode.md
    // Changeset: docs/dev/1-WIP/2026-09-05-sandboxed-codebase-mode.md

    /// A metadata grant is rendered as its own `file-read-metadata` rule, so the jail may resolve
    /// the path without the `file-read*` block's power to read what is there.
    #[test]
    fn a_metadata_read_is_rendered_as_a_file_read_metadata_rule() {
        // Given
        let plan = a_plan(
            vec![
                ReadSpec::literal("/", ReadReason::DyldRoot),
                ReadSpec::metadata("/Users/someone/code", ReadReason::Custom),
            ],
            NetworkSpec::default(),
        );

        // When
        let profile = render_plan(&plan).expect("render must succeed");

        // Then
        assert!(
            profile.contains("(allow file-read-metadata"),
            "a metadata grant must emit its own rule; profile was:\n{profile}"
        );
        assert!(
            profile.contains("(literal \"/Users/someone/code\")"),
            "the granted path must be named; profile was:\n{profile}"
        );
    }

    /// …and it must not leak into the `file-read*` block, which on a directory also permits listing
    /// its entries — the whole reason this kind exists rather than reusing `literal`.
    #[test]
    fn a_metadata_read_grants_no_ordinary_read_on_the_same_path() {
        // Given
        let plan = a_plan(
            vec![
                ReadSpec::literal("/", ReadReason::DyldRoot),
                ReadSpec::metadata("/Users/someone/code", ReadReason::Custom),
            ],
            NetworkSpec::default(),
        );

        // When
        let profile = render_plan(&plan).expect("render must succeed");

        // Then
        let read_block = profile
            .split("(allow file-read*")
            .nth(1)
            .expect("the profile must have a file-read* block")
            .split(")\n\n")
            .next()
            .expect("the file-read* block must end");
        assert!(
            !read_block.contains("/Users/someone/code"),
            "a metadata-only path must not appear in the file-read* block; block was:\n{read_block}"
        );
    }

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
        assert!(profile.contains("(deny network*)"));
    }

    /// The host's per-user temp base (`/var/folders/<hash>/<hash>`, and the `/var/folders` tree it
    /// sits in) is shared by every session on the machine and by every other application the user
    /// runs. A jail that holds any of it holds far more than the tree its plan declares, so no
    /// rendered profile may name it at all — not as a blanket rule, and not as the grandparent of
    /// whatever `TMPDIR` the daemon happened to inherit. A confined process keeps its `HOME` and
    /// `TMPDIR` inside the plan's own scratch dir ([`tddy_sandbox::scratch_runner_env`]), so it has
    /// no business there.
    #[test]
    fn rendered_profile_grants_no_part_of_the_host_per_user_temp_base() {
        // Given
        let plan = a_plan(
            vec![ReadSpec::literal("/", ReadReason::DyldRoot)],
            NetworkSpec::default(),
        );

        // When
        let profile = render_plan(&plan).expect("render must succeed");

        // Then
        assert!(
            !profile.contains("/var/folders"),
            "a plan that declares no path under the host per-user temp base must render no grant \
             for it:\n{profile}"
        );
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

    /// Project tree for the context-dir carve-out tests. Deliberately a path that is never created
    /// on disk: [`canonical_rule_path`] leaves a non-existent path spelled exactly as given, so the
    /// rules asserted below are stable and unaffected by the `/tmp` → `/private/tmp` symlink.
    const A_PROJECT_ROOT_NOT_ON_DISK: &str = "/tmp/tddy-context-deny-test-not-on-disk";
    const ITS_CONTEXT_DIR: &str = "/tmp/tddy-context-deny-test-not-on-disk/context";

    /// A plan shaped like every real sandboxed agent session: the confined command is a
    /// `tddy-sandbox-runner` argv that tells the runner which directory holds the agent's guidance
    /// files, and that directory lives inside the project root.
    fn a_runner_plan_told_its_context_dir() -> SandboxPlan {
        let mut plan = a_plan(
            vec![ReadSpec::literal("/", ReadReason::DyldRoot)],
            NetworkSpec::default(),
        );
        plan.spec.project_root = PathBuf::from(A_PROJECT_ROOT_NOT_ON_DISK);
        plan.spec.scratch_dir = PathBuf::from(A_PROJECT_ROOT_NOT_ON_DISK).join(".work");
        plan.spec.egress_dir = PathBuf::from(A_PROJECT_ROOT_NOT_ON_DISK).join("out");
        plan.spec.command = vec![
            "/usr/local/bin/tddy-sandbox-runner".into(),
            "--context-dir".into(),
            ITS_CONTEXT_DIR.into(),
        ];
        plan
    }

    #[test]
    fn rendered_profile_denies_writes_under_the_context_dir_the_runner_was_told_to_use() {
        // Given
        let plan = a_runner_plan_told_its_context_dir();

        // When
        let profile = render_plan(&plan).expect("render must succeed");

        // Then
        assert!(
            profile.contains(&format!(
                "(deny file-write* (subpath \"{ITS_CONTEXT_DIR}\"))"
            )),
            "the context dir must be carved out of the writable tree:\n{profile}"
        );
    }

    /// Seatbelt resolves conflicting rules by **last match wins**, so a carve-out that precedes the
    /// allow it contradicts is silently useless. Pinning the order is pinning the protection.
    #[test]
    fn rendered_profile_places_the_context_dir_deny_after_the_project_root_allow() {
        // Given
        let plan = a_runner_plan_told_its_context_dir();

        // When
        let profile = render_plan(&plan).expect("render must succeed");

        // Then
        let project_root_allow = profile
            .find(&format!("(subpath \"{A_PROJECT_ROOT_NOT_ON_DISK}\")"))
            .expect("the project root must be granted write access");
        let context_dir_deny = profile
            .find(&format!(
                "(deny file-write* (subpath \"{ITS_CONTEXT_DIR}\"))"
            ))
            .expect("the context dir must be denied write access");
        assert!(
            project_root_allow < context_dir_deny,
            "the context-dir deny must come after the project-root allow to win, but the allow is \
             at byte {project_root_allow} and the deny at byte {context_dir_deny}:\n{profile}"
        );
    }

    /// The carve-out must not cost the agent its working tree: everything in the project root that
    /// is not the context dir stays writable.
    #[test]
    fn rendered_profile_keeps_the_rest_of_the_project_root_writable() {
        // Given
        let plan = a_runner_plan_told_its_context_dir();

        // When
        let profile = render_plan(&plan).expect("render must succeed");

        // Then
        let write_grants = profile
            .split("(deny file-write* (subpath")
            .next()
            .expect("the write-allow section precedes the context-dir deny");
        assert!(
            write_grants.contains(&format!("(subpath \"{A_PROJECT_ROOT_NOT_ON_DISK}\")")),
            "the project root must still be writable:\n{profile}"
        );
    }

    /// A plan that runs something other than the sandbox runner has no agent guidance to protect,
    /// so it must not acquire a phantom deny rule.
    #[test]
    fn rendered_profile_emits_no_context_dir_deny_when_the_command_declares_none() {
        // Given
        let plan = a_plan(
            vec![ReadSpec::literal("/", ReadReason::DyldRoot)],
            NetworkSpec::default(),
        );

        // When
        let profile = render_plan(&plan).expect("render must succeed");

        // Then
        assert!(
            !profile.contains("(deny file-write* (subpath"),
            "a plan with no --context-dir must render no context-dir carve-out:\n{profile}"
        );
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
