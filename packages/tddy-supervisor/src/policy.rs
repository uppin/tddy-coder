//! Resolving a request against root-owned policy.
//!
//! Everything here is pure but one thing: [`resolve_mount_source`] has the kernel resolve the source
//! it is about to approve, because a mount root is a *prefix* over a tree the session user writes
//! into and no amount of string comparison can tell a directory from a symlink to `/`. Its own docs
//! carry the argument.
//!
//! This is the security boundary, so it denies by default: a value that is not explicitly permitted
//! is refused, and a refusal is opaque.
//!
//! The distinction between [`SupervisorError::Denied`] and [`SupervisorError::Invalid`] matters.
//! `Denied` covers anything that could otherwise be used as an existence oracle — "is there a user
//! called `alice`?", "does `/opt/secret/tool` exist?". `Invalid` covers requests that are
//! malformed on their face, where a precise message reveals nothing and saves an operator an hour.

use std::collections::BTreeMap;
use std::fmt;
use std::os::fd::{AsRawFd, OwnedFd};
use std::path::{Component, Path, PathBuf};
use std::str::FromStr;

use crate::config::spawn_policy::is_loader_env_key;
use crate::config::spawn_policy::{ExecRoot, SpawnPolicy};
use crate::config::{CgroupPolicy, ROOT_USER};
use crate::error::SupervisorError;
use crate::request::{AppliedLimits, RequestedLimits};
use crate::spawn_broker;
use crate::spawn_broker::spawn_plan::EnvironmentBase;
use crate::users_file::UsersFile;

/// The kernel's `cpu.max` pair: `"<quota_us> <period_us>"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuMax {
    pub quota_us: u64,
    pub period_us: u64,
}

impl FromStr for CpuMax {
    type Err = SupervisorError;

    fn from_str(text: &str) -> Result<CpuMax, SupervisorError> {
        let malformed = || SupervisorError::Invalid {
            message: format!("cpu.max must be `<quota_us> <period_us>`, got `{text}`"),
        };

        let mut fields = text.split_whitespace();
        let (Some(quota), Some(period), None) = (fields.next(), fields.next(), fields.next())
        else {
            return Err(malformed());
        };
        let quota_us = quota.parse::<u64>().map_err(|_| malformed())?;
        let period_us = period.parse::<u64>().map_err(|_| malformed())?;

        if period_us == 0 {
            return Err(SupervisorError::Invalid {
                message: "cpu.max period must be greater than zero".to_string(),
            });
        }
        Ok(CpuMax {
            quota_us,
            period_us,
        })
    }
}

impl fmt::Display for CpuMax {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.quota_us, self.period_us)
    }
}

/// Resolve the OS user a session may run as from the mapping in force (`#agent-tools` 31/32): one
/// `users.yaml` names, never `root`.
///
/// `root` is refused even if a mapping somehow named it — the loader rejects that too, and a
/// privilege boundary is worth checking twice.
pub fn resolve_mapped_session_user(
    users: &UsersFile,
    requested: &str,
) -> Result<String, SupervisorError> {
    if requested == ROOT_USER {
        return Err(SupervisorError::Denied);
    }
    if users.os_users().iter().any(|mapped| mapped == requested) {
        Ok(requested.to_string())
    } else {
        Err(SupervisorError::Denied)
    }
}

/// A binary the exec gate passed, and what to exec so that it is that binary that runs.
///
/// On Linux the gate keeps an `O_PATH` descriptor of the file it checked, and the child execs
/// `/proc/self/fd/<n>`: a file swapped in at the same path between the check and the exec is not
/// what runs. macOS has no exec through a descriptor, so there the check is path-only and
/// [`Self::exec_path`] is the resolved path.
#[derive(Debug, Clone)]
pub struct CheckedExecutable {
    /// The file the requested path resolved to — a symlink is judged by its target.
    path: PathBuf,
    /// The checked file, held open (Linux only).
    descriptor: Option<std::sync::Arc<OwnedFd>>,
}

impl CheckedExecutable {
    /// The file the gate checked: the requested path's target, never a symlink.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The descriptor [`Self::exec_path`] names, when the child execs through one (Linux).
    ///
    /// The child must clear its `FD_CLOEXEC` before the exec: for a script, the kernel hands the
    /// interpreter `/proc/self/fd/<n>` as the file to read, and a descriptor closed by the exec
    /// leaves the interpreter nothing to open.
    pub fn exec_descriptor(&self) -> Option<std::os::fd::RawFd> {
        self.descriptor
            .as_ref()
            .map(|descriptor| descriptor.as_raw_fd())
    }

    /// What the child execs: the checked descriptor on Linux, the checked path elsewhere.
    pub fn exec_path(&self) -> PathBuf {
        match &self.descriptor {
            Some(descriptor) => PathBuf::from(format!("/proc/self/fd/{}", descriptor.as_raw_fd())),
            None => self.path.clone(),
        }
    }
}

/// Resolve the binary any spawn — session, sandbox, streamed or relayed — may exec.
///
/// The requested path is plain and absolute, and is judged by the file it resolves to: that file
/// must be named by an `exec_roots` file entry or lie under a directory entry, and the file, every
/// directory above it and the matching entry must each pass [`is_trusted_exec_path_entry`] — a sticky
/// world-writable directory such as `/tmp` included — except that an entry the operator marked
/// `trusted_prefix` is not itself checked (directories above and below it still are). The reason
/// for a refusal is logged; the caller
/// gets the opaque [`SupervisorError::Denied`].
pub fn resolve_exec(
    policy: &SpawnPolicy,
    requested: &Path,
) -> Result<CheckedExecutable, SupervisorError> {
    let requested = plain_absolute_path(requested)?;
    // A symlink is judged by its target: the kernel resolves the whole path.
    let target = std::fs::canonicalize(requested)
        .map_err(|error| refused_exec(requested, &format!("it does not resolve ({error})")))?;
    // SAFETY: reads this process's own effective uid and nothing else.
    let enforcing_uid = unsafe { libc::geteuid() };
    if !policy
        .exec_roots
        .iter()
        .any(|entry| exec_root_admits(entry, &target, enforcing_uid))
    {
        return Err(refused_exec(
            requested,
            &format!(
                "{} is under no exec_roots entry it and every directory above it are trusted by",
                target.display()
            ),
        ));
    }
    Ok(CheckedExecutable {
        descriptor: checked_descriptor(&target)?,
        path: target,
    })
}

/// Whether `entry` names `target` (a file entry) or a directory above it, and the file, every
/// directory above it and the entry itself are trusted — but for an operator-marked trusted
/// prefix, the one entry not itself checked.
fn exec_root_admits(entry: &ExecRoot, target: &Path, enforcing_uid: u32) -> bool {
    let Ok(entry_target) = std::fs::canonicalize(&entry.path) else {
        return false;
    };
    let contained = if entry_target.is_file() {
        target == entry_target
    } else {
        target.starts_with(&entry_target)
    };
    contained
        && target.ancestors().all(|path| {
            (entry.trusted_prefix && path == entry_target)
                || is_trusted_on_disk(path, enforcing_uid)
        })
}

/// [`is_trusted_exec_path_entry`] for what is on disk at `path`; a path that cannot be read is not.
fn is_trusted_on_disk(path: &Path, enforcing_uid: u32) -> bool {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path).is_ok_and(|metadata| {
        is_trusted_exec_path_entry(metadata.uid(), metadata.mode(), enforcing_uid)
    })
}

/// The checked file held open, so that what the child execs is what was checked (Linux).
#[cfg(target_os = "linux")]
fn checked_descriptor(target: &Path) -> Result<Option<std::sync::Arc<OwnedFd>>, SupervisorError> {
    let c_path = spawn_broker::path_to_c_string(target).map_err(|_| SupervisorError::Denied)?;
    let fd =
        spawn_broker::open_resolved(libc::AT_FDCWD, &c_path, spawn_broker::RESOLVE_NO_SYMLINKS)
            .map_err(|error| refused_exec(target, &format!("it could not be opened ({error})")))?;
    Ok(Some(std::sync::Arc::new(fd)))
}

/// macOS has no exec through a descriptor: the check is path-only.
#[cfg(not(target_os = "linux"))]
fn checked_descriptor(_target: &Path) -> Result<Option<std::sync::Arc<OwnedFd>>, SupervisorError> {
    Ok(None)
}

/// Log why the exec gate refused `requested`, and return the opaque refusal the caller gets.
fn refused_exec(requested: &Path, why: &str) -> SupervisorError {
    log::warn!(
        target: "tddy_supervisor::policy",
        "spawn refused: {} is not an executable the exec gate passes: {why}",
        requested.display()
    );
    SupervisorError::Denied
}

/// Whether one file or directory on the way to a binary is one only root, or the supervisor's own
/// `enforcing_uid`, could have written: owned by one of them, and writable by neither its group nor
/// everyone. Unlike the policy file's parents, a sticky world-writable directory is **not** trusted.
pub fn is_trusted_exec_path_entry(owner_uid: u32, mode: u32, enforcing_uid: u32) -> bool {
    (owner_uid == 0 || owner_uid == enforcing_uid) && mode & 0o022 == 0
}

/// Which kind of spawn an environment is resolved for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpawnKind {
    Session,
    Sandbox,
    Streamed,
    Relayed,
}

/// The environment a spawn is started with: the daemon's variables, and what they are laid over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpawnEnvironment {
    pub variables: BTreeMap<String, String>,
    pub base: EnvironmentBase,
}

/// The one environment rule, for every spawn kind.
///
/// The env the daemon computed passes whole: the supervisor holds no list of keys, because what a
/// child needs is the daemon's configuration. A loader variable ([`is_loader_env_key`]) is refused,
/// because it chooses what code runs. `HOME`/`USER`/`LOGNAME` are forced to the target account, and
/// `LISTEN_*` and `TDDY_SUPERVISOR_SOCKET` stripped, when the child's environment is built. The base
/// is [`EnvironmentBase::Minimal`] for sessions, sandboxes and streamed spawns, and
/// [`EnvironmentBase::Exactly`] for a relayed spawn.
pub fn resolve_spawn_env(
    kind: SpawnKind,
    requested: &BTreeMap<String, String>,
) -> Result<SpawnEnvironment, SupervisorError> {
    for key in requested.keys() {
        if is_loader_env_key(key) {
            log::warn!(
                target: "tddy_supervisor::policy",
                "spawn refused: `{key}` chooses what code the session loads and is never a \
                 caller's to set"
            );
            return Err(SupervisorError::Denied);
        }
    }
    Ok(SpawnEnvironment {
        variables: requested.clone(),
        base: match kind {
            SpawnKind::Relayed => EnvironmentBase::Exactly,
            SpawnKind::Session | SpawnKind::Sandbox | SpawnKind::Streamed => {
                EnvironmentBase::Minimal
            }
        },
    })
}

/// Resolve a bind-mount source for a sandbox. Must sit under one of the permitted roots, and must
/// still be under it once the kernel has resolved it.
pub fn resolve_mount_source(
    policy: &SpawnPolicy,
    requested: &Path,
) -> Result<PathBuf, SupervisorError> {
    let requested = plain_absolute_path(requested)?;
    // `starts_with` compares whole path components, so `/srv/tddy/repos-backup` is not under
    // `/srv/tddy/repos` — a string prefix test would let it through.
    let root = policy
        .allowed_mount_roots
        .iter()
        .find(|root| requested.starts_with(root))
        .ok_or(SupervisorError::Denied)?;
    refuse_a_source_the_kernel_will_not_resolve_beneath(root, requested)?;
    Ok(requested.to_path_buf())
}

/// Refuse a source that is only *textually* beneath its root.
///
/// Component-wise containment plus a ban on `..` is exactly right for an allowlist of paths only
/// root can write. It does not carry over to a mount root,
/// which is a prefix over a whole tree the session user owns: `alice` can create
/// `/srv/tddy/repos/alice/esc -> /`, name it as a source, pass a component-wise check with it, and
/// have `mount(2)` follow the link and bind `/` into her jail.
///
/// So the kernel resolves it, once, with `RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS`: a symlink anywhere
/// in the path is `ELOOP`, and a resolution that would leave the root is `EXDEV`. `spawn_broker`
/// resolves the same path the same way when it opens the descriptor the child actually binds, so what
/// is mounted is the object that was checked rather than the same name walked a second time.
fn refuse_a_source_the_kernel_will_not_resolve_beneath(
    root: &Path,
    requested: &Path,
) -> Result<(), SupervisorError> {
    let root_path = spawn_broker::path_to_c_string(root).map_err(|_| SupervisorError::Denied)?;
    let beneath = requested
        .strip_prefix(root)
        .map_err(|_| SupervisorError::Denied)?;
    // `openat2` has no empty path, and the root is `.` relative to itself.
    let beneath = if beneath.as_os_str().is_empty() {
        Path::new(".")
    } else {
        beneath
    };
    let beneath = spawn_broker::path_to_c_string(beneath).map_err(|_| SupervisorError::Denied)?;

    containment_of(
        spawn_broker::open_directory_reference(&root_path).and_then(|root| {
            spawn_broker::open_resolved(
                root.as_raw_fd(),
                &beneath,
                spawn_broker::RESOLVE_BENEATH | spawn_broker::RESOLVE_NO_SYMLINKS,
            )
        }),
    )
}

/// What the kernel's answer to that resolution means for the request.
fn containment_of(resolved: std::io::Result<OwnedFd>) -> Result<(), SupervisorError> {
    match resolved {
        // Resolved whole, without following a symlink and without leaving the root: this source is
        // the object its name says it is.
        Ok(_) => Ok(()),
        // Nothing exists yet for a symlink to point at, and a denial that meant "no such directory"
        // would send an operator looking for a policy mistake instead of a typo. The descriptor the
        // child binds is resolved the same way, so a source created as a symlink in between is
        // refused there rather than bound.
        Err(error) if error.raw_os_error() == Some(libc::ENOENT) => Ok(()),
        // Everything else is a resolution the kernel refused to perform — a symlink (`ELOOP`), an
        // escape (`EXDEV`), or a kernel with no `openat2` to ask (`ENOSYS`). A source that cannot be
        // shown to be contained is not granted: the path-based check that would be the alternative is
        // the race this exists to remove.
        Err(_) => Err(SupervisorError::Denied),
    }
}

/// Require an absolute path with no `..` in it.
///
/// A `..` is refused rather than normalized. Normalizing would either need the filesystem — making
/// the decision depend on what is on disk at this instant, so a symlink swapped in before `exec`
/// changes the answer — or reimplement path resolution, which is where traversal bugs live.
pub(crate) fn plain_absolute_path(path: &Path) -> Result<&Path, SupervisorError> {
    let traversal_free = path
        .components()
        .all(|component| !matches!(component, Component::ParentDir));
    if path.is_absolute() && traversal_free {
        Ok(path)
    } else {
        Err(SupervisorError::Denied)
    }
}

/// Clamp a caller's requested limits to the policy ceilings.
///
/// Two rules that are easy to get backwards:
/// * A request above a ceiling is **lowered to the ceiling**, not rejected — a session that asks
///   for too much should run smaller, not fail to start.
/// * A request that names *no* limit still gets the ceiling. Omitting a field must not be a way to
///   opt out of the limit.
pub fn clamp_limits(
    policy: &CgroupPolicy,
    requested: &RequestedLimits,
) -> Result<AppliedLimits, SupervisorError> {
    Ok(AppliedLimits {
        memory_max: clamp_scalar(requested.memory_max, policy.memory_max_ceiling),
        cpu_max: clamp_cpu(
            requested.cpu_max.as_deref(),
            policy.cpu_max_ceiling.as_deref(),
        )?,
        pids_max: clamp_scalar(requested.pids_max, policy.pids_max_ceiling),
    })
}

fn clamp_scalar(requested: Option<u64>, ceiling: Option<u64>) -> Option<u64> {
    match (requested, ceiling) {
        (Some(requested), Some(ceiling)) => Some(requested.min(ceiling)),
        (Some(requested), None) => Some(requested),
        (None, ceiling) => ceiling,
    }
}

fn clamp_cpu(
    requested: Option<&str>,
    ceiling: Option<&str>,
) -> Result<Option<String>, SupervisorError> {
    let requested = requested.map(CpuMax::from_str).transpose()?;
    let ceiling = ceiling.map(CpuMax::from_str).transpose()?;

    match (requested, ceiling) {
        (Some(requested), Some(ceiling)) => {
            // Quotas are only comparable within the same period. Clamping across periods would
            // silently hand the caller a different share of the cpu than the ceiling describes.
            if requested.period_us != ceiling.period_us {
                return Err(SupervisorError::Invalid {
                    message: format!(
                        "cpu.max period {} does not match the policy period {}",
                        requested.period_us, ceiling.period_us
                    ),
                });
            }
            Ok(Some(
                CpuMax {
                    quota_us: requested.quota_us.min(ceiling.quota_us),
                    period_us: ceiling.period_us,
                }
                .to_string(),
            ))
        }
        (Some(requested), None) => Ok(Some(requested.to_string())),
        (None, ceiling) => Ok(ceiling.map(|ceiling| ceiling.to_string())),
    }
}

/// Directory a named scope lives in, under the supervisor's delegated base.
///
/// Deterministic, because `DestroyScope` addresses a scope by the same name that created it.
pub fn scope_dir(base: &Path, name: &str) -> Result<PathBuf, SupervisorError> {
    // A scope name names a directory immediately under the delegated base, so it has to be exactly
    // one ordinary component. Anything else — a separator, a `..`, an empty name — could address a
    // directory outside the subtree the supervisor owns.
    let mut components = Path::new(name).components();
    let single = match (components.next(), components.next()) {
        (Some(Component::Normal(single)), None) => single,
        _ => return Err(SupervisorError::Denied),
    };
    if single.to_str() != Some(name) {
        return Err(SupervisorError::Denied);
    }
    Ok(base.join(format!("tddy-{name}.scope")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::spawn_policy::ExecRoot;
    use crate::test_util::{
        a_cgroup_policy, a_spawn_policy, requesting_cpu, requesting_memory, requesting_pids,
        unlimited, MIB,
    };

    // -----------------------------------------------------------------------------------------
    // Session users
    // -----------------------------------------------------------------------------------------

    fn a_mapping_of(os_users: &[&str]) -> UsersFile {
        let yaml: String = os_users
            .iter()
            .enumerate()
            .map(|(n, os_user)| format!("- {{github_user: login{n}, os_user: {os_user}}}\n"))
            .collect();
        UsersFile::from_yaml(&yaml).expect("a valid mapping")
    }

    #[test]
    fn a_session_may_run_only_as_an_os_user_the_mapping_names() {
        // Given users.yaml maps alice and bob
        let users = a_mapping_of(&["alice", "bob"]);

        // When the daemon asks for a session as each of them, and as somebody unmapped
        let answers = (
            resolve_mapped_session_user(&users, "bob"),
            resolve_mapped_session_user(&users, "mallory"),
        );

        // Then only the mapped user is granted
        assert_eq!(
            answers,
            (Ok("bob".to_string()), Err(SupervisorError::Denied))
        );
    }

    #[test]
    fn a_session_as_root_is_refused_whatever_the_mapping_says() {
        // Given an empty mapping — a mapping naming root does not load at all
        let users = UsersFile::default();

        // Then
        assert_eq!(
            resolve_mapped_session_user(&users, "root"),
            Err(SupervisorError::Denied)
        );
    }

    #[test]
    fn accepts_a_session_user_that_the_mapping_lists() {
        // Given
        let users = a_mapping_of(&["alice", "bob"]);

        // When
        let resolved = resolve_mapped_session_user(&users, "bob");

        // Then
        assert_eq!(resolved, Ok("bob".to_string()));
    }

    #[test]
    fn denies_a_session_user_that_the_mapping_does_not_list() {
        // Given
        let users = a_mapping_of(&["alice"]);

        // When
        let resolved = resolve_mapped_session_user(&users, "mallory");

        // Then
        assert_eq!(resolved, Err(SupervisorError::Denied));
    }

    #[test]
    fn denies_every_session_user_when_the_mapping_lists_none() {
        // Given
        let users = UsersFile::default();

        // When
        let resolved = resolve_mapped_session_user(&users, "alice");

        // Then
        assert_eq!(resolved, Err(SupervisorError::Denied));
    }

    #[test]
    fn denies_root_as_a_session_user_even_when_the_mapping_lists_it() {
        // Given a mapping naming root, which the loader refuses — built here so the guard is
        // pinned even against a mapping that bypassed it.
        let users = a_mapping_of(&["alice"]);

        // When
        let resolved = resolve_mapped_session_user(&users, "root");

        // Then — the whole point of the supervisor is that nothing it spawns is root.
        assert_eq!(resolved, Err(SupervisorError::Denied));
    }

    // -----------------------------------------------------------------------------------------
    // Mount sources
    // -----------------------------------------------------------------------------------------

    /// Every test that reaches [`refuse_a_source_the_kernel_will_not_resolve_beneath`] — the six
    /// below — asserts what `openat2(2)` answers, and that syscall is Linux 5.6's. Off Linux
    /// `spawn_broker::open_resolved` refuses instead, so a mount source is denied for want of a
    /// kernel rather than on its merits: an accepting test would fail and a denying one would pass
    /// without having tested anything. The path-based rules around it are asserted on every host.
    #[cfg(target_os = "linux")]
    #[test]
    fn accepts_a_mount_source_under_an_allowed_root() {
        // Given
        let policy = a_spawn_policy()
            .allowing_mount_root("/srv/tddy/repos")
            .build();

        // When
        let resolved = resolve_mount_source(&policy, Path::new("/srv/tddy/repos/alice/project"));

        // Then
        assert_eq!(resolved, Ok(PathBuf::from("/srv/tddy/repos/alice/project")));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn accepts_an_allowed_root_itself_as_a_mount_source() {
        // Given
        let policy = a_spawn_policy()
            .allowing_mount_root("/srv/tddy/repos")
            .build();

        // When
        let resolved = resolve_mount_source(&policy, Path::new("/srv/tddy/repos"));

        // Then
        assert_eq!(resolved, Ok(PathBuf::from("/srv/tddy/repos")));
    }

    #[test]
    fn denies_a_mount_source_outside_every_allowed_root() {
        // Given
        let policy = a_spawn_policy()
            .allowing_mount_root("/srv/tddy/repos")
            .build();

        // When
        let resolved = resolve_mount_source(&policy, Path::new("/etc/shadow"));

        // Then
        assert_eq!(resolved, Err(SupervisorError::Denied));
    }

    #[test]
    fn denies_a_mount_source_that_only_shares_a_prefix_with_an_allowed_root() {
        // Given
        let policy = a_spawn_policy()
            .allowing_mount_root("/srv/tddy/repos")
            .build();

        // When
        let resolved = resolve_mount_source(&policy, Path::new("/srv/tddy/repos-backup/secrets"));

        // Then — containment is by path component, not by string prefix.
        assert_eq!(resolved, Err(SupervisorError::Denied));
    }

    /// A mount root that really exists, so the kernel has something to resolve against.
    ///
    /// The escape tests need a directory a session user could write into, and there is no way to
    /// assert a symlink is refused without a symlink on disk to refuse.
    #[cfg(target_os = "linux")]
    fn a_real_mount_root() -> tempfile::TempDir {
        tempfile::tempdir().expect("a mount root on disk")
    }

    #[cfg(target_os = "linux")]
    fn policy_allowing(root: &Path) -> SpawnPolicy {
        a_spawn_policy()
            .allowing_mount_root(root.to_str().expect("a utf-8 mount root"))
            .build()
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn accepts_a_mount_source_the_kernel_resolves_inside_its_allowed_root() {
        // Given
        let root = a_real_mount_root();
        let project = root.path().join("alice").join("project");
        std::fs::create_dir_all(&project).expect("create the source directory");

        // When
        let resolved = resolve_mount_source(&policy_allowing(root.path()), &project);

        // Then
        assert_eq!(resolved, Ok(project));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn denies_a_mount_source_that_leaves_its_allowed_root_through_a_symlink() {
        // Given the escape a session user can plant for herself: the mount roots are trees she
        // writes into, so containment by path component says nothing about where the path leads.
        let root = a_real_mount_root();
        let escape = root.path().join("esc");
        std::os::unix::fs::symlink("/", &escape).expect("plant the escape");

        // When
        let resolved = resolve_mount_source(&policy_allowing(root.path()), &escape);

        // Then — component-wise this *is* under the allowed root, and `mount(2)` would follow it and
        // bind `/` into the jail.
        assert_eq!(resolved, Err(SupervisorError::Denied));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn denies_a_mount_source_whose_parent_directory_is_a_symlink_out_of_the_root() {
        // Given
        let root = a_real_mount_root();
        std::os::unix::fs::symlink("/etc", root.path().join("esc")).expect("plant the escape");

        // When
        let resolved = resolve_mount_source(
            &policy_allowing(root.path()),
            &root.path().join("esc/shadow"),
        );

        // Then — every component is resolved, not just the last one.
        assert_eq!(resolved, Err(SupervisorError::Denied));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn accepts_a_mount_source_that_does_not_exist_yet_and_leaves_the_bind_to_refuse_it() {
        // Given
        let root = a_real_mount_root();
        let absent = root.path().join("not-created-yet");

        // When
        let resolved = resolve_mount_source(&policy_allowing(root.path()), &absent);

        // Then — a policy denial that meant "no such directory" would send an operator looking for a
        // policy mistake instead of a typo, and there is nothing here for a symlink to point at yet.
        // The descriptor the child actually binds is resolved the same way, so a source created as a
        // symlink between now and then is refused there.
        assert_eq!(resolved, Ok(absent));
    }

    #[test]
    fn denies_every_mount_source_on_a_kernel_that_cannot_resolve_one_without_a_symlink_race() {
        // Given the answer a kernel older than 5.6 gives to `openat2`
        let unsupported = Err(std::io::Error::from_raw_os_error(libc::ENOSYS));

        // When
        let contained = containment_of(unsupported);

        // Then — falling back to the path-based check would be falling back to the race this
        // resolution exists to remove.
        assert_eq!(contained, Err(SupervisorError::Denied));
    }

    #[test]
    fn denies_a_mount_source_that_escapes_its_allowed_root_with_a_traversal() {
        // Given
        let policy = a_spawn_policy()
            .allowing_mount_root("/srv/tddy/repos")
            .build();

        // When
        let resolved = resolve_mount_source(&policy, Path::new("/srv/tddy/repos/../../../etc"));

        // Then
        assert_eq!(resolved, Err(SupervisorError::Denied));
    }

    // -----------------------------------------------------------------------------------------
    // Limit clamping
    // -----------------------------------------------------------------------------------------

    #[test]
    fn passes_through_a_request_that_sits_under_every_ceiling() {
        // Given
        let policy = a_cgroup_policy()
            .with_memory_ceiling(512 * MIB)
            .with_cpu_ceiling("400000 100000")
            .with_pids_ceiling(512)
            .build();
        let requested = RequestedLimits {
            memory_max: Some(128 * MIB),
            cpu_max: Some("200000 100000".to_string()),
            pids_max: Some(64),
        };

        // When
        let applied = clamp_limits(&policy, &requested);

        // Then
        assert_eq!(
            applied,
            Ok(AppliedLimits {
                memory_max: Some(128 * MIB),
                cpu_max: Some("200000 100000".to_string()),
                pids_max: Some(64),
            })
        );
    }

    #[test]
    fn clamps_a_memory_request_down_to_the_ceiling() {
        // Given
        let policy = a_cgroup_policy().with_memory_ceiling(64 * MIB).build();

        // When
        let applied = clamp_limits(&policy, &requesting_memory(256 * MIB));

        // Then
        assert_eq!(
            applied,
            Ok(AppliedLimits {
                memory_max: Some(64 * MIB),
                cpu_max: None,
                pids_max: None,
            })
        );
    }

    #[test]
    fn clamps_a_cpu_quota_down_to_the_ceiling() {
        // Given
        let policy = a_cgroup_policy().with_cpu_ceiling("100000 100000").build();

        // When
        let applied = clamp_limits(&policy, &requesting_cpu("400000 100000"));

        // Then
        assert_eq!(
            applied,
            Ok(AppliedLimits {
                memory_max: None,
                cpu_max: Some("100000 100000".to_string()),
                pids_max: None,
            })
        );
    }

    #[test]
    fn clamps_a_pids_request_down_to_the_ceiling() {
        // Given
        let policy = a_cgroup_policy().with_pids_ceiling(64).build();

        // When
        let applied = clamp_limits(&policy, &requesting_pids(512));

        // Then
        assert_eq!(
            applied,
            Ok(AppliedLimits {
                memory_max: None,
                cpu_max: None,
                pids_max: Some(64),
            })
        );
    }

    #[test]
    fn applies_every_ceiling_when_the_caller_requests_no_limit_at_all() {
        // Given
        let policy = a_cgroup_policy()
            .with_memory_ceiling(64 * MIB)
            .with_cpu_ceiling("100000 100000")
            .with_pids_ceiling(64)
            .build();

        // When
        let applied = clamp_limits(&policy, &unlimited());

        // Then — omitting a field must not be a way to escape the limit.
        assert_eq!(
            applied,
            Ok(AppliedLimits {
                memory_max: Some(64 * MIB),
                cpu_max: Some("100000 100000".to_string()),
                pids_max: Some(64),
            })
        );
    }

    #[test]
    fn leaves_a_limit_unset_when_neither_the_caller_nor_the_policy_names_one() {
        // Given
        let policy = a_cgroup_policy().build();

        // When
        let applied = clamp_limits(&policy, &unlimited());

        // Then
        assert_eq!(applied, Ok(AppliedLimits::default()));
    }

    #[test]
    fn honors_a_request_when_the_policy_sets_no_ceiling_for_it() {
        // Given a policy that caps memory but says nothing about pids.
        let policy = a_cgroup_policy().with_memory_ceiling(64 * MIB).build();

        // When
        let applied = clamp_limits(&policy, &requesting_pids(512));

        // Then
        assert_eq!(
            applied,
            Ok(AppliedLimits {
                memory_max: Some(64 * MIB),
                cpu_max: None,
                pids_max: Some(512),
            })
        );
    }

    #[test]
    fn rejects_a_cpu_request_whose_period_differs_from_the_ceiling_period() {
        // Given
        let policy = a_cgroup_policy().with_cpu_ceiling("100000 100000").build();

        // When
        let applied = clamp_limits(&policy, &requesting_cpu("100000 50000"));

        // Then — comparing quotas across different periods would silently double the caller's cpu
        // share, so the request is refused instead of guessed at.
        assert_eq!(
            applied,
            Err(SupervisorError::Invalid {
                message: "cpu.max period 50000 does not match the policy period 100000".to_string()
            })
        );
    }

    #[test]
    fn rejects_a_cpu_request_that_is_not_a_quota_and_a_period() {
        // Given
        let policy = a_cgroup_policy().with_cpu_ceiling("100000 100000").build();

        // When
        let applied = clamp_limits(&policy, &requesting_cpu("plenty"));

        // Then
        assert_eq!(
            applied,
            Err(SupervisorError::Invalid {
                message: "cpu.max must be `<quota_us> <period_us>`, got `plenty`".to_string()
            })
        );
    }

    // -----------------------------------------------------------------------------------------
    // cpu.max parsing
    // -----------------------------------------------------------------------------------------

    #[test]
    fn parses_a_cpu_max_quota_and_period() {
        // Given / When
        let parsed: CpuMax = "200000 100000".parse().expect("parse cpu.max");

        // Then
        assert_eq!(
            parsed,
            CpuMax {
                quota_us: 200_000,
                period_us: 100_000
            }
        );
    }

    #[test]
    fn renders_a_cpu_max_back_in_the_kernel_format() {
        // Given
        let cpu_max = CpuMax {
            quota_us: 200_000,
            period_us: 100_000,
        };

        // When / Then
        assert_eq!(cpu_max.to_string(), "200000 100000");
    }

    #[test]
    fn rejects_a_cpu_max_with_a_single_field() {
        // Given / When
        let parsed = "200000".parse::<CpuMax>();

        // Then
        assert_eq!(
            parsed,
            Err(SupervisorError::Invalid {
                message: "cpu.max must be `<quota_us> <period_us>`, got `200000`".to_string()
            })
        );
    }

    #[test]
    fn rejects_a_cpu_max_with_a_zero_period() {
        // Given / When
        let parsed = "200000 0".parse::<CpuMax>();

        // Then — a zero period is a division by zero for every downstream comparison.
        assert_eq!(
            parsed,
            Err(SupervisorError::Invalid {
                message: "cpu.max period must be greater than zero".to_string()
            })
        );
    }

    // -----------------------------------------------------------------------------------------
    // Scope directories
    // -----------------------------------------------------------------------------------------

    #[test]
    fn builds_a_scope_directory_under_the_delegated_base() {
        // Given / When
        let dir = scope_dir(Path::new("/sys/fs/cgroup/tddy.slice"), "session-alpha");

        // Then
        assert_eq!(
            dir,
            Ok(PathBuf::from(
                "/sys/fs/cgroup/tddy.slice/tddy-session-alpha.scope"
            ))
        );
    }

    #[test]
    fn rejects_a_scope_name_containing_a_path_separator() {
        // Given / When
        let dir = scope_dir(Path::new("/sys/fs/cgroup"), "alice/../../escape");

        // Then
        assert_eq!(dir, Err(SupervisorError::Denied));
    }

    #[test]
    fn rejects_a_scope_name_that_is_a_parent_directory_reference() {
        // Given / When
        let dir = scope_dir(Path::new("/sys/fs/cgroup"), "..");

        // Then
        assert_eq!(dir, Err(SupervisorError::Denied));
    }

    #[test]
    fn rejects_an_empty_scope_name() {
        // Given / When
        let dir = scope_dir(Path::new("/sys/fs/cgroup"), "");

        // Then
        assert_eq!(dir, Err(SupervisorError::Denied));
    }

    // -----------------------------------------------------------------------------------------
    // Exec roots (`#agent-tools` 32/32)
    //
    // The account running the tests stands in for root, as `config.rs`'s trust tests do: the gate
    // trusts uid 0 or the supervisor's own euid. The tree lives under the build's `target/tmp`,
    // never `/tmp` — a sticky world-writable directory is not trusted, so a binary there is refused.
    // -----------------------------------------------------------------------------------------

    /// A directory tree only the account running the tests can write, all the way up.
    struct AnExecTree {
        _workspace: tempfile::TempDir,
        top: PathBuf,
    }

    fn an_exec_tree() -> AnExecTree {
        use std::os::unix::fs::PermissionsExt;
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp/exec-roots");
        std::fs::create_dir_all(&base).expect("create the exec-roots test base");
        let workspace = tempfile::tempdir_in(&base).expect("create an exec tree");
        // Resolved, because the gate judges a path by what it resolves to: no `..`, no `/var` link.
        let top = workspace
            .path()
            .canonicalize()
            .expect("resolve the exec tree");
        std::fs::set_permissions(&top, std::fs::Permissions::from_mode(0o755))
            .expect("make the exec tree readable");
        AnExecTree {
            _workspace: workspace,
            top,
        }
    }

    impl AnExecTree {
        /// A directory under the tree with `mode`.
        fn directory(&self, name: &str, mode: u32) -> PathBuf {
            use std::os::unix::fs::PermissionsExt;
            let path = self.top.join(name);
            std::fs::create_dir_all(&path).expect("create a directory in the exec tree");
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode))
                .expect("set the directory's mode");
            path
        }

        /// An executable script at `relative` saying `words`, with `mode`.
        fn tool(&self, relative: &str, words: &str, mode: u32) -> PathBuf {
            use std::os::unix::fs::PermissionsExt;
            let path = self.top.join(relative);
            std::fs::write(&path, format!("#!/bin/sh\necho {words}\n")).expect("write a tool");
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode))
                .expect("set the tool's mode");
            path
        }
    }

    fn a_policy_with_exec_roots(entries: &[&Path]) -> SpawnPolicy {
        SpawnPolicy {
            exec_roots: entries.iter().map(|entry| ExecRoot::at(*entry)).collect(),
            ..SpawnPolicy::default()
        }
    }

    fn a_policy_trusting_the_prefix(prefix: &Path) -> SpawnPolicy {
        SpawnPolicy {
            exec_roots: vec![ExecRoot::trusted_prefix(prefix)],
            ..SpawnPolicy::default()
        }
    }

    /// A store laid out like `/nix/store`: the store directory group-writable and sticky, each
    /// package below it a plain directory.
    fn a_store_like_nix_in(tree: &AnExecTree, package_mode: u32) -> (PathBuf, PathBuf) {
        let store = tree.directory("store", 0o1775);
        tree.directory("store/pkg-git", package_mode);
        let git = tree.tool("store/pkg-git/git", "git", 0o755);
        (store, git)
    }

    #[test]
    fn a_binary_under_a_trusted_prefix_root_whose_own_directory_is_group_writable_and_sticky_runs()
    {
        // Given an operator-marked trusted prefix, group-writable and sticky like /nix/store
        let tree = an_exec_tree();
        let (store, git) = a_store_like_nix_in(&tree, 0o755);

        // When
        let checked = the_checked_path(&a_policy_trusting_the_prefix(&store), &git);

        // Then — its own mode is the operator's call; everything below it passed
        assert_eq!(checked, Ok(git));
    }

    #[test]
    fn the_same_store_root_without_the_trusted_prefix_mark_is_refused() {
        // Given the same store, listed as a plain root
        let tree = an_exec_tree();
        let (store, git) = a_store_like_nix_in(&tree, 0o755);

        // Then — only the operator can vouch for a writable directory, per entry
        assert_eq!(
            the_checked_path(&a_policy_with_exec_roots(&[&store]), &git),
            Err(SupervisorError::Denied)
        );
    }

    #[test]
    fn a_group_writable_directory_below_a_trusted_prefix_is_still_refused() {
        // Given a trusted prefix with a group-writable package directory below it
        let tree = an_exec_tree();
        let (store, git) = a_store_like_nix_in(&tree, 0o775);

        // Then — the mark covers the prefix itself, nothing beneath it
        assert_eq!(
            the_checked_path(&a_policy_trusting_the_prefix(&store), &git),
            Err(SupervisorError::Denied)
        );
    }

    fn the_checked_path(
        policy: &SpawnPolicy,
        requested: &Path,
    ) -> Result<PathBuf, SupervisorError> {
        resolve_exec(policy, requested).map(|checked| checked.path().to_path_buf())
    }

    #[test]
    fn a_binary_under_an_exec_root_directory_owned_by_the_supervisor_is_run() {
        // Given an exec root directory, and a tool in it nobody else can write
        let tree = an_exec_tree();
        let bin = tree.directory("bin", 0o755);
        let tool = tree.tool("bin/tool", "ran", 0o755);

        // When
        let checked = the_checked_path(&a_policy_with_exec_roots(&[&bin]), &tool);

        // Then
        assert_eq!(checked, Ok(tool));
    }

    #[test]
    fn a_binary_an_exec_root_names_as_a_file_is_run_and_its_sibling_is_not() {
        // Given an exec root naming one trusted file, as `/usr/bin/git` would be
        let tree = an_exec_tree();
        tree.directory("usr-bin", 0o755);
        let git = tree.tool("usr-bin/git", "git", 0o755);
        let sibling = tree.tool("usr-bin/sh-lookalike", "other", 0o755);
        let policy = a_policy_with_exec_roots(&[&git]);

        // When
        let answers = (
            the_checked_path(&policy, &git),
            the_checked_path(&policy, &sibling),
        );

        // Then
        assert_eq!(answers, (Ok(git), Err(SupervisorError::Denied)));
    }

    #[test]
    fn a_binary_outside_every_exec_root_is_refused() {
        // Given
        let tree = an_exec_tree();
        let bin = tree.directory("bin", 0o755);
        tree.directory("elsewhere", 0o755);
        let tool = tree.tool("elsewhere/tool", "ran", 0o755);

        // Then
        assert_eq!(
            the_checked_path(&a_policy_with_exec_roots(&[&bin]), &tool),
            Err(SupervisorError::Denied)
        );
    }

    #[rstest::rstest]
    #[case::group_writable_file(0o775, 0o755)]
    #[case::world_writable_file(0o757, 0o755)]
    #[case::group_writable_parent(0o755, 0o775)]
    #[case::world_writable_parent(0o755, 0o777)]
    #[case::sticky_world_writable_parent_like_tmp(0o755, 0o1777)]
    fn a_binary_whose_file_or_any_parent_is_writable_by_others_is_refused(
        #[case] file_mode: u32,
        #[case] parent_mode: u32,
    ) {
        // Given an exec root naming the file itself
        let tree = an_exec_tree();
        tree.directory("bin", parent_mode);
        let tool = tree.tool("bin/tool", "ran", file_mode);

        // When
        let checked = the_checked_path(&a_policy_with_exec_roots(&[&tool]), &tool);

        // Then — anyone who can write the file, or rename it out of the way, chooses what runs
        assert_eq!(checked, Err(SupervisorError::Denied));
    }

    #[test]
    fn a_binary_owned_by_another_user_is_refused() {
        // Given the supervisor's own euid, and an account that is neither it nor root
        // SAFETY: reads this process's own effective uid and nothing else.
        let euid = unsafe { libc::geteuid() };
        let somebody_else = if euid == 4242 { 4343 } else { 4242 };

        // When
        let trusted = (
            is_trusted_exec_path_entry(0, 0o100755, euid),
            is_trusted_exec_path_entry(euid, 0o100755, euid),
            is_trusted_exec_path_entry(somebody_else, 0o100755, euid),
            is_trusted_exec_path_entry(0, 0o41777, euid),
        );

        // Then — root's and the supervisor's own are trusted; another account's file is not, and
        // neither is a sticky world-writable directory such as /tmp
        assert_eq!(trusted, (true, true, false, false));
    }

    #[test]
    fn a_symlink_under_a_root_to_a_file_outside_every_root_is_refused() {
        // Given a link inside the exec root directory pointing out of it
        let tree = an_exec_tree();
        let bin = tree.directory("bin", 0o755);
        tree.directory("outside", 0o755);
        let target = tree.tool("outside/tool", "escaped", 0o755);
        let link = bin.join("tool");
        std::os::unix::fs::symlink(&target, &link).expect("link out of the root");

        // Then — a symlink is judged by its target
        assert_eq!(
            the_checked_path(&a_policy_with_exec_roots(&[&bin]), &link),
            Err(SupervisorError::Denied)
        );
    }

    #[test]
    fn a_symlink_outside_every_root_to_a_file_a_root_names_runs_that_file() {
        // Given a link outside the roots, as `node_modules/.bin/<harness>` is, to a file under one
        let tree = an_exec_tree();
        let bin = tree.directory("bin", 0o755);
        let target = tree.tool("bin/harness", "ran", 0o755);
        let links = tree.directory("links", 0o755);
        let link = links.join("harness");
        std::os::unix::fs::symlink(&target, &link).expect("link into the root");

        // When
        let checked = the_checked_path(&a_policy_with_exec_roots(&[&bin]), &link);

        // Then — the file checked, and run, is the target
        assert_eq!(checked, Ok(target));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_file_swapped_after_the_check_is_not_what_is_execd() {
        // Given a checked tool
        let tree = an_exec_tree();
        let bin = tree.directory("bin", 0o755);
        let tool = tree.tool("bin/tool", "checked", 0o755);
        let checked = resolve_exec(&a_policy_with_exec_roots(&[&bin]), &tool)
            .expect("the tool passes the exec gate");

        // When another file is renamed over it before the exec
        let impostor = tree.tool("bin/impostor", "swapped-in", 0o755);
        std::fs::rename(&impostor, &tool).expect("swap the file");

        // Then what the child would exec is still the file that was checked
        let execd = std::fs::read_to_string(checked.exec_path()).expect("read what is exec'd");
        assert_eq!(execd, "#!/bin/sh\necho checked\n");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn on_macos_the_exec_gate_is_path_only_and_execs_the_checked_path() {
        // Given — macOS has no exec through a descriptor, so a swap between check and exec is not
        // closed there; the check is on the path alone
        let tree = an_exec_tree();
        let bin = tree.directory("bin", 0o755);
        let tool = tree.tool("bin/tool", "ran", 0o755);

        // When
        let execd = resolve_exec(&a_policy_with_exec_roots(&[&bin]), &tool)
            .map(|checked| checked.exec_path());

        // Then
        assert_eq!(execd, Ok(tool));
    }
}
