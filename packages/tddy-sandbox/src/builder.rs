//! Explicit, cross-platform sandbox builder.
//!
//! [`SandboxBuilder`] assembles a [`SandboxPlan`] — an explicit, auditable description of
//! everything a jailed process is allowed to touch: which paths it may read (and exec), which
//! files are copied in, which symlinks exist, the environment, secrets, policies, network rules,
//! and resource limits. **Nothing is read, copied, symlinked, or exposed unless a caller names
//! it** — `build()` adds no implicit defaults and runs no filesystem/subprocess detection (that
//! lives in opt-in helpers callers may invoke and pass in explicitly).
//!
//! The plan is consumed by both platform backends: `tddy-sandbox-darwin` renders it to an SBPL
//! profile, `tddy-sandbox-cgroups` renders it to read-only bind-mounts + cgroup limits.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::{SandboxError, SandboxSpec};

/// How a read rule matches paths. An enum (not a `recursive: bool`) because SBPL genuinely needs a
/// regex rule for the PTY slave devices (`^/dev/ttys[0-9]+$`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ReadKind {
    /// Recursive subtree (`(subpath ...)`).
    Subpath,
    /// A single path (`(literal ...)`).
    Literal,
    /// A regex over absolute paths (`(regex #"...")`).
    Regex(String),
}

/// Provenance of a read rule — auditable, drives discovery labels and the spawn manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadReason {
    /// `(literal "/")` — dyld4 CacheFinder reads the root node to locate the shared cache.
    DyldRoot,
    SystemLibs,
    Toolchain,
    BinaryDeps,
    OsCaches,
    UserConfig,
    Pty,
    Custom,
}

/// A read (and optionally exec) grant for a single host path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadSpec {
    /// Host path the grant is for.
    pub host: PathBuf,
    /// Optional remap to a different path inside the jail (Linux bind only; `None` = same path).
    pub jail: Option<PathBuf>,
    pub kind: ReadKind,
    /// Also grant `process-exec*` (macOS) / drop `MS_NOEXEC` (Linux).
    pub exec: bool,
    pub reason: ReadReason,
}

impl ReadSpec {
    /// A recursive read grant for a subtree.
    pub fn subpath(host: impl Into<PathBuf>, reason: ReadReason) -> Self {
        Self {
            host: host.into(),
            jail: None,
            kind: ReadKind::Subpath,
            exec: false,
            reason,
        }
    }

    /// A read grant for a single path.
    pub fn literal(host: impl Into<PathBuf>, reason: ReadReason) -> Self {
        Self {
            host: host.into(),
            jail: None,
            kind: ReadKind::Literal,
            exec: false,
            reason,
        }
    }

    /// A read grant matching an absolute-path regex (e.g. PTY slaves).
    pub fn regex(pattern: impl Into<String>, reason: ReadReason) -> Self {
        let pattern = pattern.into();
        Self {
            host: PathBuf::from("/"),
            jail: None,
            kind: ReadKind::Regex(pattern),
            exec: false,
            reason,
        }
    }

    /// Mark this grant as also executable.
    pub fn executable(mut self) -> Self {
        self.exec = true;
        self
    }

    /// Remap the grant to a different path inside the jail (Linux bind only).
    pub fn at_jail_path(mut self, jail: impl Into<PathBuf>) -> Self {
        self.jail = Some(jail.into());
        self
    }
}

/// A file to copy into the writable jail tree before spawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopySpec {
    pub src: PathBuf,
    pub dest: PathBuf,
    /// Skip silently when `src` is missing instead of failing.
    pub optional: bool,
    /// chmod the destination after copying.
    pub mode: Option<u32>,
}

/// A symlink to create inside the jail tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymlinkSpec {
    pub link: PathBuf,
    pub target: PathBuf,
}

/// A host directory made available inside the jail (e.g. the project repo). On macOS the path is
/// granted read — and write when `writable` — at its real location (Seatbelt has no path remap, so
/// `jail` is ignored there). On Linux it is bind-mounted read-only (or read-write when `writable`),
/// optionally remapped to `jail`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MountSpec {
    pub host: PathBuf,
    pub jail: Option<PathBuf>,
    pub writable: bool,
}

impl MountSpec {
    /// A read-only mount of `host` at its real path.
    pub fn read_only(host: impl Into<PathBuf>) -> Self {
        Self {
            host: host.into(),
            jail: None,
            writable: false,
        }
    }

    /// A read-write mount of `host` at its real path.
    pub fn read_write(host: impl Into<PathBuf>) -> Self {
        Self {
            host: host.into(),
            jail: None,
            writable: true,
        }
    }
}

/// A secret delivered out-of-band: written to a `0600` file under scratch and set on the inner
/// child only — never placed in the broad env list or `sandbox-exec` argv.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretSpec {
    pub env_name: String,
    pub source: SecretSource,
}

/// Where a secret's value comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecretSource {
    /// A literal value supplied by the caller.
    Value(String),
    /// A host file whose contents are the secret.
    HostFile(PathBuf),
}

/// Environment for the inner process: a plain var map plus out-of-band secrets.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EnvSpec {
    pub vars: BTreeMap<String, String>,
    pub secrets: Vec<SecretSpec>,
}

/// mach-lookup policy (macOS).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MachPolicy {
    All,
    Names(Vec<String>),
}

/// Non-file allows (macOS-centric; Linux ignores the macOS-only knobs).
#[derive(Debug, Clone)]
pub struct PolicySpec {
    pub allow_dynamic_code_generation: bool,
    pub allow_process_fork: bool,
    pub mach_lookup: MachPolicy,
    pub sysctl_read: bool,
    pub pseudo_tty: bool,
    /// Paths granted `process-exec*` (macOS).
    pub exec_paths: Vec<PathBuf>,
}

impl Default for PolicySpec {
    fn default() -> Self {
        Self {
            allow_dynamic_code_generation: false,
            allow_process_fork: false,
            mach_lookup: MachPolicy::Names(vec![]),
            sysctl_read: false,
            pseudo_tty: false,
            exec_paths: vec![],
        }
    }
}

/// Loopback network policy.
#[derive(Debug, Clone, Default)]
pub struct NetworkSpec {
    /// Loopback TCP ports the jail may bind and connect to (gRPC + egress shim).
    pub loopback_allow_ports: Vec<u16>,
    /// Allow inbound on ephemeral loopback ports (Claude OAuth callback server).
    pub allow_oauth_inbound: bool,
}

/// cgroup v2 / resource limits (Linux).
#[derive(Debug, Clone, Default)]
pub struct ResourceLimits {
    pub memory_max: Option<u64>,
    pub cpu_max: Option<String>,
    pub pids_max: Option<u64>,
}

/// cgroup v2 delegation parameters for the Linux cgroups backend. All fields are optional so the
/// backend can derive sensible defaults at runtime (deriving the delegated base from
/// `/proc/self/cgroup`, controllers `memory cpu pids`, supervisor leaf `supervisor`). The macOS and
/// QEMU backends ignore this. Sourced from daemon config so nothing is hardcoded in the crate.
#[derive(Debug, Clone, Default)]
pub struct CgroupConfig {
    /// Explicit delegated cgroup base directory. When set, skips `/proc/self/cgroup` derivation.
    pub base_override: Option<PathBuf>,
    /// cgroup v2 unified mount root. Defaults to `/sys/fs/cgroup` when unset.
    pub mount_root: Option<PathBuf>,
    /// Controllers to enable in the base's `cgroup.subtree_control`. Empty means `[memory, cpu, pids]`.
    pub controllers: Vec<String>,
    /// Leaf cgroup the daemon relocates its own process into. Defaults to `supervisor` when unset.
    pub supervisor_leaf: Option<String>,
}

/// The fully-explicit jail description both backends consume. Wraps the legacy [`SandboxSpec`]
/// (composition) so spec-only code keeps working; the typed allow-lists are additive.
#[derive(Debug, Clone)]
pub struct SandboxPlan {
    pub spec: SandboxSpec,
    pub reads: Vec<ReadSpec>,
    pub mounts: Vec<MountSpec>,
    pub copies: Vec<CopySpec>,
    pub symlinks: Vec<SymlinkSpec>,
    pub env: EnvSpec,
    pub policy: PolicySpec,
    pub network: NetworkSpec,
    pub limits: ResourceLimits,
    /// Optional stdin bytes fed to the confined child after spawn.
    pub stdin: Option<Vec<u8>>,
    /// cgroup v2 delegation parameters (Linux backend only; ignored elsewhere).
    pub cgroup: CgroupConfig,
}

/// Assembles a [`SandboxPlan`] from explicit, caller-supplied grants.
#[derive(Debug, Clone)]
pub struct SandboxBuilder {
    project_root: PathBuf,
    scratch_dir: PathBuf,
    egress_dir: PathBuf,
    command: Vec<String>,
    profile_path: Option<PathBuf>,
    ipc_socket: Option<PathBuf>,
    reads: Vec<ReadSpec>,
    mounts: Vec<MountSpec>,
    copies: Vec<CopySpec>,
    symlinks: Vec<SymlinkSpec>,
    env: BTreeMap<String, String>,
    secrets: Vec<SecretSpec>,
    policy: PolicySpec,
    network: NetworkSpec,
    limits: ResourceLimits,
    cwd: Option<PathBuf>,
    stdin: Option<Vec<u8>>,
}

impl SandboxBuilder {
    /// Start an empty plan: no reads, copies, symlinks, env, or policy unless the caller adds them.
    pub fn new(
        project_root: impl Into<PathBuf>,
        scratch_dir: impl Into<PathBuf>,
        egress_dir: impl Into<PathBuf>,
        command: Vec<String>,
    ) -> Self {
        Self {
            project_root: project_root.into(),
            scratch_dir: scratch_dir.into(),
            egress_dir: egress_dir.into(),
            command,
            profile_path: None,
            ipc_socket: None,
            reads: vec![],
            mounts: vec![],
            copies: vec![],
            symlinks: vec![],
            env: BTreeMap::new(),
            secrets: vec![],
            policy: PolicySpec::default(),
            network: NetworkSpec::default(),
            limits: ResourceLimits::default(),
            cwd: None,
            stdin: None,
        }
    }

    pub fn stdin(mut self, stdin: Option<Vec<u8>>) -> Self {
        self.stdin = stdin;
        self
    }

    pub fn cwd(mut self, cwd: Option<PathBuf>) -> Self {
        self.cwd = cwd;
        self
    }

    pub fn profile_path(mut self, p: impl Into<PathBuf>) -> Self {
        self.profile_path = Some(p.into());
        self
    }

    pub fn ipc_socket(mut self, p: Option<PathBuf>) -> Self {
        self.ipc_socket = p;
        self
    }

    pub fn read(mut self, r: ReadSpec) -> Self {
        self.reads.push(r);
        self
    }

    pub fn reads(mut self, r: Vec<ReadSpec>) -> Self {
        self.reads.extend(r);
        self
    }

    pub fn mount(mut self, m: MountSpec) -> Self {
        self.mounts.push(m);
        self
    }

    pub fn mounts(mut self, m: Vec<MountSpec>) -> Self {
        self.mounts.extend(m);
        self
    }

    pub fn copy(mut self, c: CopySpec) -> Self {
        self.copies.push(c);
        self
    }

    pub fn copies(mut self, c: Vec<CopySpec>) -> Self {
        self.copies.extend(c);
        self
    }

    pub fn symlink(mut self, s: SymlinkSpec) -> Self {
        self.symlinks.push(s);
        self
    }

    pub fn env_map(mut self, vars: BTreeMap<String, String>) -> Self {
        self.env = vars;
        self
    }

    pub fn secret(mut self, env_name: impl Into<String>, source: SecretSource) -> Self {
        self.secrets.push(SecretSpec {
            env_name: env_name.into(),
            source,
        });
        self
    }

    pub fn policy(mut self, p: PolicySpec) -> Self {
        self.policy = p;
        self
    }

    pub fn network(mut self, n: NetworkSpec) -> Self {
        self.network = n;
        self
    }

    pub fn limits(mut self, l: ResourceLimits) -> Self {
        self.limits = l;
        self
    }

    /// Assemble the plan. Pure: no filesystem access, no subprocess detection. Dedups reads by
    /// `(host, kind)`, drops reads shadowed by an enclosing subpath, validates that copy
    /// destinations and symlink links are inside the writable jail tree, computes per-secret
    /// scratch file paths, and mirrors `Subpath` reads into `spec.allow_read_paths`.
    pub fn build(self) -> Result<SandboxPlan, SandboxError> {
        let SandboxBuilder {
            project_root,
            scratch_dir,
            egress_dir,
            command,
            profile_path,
            ipc_socket,
            reads,
            mounts,
            copies,
            symlinks,
            env,
            secrets,
            policy,
            network,
            limits,
            cwd,
            stdin,
        } = self;

        // Dedup reads by (host, kind), preserving first-seen order.
        let mut seen: std::collections::HashSet<(PathBuf, ReadKind)> =
            std::collections::HashSet::new();
        let mut deduped: Vec<ReadSpec> = Vec::new();
        for r in reads {
            if seen.insert((r.host.clone(), r.kind.clone())) {
                deduped.push(r);
            }
        }

        // An empty host would render as `(subpath "")` / `(literal "")`: macOS `sandbox-exec`
        // rejects it, and as an enclosing subpath it starts-with (shadows) every other read below.
        // Never let one through. Regex grants carry a placeholder host ("/") and are exempt.
        deduped.retain(|r| matches!(r.kind, ReadKind::Regex(_)) || !r.host.as_os_str().is_empty());

        // Drop reads fully contained by an enclosing `Subpath` grant (regex grants never shadow and
        // are never shadowed — their `host` is a placeholder).
        let enclosing: Vec<PathBuf> = deduped
            .iter()
            .filter(|r| r.kind == ReadKind::Subpath)
            .map(|r| r.host.clone())
            .collect();
        let reads: Vec<ReadSpec> = deduped
            .into_iter()
            .filter(|r| {
                if matches!(r.kind, ReadKind::Regex(_)) {
                    return true;
                }
                !enclosing
                    .iter()
                    .any(|h| h != &r.host && r.host.starts_with(h))
            })
            .collect();

        // Copy destinations and symlink links must stay inside the writable jail tree.
        for c in &copies {
            if !is_inside(&c.dest, &project_root, &scratch_dir, &egress_dir) {
                return Err(SandboxError::InvalidSpec(format!(
                    "copy destination {} is outside the writable jail tree",
                    c.dest.display()
                )));
            }
        }
        for s in &symlinks {
            if !is_inside(&s.link, &project_root, &scratch_dir, &egress_dir) {
                return Err(SandboxError::InvalidSpec(format!(
                    "symlink link {} is outside the writable jail tree",
                    s.link.display()
                )));
            }
        }

        // Mounted host directories are granted at absolute paths (Seatbelt and bind mounts both
        // require them).
        for m in &mounts {
            if !m.host.is_absolute() {
                return Err(SandboxError::InvalidSpec(format!(
                    "mount host {} must be absolute",
                    m.host.display()
                )));
            }
        }

        // Each secret is delivered via a `0600` scratch file referenced by `TDDY_SECRET_<NAME>`; the
        // value itself never enters the env var map (only the file path does).
        let mut vars = env;
        for sec in &secrets {
            let path = scratch_dir.join(".secrets").join(&sec.env_name);
            vars.insert(
                format!("TDDY_SECRET_{}", sec.env_name),
                path.to_string_lossy().into_owned(),
            );
        }

        let allow_read_paths: Vec<PathBuf> = reads
            .iter()
            .filter(|r| r.kind == ReadKind::Subpath)
            .map(|r| r.host.clone())
            .collect();

        let profile_path = profile_path.unwrap_or_else(|| project_root.join("profile.sb"));

        let spec = SandboxSpec {
            project_root,
            scratch_dir,
            egress_dir,
            allow_read_paths,
            command,
            env: vars.clone(),
            profile_path,
            loopback_allow_ports: network.loopback_allow_ports.clone(),
            ipc_socket,
            cwd,
        };
        spec.validate()?;

        Ok(SandboxPlan {
            spec,
            reads,
            mounts,
            copies,
            symlinks,
            env: EnvSpec { vars, secrets },
            policy,
            network,
            limits,
            stdin,
            cgroup: CgroupConfig::default(),
        })
    }
}

/// Whether `path` lies within any of the writable jail roots.
fn is_inside(
    path: &std::path::Path,
    project_root: &std::path::Path,
    scratch_dir: &std::path::Path,
    egress_dir: &std::path::Path,
) -> bool {
    path.starts_with(project_root) || path.starts_with(scratch_dir) || path.starts_with(egress_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_builder() -> SandboxBuilder {
        SandboxBuilder::new(
            "/tmp/tddy-builder-test",
            "/tmp/tddy-builder-test/.work",
            "/tmp/tddy-builder-test/out",
            vec!["/bin/echo".into(), "hi".into()],
        )
        .profile_path("/tmp/tddy-builder-test/profile.sb")
    }

    #[test]
    fn carries_a_declared_writable_mount_into_the_plan() {
        // Given
        let builder = a_builder().mount(MountSpec::read_write("/Users/me/proj"));

        // When
        let plan = builder.build().expect("plan must build");

        // Then
        assert_eq!(plan.mounts, vec![MountSpec::read_write("/Users/me/proj")]);
    }

    #[test]
    fn rejects_a_mount_with_a_relative_host_path() {
        // Given
        let builder = a_builder().mount(MountSpec::read_only("relative/proj"));

        // When
        let err = builder
            .build()
            .expect_err("relative mount host must be rejected");

        // Then
        assert!(
            err.to_string().contains("must be absolute"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn builds_a_plan_with_only_the_reads_the_caller_declared() {
        // Given — exactly one declared read, nothing implicit
        let builder = a_builder().read(ReadSpec::subpath("/usr/lib", ReadReason::SystemLibs));

        // When
        let plan = builder.build().expect("plan must build");

        // Then — the plan carries that read and no others
        assert_eq!(
            plan.reads,
            vec![ReadSpec::subpath("/usr/lib", ReadReason::SystemLibs)]
        );
    }

    /// An empty-host read (e.g. from a bare binary name whose parent path is "") must never survive
    /// into the plan: it renders as `(subpath "")` — rejected by macOS `sandbox-exec` — and, as an
    /// enclosing subpath, would shadow every other read in the allow-list (every path starts with
    /// "").
    #[test]
    fn drops_an_empty_host_read_and_does_not_let_it_shadow_others() {
        // Given — a real read plus a bogus empty-host exec subpath (as binary_exec_reads once emitted)
        let builder = a_builder()
            .read(ReadSpec::subpath("/usr/lib", ReadReason::SystemLibs))
            .read(ReadSpec::subpath("", ReadReason::BinaryDeps).executable());

        // When
        let plan = builder.build().expect("plan must build");

        // Then — the empty-host read is gone and the real read was not shadowed away
        assert!(
            plan.reads.iter().all(|r| !r.host.as_os_str().is_empty()),
            "empty-host read must be dropped: {:?}",
            plan.reads
        );
        assert!(
            plan.reads
                .iter()
                .any(|r| r.host == std::path::Path::new("/usr/lib")),
            "the real read must survive: {:?}",
            plan.reads
        );
    }

    #[test]
    fn deduplicates_reads_with_the_same_host_and_kind() {
        // Given — the same read declared twice
        let builder = a_builder()
            .read(ReadSpec::subpath("/usr/lib", ReadReason::SystemLibs))
            .read(ReadSpec::subpath("/usr/lib", ReadReason::Toolchain));

        // When
        let plan = builder.build().expect("plan must build");

        // Then — collapsed to a single rule
        assert_eq!(plan.reads.len(), 1);
        assert_eq!(plan.reads[0].host, PathBuf::from("/usr/lib"));
    }

    #[test]
    fn drops_a_read_shadowed_by_an_enclosing_subpath() {
        // Given — a child subpath fully contained by an enclosing subpath
        let builder = a_builder()
            .read(ReadSpec::subpath("/usr/lib", ReadReason::SystemLibs))
            .read(ReadSpec::subpath("/usr", ReadReason::SystemLibs));

        // When
        let plan = builder.build().expect("plan must build");

        // Then — only the enclosing subpath survives
        assert_eq!(
            plan.reads,
            vec![ReadSpec::subpath("/usr", ReadReason::SystemLibs)]
        );
    }

    #[test]
    fn rejects_a_copy_whose_destination_is_outside_the_writable_jail_tree() {
        // Given — a copy whose destination escapes the jail tree
        let builder = a_builder().copy(CopySpec {
            src: PathBuf::from("/etc/hosts"),
            dest: PathBuf::from("/etc/evil"),
            optional: false,
            mode: None,
        });

        // When
        let err = builder
            .build()
            .expect_err("copy outside the tree must be rejected");

        // Then
        assert!(
            err.to_string().contains("outside the writable jail tree"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn rejects_a_symlink_whose_link_is_outside_the_jail_tree() {
        // Given — a symlink whose link path escapes the jail tree
        let builder = a_builder().symlink(SymlinkSpec {
            link: PathBuf::from("/usr/local/bin/claude"),
            target: PathBuf::from("/tmp/tddy-builder-test/.work/home/.local/bin/claude"),
        });

        // When
        let err = builder
            .build()
            .expect_err("symlink outside the tree must be rejected");

        // Then
        assert!(
            err.to_string().contains("outside the writable jail tree"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn records_a_declared_secret_without_placing_its_value_in_the_env_map() {
        // Given — a secret declared alongside ordinary env vars
        let mut vars = BTreeMap::new();
        vars.insert(
            "HOME".to_string(),
            "/tmp/tddy-builder-test/.work/home".to_string(),
        );
        let builder = a_builder().env_map(vars).secret(
            "CLAUDE_CODE_OAUTH_TOKEN",
            SecretSource::Value("sk-ant-oat01-SECRET".to_string()),
        );

        // When
        let plan = builder.build().expect("plan must build");

        // Then — the secret is recorded, but its value never lands in the env var map
        assert_eq!(
            plan.env.secrets,
            vec![SecretSpec {
                env_name: "CLAUDE_CODE_OAUTH_TOKEN".to_string(),
                source: SecretSource::Value("sk-ant-oat01-SECRET".to_string()),
            }]
        );
        assert!(
            plan.env
                .vars
                .values()
                .all(|v| !v.contains("sk-ant-oat01-SECRET")),
            "secret value must not appear in env vars: {:?}",
            plan.env.vars
        );
    }
}
