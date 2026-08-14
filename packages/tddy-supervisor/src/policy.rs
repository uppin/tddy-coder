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

use crate::config::{is_loader_env_key, CgroupPolicy, SpawnPolicy, ROOT_USER};
use crate::error::SupervisorError;
use crate::request::{AppliedLimits, RequestedLimits};
use crate::spawn_broker;

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

/// Resolve the OS user a session may run as.
///
/// `root` is refused even if a config somehow lists it — the loader rejects that too, and a
/// privilege boundary is worth checking twice.
pub fn resolve_session_user(
    policy: &SpawnPolicy,
    requested: &str,
) -> Result<String, SupervisorError> {
    if requested == ROOT_USER {
        return Err(SupervisorError::Denied);
    }
    if policy
        .allowed_session_users
        .iter()
        .any(|allowed| allowed == requested)
    {
        Ok(requested.to_string())
    } else {
        Err(SupervisorError::Denied)
    }
}

/// Resolve the binary a session may exec.
///
/// The path must be absolute, free of `..`, and listed verbatim in the policy. Nothing is
/// canonicalized against the filesystem: a policy decision must not depend on what happens to be
/// on disk at the moment it is made, or a symlink swapped in between check and exec would change
/// the answer.
pub fn resolve_tool_path(
    policy: &SpawnPolicy,
    requested: &Path,
) -> Result<PathBuf, SupervisorError> {
    let requested = plain_absolute_path(requested)?;
    if policy
        .allowed_tool_paths
        .iter()
        .any(|allowed| allowed == requested)
    {
        Ok(requested.to_path_buf())
    } else {
        Err(SupervisorError::Denied)
    }
}

/// Resolve the environment a caller may put on a session it asks for.
///
/// Allowlisted, not denylisted. The tool allowlist is the supervisor's whole control over *what* runs
/// as another OS user, and a caller that can set environment variables on that process has several
/// ways to make it run something else — `LD_PRELOAD` most directly, but also everything else the
/// loader and libc read paths out of. A denylist would have to be complete to be worth anything;
/// an allowlist only has to be short.
///
/// A variable the policy does not list is a refusal rather than a silent omission: a session that
/// started without the environment its caller asked for would behave differently for reasons nobody
/// could see, and quietly doing less than you were asked is how a security control becomes a bug
/// report about something else.
pub fn resolve_env(
    policy: &SpawnPolicy,
    requested: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>, SupervisorError> {
    for key in requested.keys() {
        // Checked before the allowlist, so that a policy which lists one by mistake — the loader
        // never gets to matter — is still refused here.
        if is_loader_env_key(key) {
            log::warn!(
                target: "tddy_supervisor::policy",
                "spawn refused: `{key}` chooses what code the session loads and is never a \
                 caller's to set"
            );
            return Err(SupervisorError::Denied);
        }
        if !policy.allowed_env_keys.iter().any(|allowed| allowed == key) {
            log::warn!(
                target: "tddy_supervisor::policy",
                "spawn refused: `{key}` is not in allowed_env_keys"
            );
            return Err(SupervisorError::Denied);
        }
    }
    Ok(requested.clone())
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
/// Component-wise containment plus a ban on `..` is exactly right for [`resolve_tool_path`], whose
/// allowlist is equality against paths only root can write. It does not carry over to a mount root,
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
fn plain_absolute_path(path: &Path) -> Result<&Path, SupervisorError> {
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
    use crate::test_util::{
        a_cgroup_policy, a_spawn_policy, requesting_cpu, requesting_memory, requesting_pids,
        unlimited, MIB,
    };

    // -----------------------------------------------------------------------------------------
    // Session users
    // -----------------------------------------------------------------------------------------

    #[test]
    fn accepts_a_session_user_that_the_policy_lists() {
        // Given
        let policy = a_spawn_policy()
            .allowing_session_user("alice")
            .allowing_session_user("bob")
            .build();

        // When
        let resolved = resolve_session_user(&policy, "bob");

        // Then
        assert_eq!(resolved, Ok("bob".to_string()));
    }

    #[test]
    fn denies_a_session_user_that_the_policy_does_not_list() {
        // Given
        let policy = a_spawn_policy().allowing_session_user("alice").build();

        // When
        let resolved = resolve_session_user(&policy, "mallory");

        // Then
        assert_eq!(resolved, Err(SupervisorError::Denied));
    }

    #[test]
    fn denies_every_session_user_when_the_policy_lists_none() {
        // Given
        let policy = a_spawn_policy().build();

        // When
        let resolved = resolve_session_user(&policy, "alice");

        // Then
        assert_eq!(resolved, Err(SupervisorError::Denied));
    }

    #[test]
    fn denies_root_as_a_session_user_even_when_the_policy_lists_it() {
        // Given a misconfiguration that the loader should have caught.
        let policy = a_spawn_policy().allowing_session_user("root").build();

        // When
        let resolved = resolve_session_user(&policy, "root");

        // Then — the whole point of the supervisor is that nothing it spawns is root.
        assert_eq!(resolved, Err(SupervisorError::Denied));
    }

    // -----------------------------------------------------------------------------------------
    // Tool paths
    // -----------------------------------------------------------------------------------------

    #[test]
    fn accepts_a_tool_path_that_the_policy_lists() {
        // Given
        let policy = a_spawn_policy()
            .allowing_tool("/usr/local/bin/tddy-coder")
            .build();

        // When
        let resolved = resolve_tool_path(&policy, Path::new("/usr/local/bin/tddy-coder"));

        // Then
        assert_eq!(resolved, Ok(PathBuf::from("/usr/local/bin/tddy-coder")));
    }

    #[test]
    fn denies_a_tool_path_the_policy_does_not_list() {
        // Given
        let policy = a_spawn_policy()
            .allowing_tool("/usr/local/bin/tddy-coder")
            .build();

        // When
        let resolved = resolve_tool_path(&policy, Path::new("/usr/local/bin/tddy-tools"));

        // Then
        assert_eq!(resolved, Err(SupervisorError::Denied));
    }

    #[test]
    fn denies_a_relative_tool_path() {
        // Given
        let policy = a_spawn_policy()
            .allowing_tool("/usr/local/bin/tddy-coder")
            .build();

        // When
        let resolved = resolve_tool_path(&policy, Path::new("tddy-coder"));

        // Then — a relative path would resolve against the supervisor's cwd, which the caller does
        // not know and must not be able to exploit.
        assert_eq!(resolved, Err(SupervisorError::Denied));
    }

    #[test]
    fn denies_a_tool_path_that_reaches_an_allowlisted_path_through_a_traversal() {
        // Given
        let policy = a_spawn_policy()
            .allowing_tool("/usr/local/bin/tddy-coder")
            .build();

        // When
        let resolved = resolve_tool_path(&policy, Path::new("/usr/local/bin/../bin/tddy-coder"));

        // Then — comparison is textual, so a `..` must be refused outright rather than normalized
        // and matched. Normalizing would make the allowlist depend on filesystem state.
        assert_eq!(resolved, Err(SupervisorError::Denied));
    }

    // -----------------------------------------------------------------------------------------
    // Environment
    // -----------------------------------------------------------------------------------------

    fn requesting_env(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    }

    #[test]
    fn accepts_an_environment_variable_the_policy_lists() {
        // Given
        let policy = a_spawn_policy().allowing_env_key("RUST_LOG").build();

        // When
        let resolved = resolve_env(&policy, &requesting_env(&[("RUST_LOG", "debug")]));

        // Then
        assert_eq!(resolved, Ok(requesting_env(&[("RUST_LOG", "debug")])));
    }

    #[test]
    fn accepts_a_request_that_asks_for_no_environment_at_all() {
        // Given
        let policy = a_spawn_policy().build();

        // When
        let resolved = resolve_env(&policy, &BTreeMap::new());

        // Then
        assert_eq!(resolved, Ok(BTreeMap::new()));
    }

    #[test]
    fn denies_an_environment_variable_the_policy_does_not_list() {
        // Given
        let policy = a_spawn_policy().allowing_env_key("RUST_LOG").build();

        // When
        let resolved = resolve_env(&policy, &requesting_env(&[("SSH_AUTH_SOCK", "/tmp/agent")]));

        // Then
        assert_eq!(resolved, Err(SupervisorError::Denied));
    }

    #[test]
    fn denies_every_environment_variable_when_the_policy_lists_none() {
        // Given
        let policy = a_spawn_policy().build();

        // When
        let resolved = resolve_env(&policy, &requesting_env(&[("RUST_LOG", "debug")]));

        // Then
        assert_eq!(resolved, Err(SupervisorError::Denied));
    }

    #[test]
    fn denies_a_preloaded_library_even_when_the_policy_somehow_lists_it() {
        // Given a misconfiguration the loader should have caught.
        let policy = a_spawn_policy().allowing_env_key("LD_PRELOAD").build();

        // When
        let resolved = resolve_env(&policy, &requesting_env(&[("LD_PRELOAD", "/tmp/evil.so")]));

        // Then — this one variable is the tool allowlist bypassed without being touched: the child
        // execs after `setuid` to an ordinary account, so the loader honors it.
        assert_eq!(resolved, Err(SupervisorError::Denied));
    }

    #[test]
    fn denies_a_library_search_path_alongside_an_allowed_variable() {
        // Given a request that hides one dangerous variable behind a permitted one.
        let policy = a_spawn_policy().allowing_env_key("RUST_LOG").build();

        // When
        let resolved = resolve_env(
            &policy,
            &requesting_env(&[("RUST_LOG", "debug"), ("LD_LIBRARY_PATH", "/tmp/lib")]),
        );

        // Then — every variable is checked, not just the first.
        assert_eq!(resolved, Err(SupervisorError::Denied));
    }

    #[test]
    fn denies_a_character_set_conversion_path_that_libc_would_load_code_from() {
        // Given
        let policy = a_spawn_policy().allowing_env_key("GCONV_PATH").build();

        // When
        let resolved = resolve_env(&policy, &requesting_env(&[("GCONV_PATH", "/tmp/gconv")]));

        // Then — the same class of variable as `LD_PRELOAD` without the `LD_` prefix to recognise it
        // by, which is why the rule is a list and not just a prefix.
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
}
