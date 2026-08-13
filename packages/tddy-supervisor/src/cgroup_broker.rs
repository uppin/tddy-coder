//! cgroup v2 scope lifecycle inside the subtree the supervisor owns.
//!
//! Every write is relative to an injected base. Production points that base at a delegated slice
//! under `/sys/fs/cgroup`; an operator (or an acceptance test) can point it anywhere with
//! `CgroupPolicy::base_override`. There is one code path either way — the files a scope is made of
//! are ordinary writes, and the kernel is what makes them mean something.

use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::config::CgroupPolicy;
use crate::error::SupervisorError;
use crate::policy::{clamp_limits, scope_dir};
use crate::request::{CreateScopeRequest, ScopeHandle};

/// Owns the delegated cgroup v2 subtree and carves per-session scopes out of it.
#[derive(Debug, Clone)]
pub struct CgroupBroker {
    base: PathBuf,
    policy: CgroupPolicy,
}

impl CgroupBroker {
    /// Take ownership of `base`, applying `policy`'s ceilings to every scope created under it.
    pub fn new(base: PathBuf, policy: CgroupPolicy) -> CgroupBroker {
        CgroupBroker { base, policy }
    }

    pub fn base(&self) -> &Path {
        &self.base
    }

    /// Make the subtree usable: leave it, then delegate the controllers a scope's limits need.
    ///
    /// Two writes, in this order, and on a real cgroupfs host neither is optional:
    ///
    /// 1. **The supervisor moves its own thread group into [`CgroupPolicy::supervisor_leaf`].**
    ///    cgroup v2's no-internal-processes rule refuses to enable controllers on a cgroup that still
    ///    holds processes, and the supervisor sits in the very directory its scopes are carved out of.
    ///    The leaf holds processes and delegates nothing, so it is allowed to keep them.
    /// 2. **[`CgroupPolicy::controllers`] are enabled in `<base>/cgroup.subtree_control`.** A child
    ///    cgroup's `memory.max`, `cpu.max` and `pids.max` only exist — and only mean anything — when
    ///    its parent has delegated the controller behind them.
    ///
    /// Both failures are fatal. A scope whose limits have no controller behind them is a ceiling this
    /// supervisor reports as applied and the kernel does not enforce, which is worse than refusing to
    /// start: an operator who asked for a 512MiB session ceiling would get an unlimited session and no
    /// indication of it. Failing here also fails at startup, once, where it is visible, rather than
    /// per session.
    ///
    /// `self_pid` is the supervisor's own **TGID** (`std::process::id()`), because `cgroup.procs`
    /// moves a whole thread group — which is what a tokio-driven supervisor needs.
    ///
    /// Every write is relative to the injected base, so this is the same code path on `/sys/fs/cgroup`
    /// and on the plain directory an operator's `base_override` may name.
    pub fn prepare_delegated_subtree(&self, self_pid: u32) -> Result<(), SupervisorError> {
        let leaf = self.leaf_dir()?;
        fs::create_dir_all(&leaf).map_err(|error| {
            unprepared_subtree(format!(
                "create the supervisor leaf {}: {error}",
                leaf.display()
            ))
        })?;
        self.write_pid(&leaf.join("cgroup.procs"), self_pid)
            .map_err(|error| {
                unprepared_subtree(format!(
                    "move the supervisor into {}: {error}. cgroup v2 refuses to delegate controllers \
                     from a cgroup that still holds processes, so the supervisor has to leave the \
                     base before it can carve a scope out of it",
                    leaf.display()
                ))
            })?;

        if let Some(line) = subtree_control_line(&self.policy.controllers) {
            let control = self.base.join("cgroup.subtree_control");
            fs::write(&control, format!("{line}\n")).map_err(|error| {
                unprepared_subtree(format!(
                    "enable controllers `{line}` in {}: {error}. A scope cannot carry the limits this \
                     policy declares without them",
                    control.display()
                ))
            })?;
        }

        log::info!(
            target: "tddy_supervisor::cgroup_broker",
            "prepared the delegated subtree {}: moved pid {self_pid} into {} and enabled {:?}",
            self.base.display(),
            leaf.display(),
            self.policy.controllers
        );
        Ok(())
    }

    /// Create a scope directory and write the limits policy resolved for it.
    ///
    /// Requested limits are clamped down, never rejected: a session that asks for more than the
    /// host permits runs smaller instead of failing to start.
    pub fn create_scope(
        &self,
        request: &CreateScopeRequest,
    ) -> Result<ScopeHandle, SupervisorError> {
        let path = scope_dir(&self.base, &request.name)?;
        let applied = clamp_limits(&self.policy, &request.limits)?;

        fs::create_dir_all(&path).map_err(|error| self.failure("create scope directory", error))?;
        if let Some(memory_max) = applied.memory_max {
            self.write_control(&path, "memory.max", &memory_max.to_string())?;
        }
        if let Some(cpu_max) = &applied.cpu_max {
            self.write_control(&path, "cpu.max", cpu_max)?;
        }
        if let Some(pids_max) = applied.pids_max {
            self.write_control(&path, "pids.max", &pids_max.to_string())?;
        }

        log::info!(
            target: "tddy_supervisor::cgroup_broker",
            "created scope {} with limits {applied:?}",
            path.display()
        );
        Ok(ScopeHandle {
            name: request.name.clone(),
            path,
            applied,
        })
    }

    /// Move an existing process into a scope.
    pub fn attach_pid(&self, scope: &str, pid: u32) -> Result<(), SupervisorError> {
        let procs = self.scope_procs_path(scope)?;
        self.write_pid(&procs, pid)
            .map_err(|error| self.failure("write cgroup.procs", error))
    }

    /// Remove a scope once its session has ended.
    pub fn destroy_scope(&self, scope: &str) -> Result<(), SupervisorError> {
        let path = self.existing_scope_dir(scope)?;
        // `rmdir`, not a recursive delete: a cgroup's control files cannot be unlinked, and the
        // kernel removes them itself when the now-empty directory goes away.
        fs::remove_dir(&path).map_err(|error| self.failure("remove scope directory", error))?;
        log::info!(
            target: "tddy_supervisor::cgroup_broker",
            "destroyed scope {}",
            path.display()
        );
        Ok(())
    }

    /// `cgroup.procs` of an existing scope, for a child to join before it drops privilege.
    pub fn scope_procs_path(&self, scope: &str) -> Result<PathBuf, SupervisorError> {
        Ok(self.existing_scope_dir(scope)?.join("cgroup.procs"))
    }

    /// Move one process into the cgroup that owns `procs`, which is what writing a pid to a
    /// `cgroup.procs` does.
    ///
    /// One pid per write, appended: that is what the kernel takes (it accepts a single pid per write
    /// and ignores the file offset), and appending is also what keeps a plain-directory base readable
    /// as the list of everything that joined rather than only the last one.
    ///
    /// The `io::Error` is handed back rather than dressed, because the two callers are answering
    /// different questions with it: one is a caller's request to place a session, the other is the
    /// supervisor placing *itself* while preparing the subtree.
    fn write_pid(&self, procs: &Path, pid: u32) -> std::io::Result<()> {
        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(procs)?;
        writeln!(file, "{pid}")
    }

    /// The leaf the supervisor moves itself into, refused unless it names one ordinary directory
    /// immediately under the base.
    ///
    /// The same containment [`crate::policy::scope_dir`] applies to a scope name, for the same reason
    /// and one more: this is where a *root* process puts itself, so a name that walks out of the
    /// delegated subtree would move the supervisor into a cgroup nobody delegated to it. The name
    /// comes from the root-owned config rather than from a caller, so the refusal says which key is
    /// wrong instead of being opaque.
    fn leaf_dir(&self) -> Result<PathBuf, SupervisorError> {
        let leaf = self.policy.supervisor_leaf.as_str();
        if !names_one_directory(leaf) {
            return Err(SupervisorError::Invalid {
                message: format!(
                    "cgroup.supervisor_leaf `{leaf}` must name one directory immediately under the \
                     delegated base"
                ),
            });
        }
        Ok(self.base.join(leaf))
    }

    fn existing_scope_dir(&self, scope: &str) -> Result<PathBuf, SupervisorError> {
        let path = scope_dir(&self.base, scope)?;
        if path.is_dir() {
            Ok(path)
        } else {
            Err(SupervisorError::NotFound {
                name: scope.to_string(),
            })
        }
    }

    fn write_control(
        &self,
        scope: &Path,
        file: &str,
        contents: &str,
    ) -> Result<(), SupervisorError> {
        fs::write(scope.join(file), format!("{contents}\n"))
            .map_err(|error| self.failure(&format!("write {file}"), error))
    }

    fn failure(&self, what: &str, error: std::io::Error) -> SupervisorError {
        SupervisorError::OperationFailed {
            message: format!("{what} under {}: {error}", self.base.display()),
        }
    }
}

/// A subtree the supervisor cannot prepare, with what an operator can do about it.
///
/// One helper for every step of the preparation, because whichever of them failed the two remedies
/// are the same — and because this failure stops the supervisor from starting, so the message is the
/// operator's only diagnostic.
fn unprepared_subtree(what: String) -> SupervisorError {
    SupervisorError::OperationFailed {
        message: format!(
            "{what}. The supervisor will not run with a cgroup subtree it cannot limit: start it in a \
             slice with `Delegate=yes`, or name an already-prepared subtree with \
             `cgroup.base_override`"
        ),
    }
}

/// What became of an attempt to remove a scope directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScopeRemoval {
    /// Gone. Either this call removed it, or something already had — `DestroyScope` remains a
    /// caller's to make, and a scope removed twice is still a scope removed.
    Removed,
    /// The cgroup still holds processes, so the kernel refuses to remove it. A session's own
    /// descendants outliving the session leader is ordinary, and they are usually gone a moment later.
    StillPopulated,
    /// A failure the passing of time does not change.
    Failed { message: String },
}

/// Remove a scope directory whose session has ended, saying what became of the attempt.
///
/// Free-standing rather than a [`CgroupBroker`] method because the caller is the reap path, which
/// knows the scope directory of a session that has exited and not the name a caller created it under.
/// Every write to a cgroup directory still lives in this module.
///
/// `rmdir`, not a recursive delete, for the reason [`CgroupBroker::destroy_scope`] gives: a cgroup's
/// control files cannot be unlinked, and the kernel removes them itself with the directory.
pub fn remove_scope_dir(scope: &Path) -> ScopeRemoval {
    match fs::remove_dir(scope) {
        Ok(()) => ScopeRemoval::Removed,
        Err(error) => match classify_removal_failure(&error) {
            RemovalFailure::AlreadyGone => ScopeRemoval::Removed,
            RemovalFailure::StillPopulated => ScopeRemoval::StillPopulated,
            RemovalFailure::Permanent => ScopeRemoval::Failed {
                message: format!("remove scope directory {}: {error}", scope.display()),
            },
        },
    }
}

/// What a failed `rmdir` of a scope says about whether it is worth trying again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RemovalFailure {
    /// Somebody else removed it first, which is the outcome that was wanted.
    AlreadyGone,
    /// Processes remain in the cgroup. Trying again shortly is the only thing that helps.
    StillPopulated,
    /// Retrying would change nothing.
    Permanent,
}

/// Decide what a failed `rmdir` means. Pure, so every branch is assertable.
///
/// `EBUSY` is the interesting one: cgroup v2 returns it both for a cgroup that still holds processes
/// and for one that still has child cgroups, and either can clear on its own the moment the last of
/// them exits. Everything else — `EACCES`, `ENOTDIR`, and the `ENOTEMPTY` a plain-directory base
/// produces for the control files the broker itself wrote — is the same failure however often it is
/// retried, so it is reported instead of looped on.
fn classify_removal_failure(error: &std::io::Error) -> RemovalFailure {
    match error.raw_os_error() {
        Some(libc::ENOENT) => RemovalFailure::AlreadyGone,
        Some(libc::EBUSY) => RemovalFailure::StillPopulated,
        _ => RemovalFailure::Permanent,
    }
}

/// The line `cgroup.subtree_control` takes to enable `controllers`, or `None` when there are none.
///
/// `None` rather than an empty string: writing nothing to `cgroup.subtree_control` is not a request
/// the kernel has an answer to, and an operator who declared no controller asked for no write.
fn subtree_control_line(controllers: &[String]) -> Option<String> {
    if controllers.is_empty() {
        return None;
    }
    Some(
        controllers
            .iter()
            .map(|controller| format!("+{controller}"))
            .collect::<Vec<_>>()
            .join(" "),
    )
}

/// Whether `name` is exactly one ordinary path component, i.e. a single directory under some parent.
///
/// The same rule [`crate::policy::scope_dir`] enforces on a scope name. It lives here as well because
/// the supervisor leaf comes from the config rather than from a request, and the two rejections say
/// different things: a caller learns nothing, an operator learns which key is wrong.
fn names_one_directory(name: &str) -> bool {
    let mut components = Path::new(name).components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(single)), None) => single.to_str() == Some(name),
        _ => false,
    }
}

/// Resolve the cgroup v2 subtree the supervisor owns.
///
/// `base_override`, when set, is used **verbatim** — no `/proc/self/cgroup` reading, no
/// `/proc/self/mountinfo` probe. It is a documented production option for hosts where the
/// delegated slice is known up front, and the only reason scope handling is exercisable off a
/// cgroupfs host.
pub fn resolve_cgroup_base(policy: &CgroupPolicy) -> anyhow::Result<PathBuf> {
    if let Some(base) = &policy.base_override {
        return Ok(base.clone());
    }
    let own = std::fs::read_to_string("/proc/self/cgroup")
        .map_err(|error| anyhow::anyhow!("read /proc/self/cgroup: {error}"))?;
    let relative = cgroup_v2_path(&own).ok_or_else(|| {
        anyhow::anyhow!(
            "no cgroup v2 (`0::`) line in /proc/self/cgroup; set cgroup.base_override instead"
        )
    })?;

    // Resolution only. Making the subtree usable — leaving it, and delegating the controllers a
    // scope's limits are written against — is [`CgroupBroker::prepare_delegated_subtree`], which acts
    // on whichever base ends up injected and so is one code path for both of these branches.
    Ok(policy.mount_root.join(relative.trim_start_matches('/')))
}

/// The unified-hierarchy path from a `/proc/<pid>/cgroup` body, i.e. the `0::` line's payload.
fn cgroup_v2_path(proc_cgroup: &str) -> Option<&str> {
    proc_cgroup
        .lines()
        .find_map(|line| line.strip_prefix("0::"))
        .map(str::trim)
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::test_util::a_cgroup_policy;

    /// A pid the kernel is free to hand to anybody, used as the supervisor's own.
    const SUPERVISOR_PID: u32 = 4242;

    /// An empty directory standing in for the delegated cgroup v2 subtree, exactly as an operator's
    /// `base_override` and the acceptance harness point the broker at one.
    fn a_delegated_base() -> tempfile::TempDir {
        tempfile::TempDir::new().expect("create a delegated cgroup base")
    }

    fn a_broker_owning(base: &Path, policy: CgroupPolicy) -> CgroupBroker {
        CgroupBroker::new(base.to_path_buf(), policy)
    }

    fn contents_of(path: PathBuf) -> String {
        std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
    }

    // -----------------------------------------------------------------------------------------
    // Preparing the delegated subtree
    // -----------------------------------------------------------------------------------------

    #[test]
    fn moves_the_supervisor_into_a_leaf_of_the_subtree_it_owns() {
        // Given
        let base = a_delegated_base();
        let broker = a_broker_owning(base.path(), a_cgroup_policy().build());

        // When
        broker
            .prepare_delegated_subtree(SUPERVISOR_PID)
            .expect("prepare the delegated subtree");

        // Then — cgroup v2 refuses controllers on a cgroup that still holds processes, so the
        // supervisor has to leave the directory it carves scopes out of before it can enable any.
        assert_eq!(
            contents_of(base.path().join("supervisor/cgroup.procs")),
            format!("{SUPERVISOR_PID}\n")
        );
    }

    #[test]
    fn enables_every_controller_the_policy_declares_on_the_subtree_it_carves_scopes_out_of() {
        // Given a policy declaring the three controllers a scope's limits are written against.
        let base = a_delegated_base();
        let broker = a_broker_owning(base.path(), a_cgroup_policy().build());

        // When
        broker
            .prepare_delegated_subtree(SUPERVISOR_PID)
            .expect("prepare the delegated subtree");

        // Then — without these, a scope's `memory.max` / `cpu.max` / `pids.max` have no controller
        // behind them and the policy's ceilings are not enforced by anything.
        assert_eq!(
            contents_of(base.path().join("cgroup.subtree_control")),
            "+memory +cpu +pids\n"
        );
    }

    #[test]
    fn writes_no_controller_line_when_the_policy_declares_no_controller() {
        // Given an operator who declared an empty controller list.
        let base = a_delegated_base();
        let broker = a_broker_owning(
            base.path(),
            CgroupPolicy {
                controllers: Vec::new(),
                ..a_cgroup_policy().build()
            },
        );

        // When
        broker
            .prepare_delegated_subtree(SUPERVISOR_PID)
            .expect("prepare the delegated subtree");

        // Then — nothing was asked for, so nothing is written. An empty write would be a request the
        // kernel has no answer to.
        assert!(
            !base.path().join("cgroup.subtree_control").exists(),
            "no controller was declared, so nothing should have been enabled"
        );
    }

    #[test]
    fn renders_the_controller_line_in_the_format_subtree_control_takes() {
        // Given
        let controllers = vec!["memory".to_string(), "pids".to_string()];

        // When
        let line = subtree_control_line(&controllers);

        // Then
        assert_eq!(line, Some("+memory +pids".to_string()));
    }

    #[test]
    fn renders_no_controller_line_for_a_policy_that_declares_none() {
        // Given / When
        let line = subtree_control_line(&[]);

        // Then
        assert_eq!(line, None);
    }

    #[rstest]
    #[case::a_parent_directory("..")]
    #[case::a_nested_path("nested/leaf")]
    #[case::an_absolute_path("/sys/fs/cgroup")]
    #[case::the_base_itself(".")]
    #[case::nothing_at_all("")]
    fn refuses_a_supervisor_leaf_that_does_not_name_one_directory_under_the_base(
        #[case] leaf: &str,
    ) {
        // Given
        let base = a_delegated_base();
        let broker = a_broker_owning(
            base.path(),
            CgroupPolicy {
                supervisor_leaf: leaf.to_string(),
                ..a_cgroup_policy().build()
            },
        );

        // When
        let prepared = broker.prepare_delegated_subtree(SUPERVISOR_PID);

        // Then — the leaf is where the supervisor puts *itself*, so a name that walks out of the
        // delegated subtree would move a root process into a cgroup nobody delegated.
        assert_eq!(
            prepared,
            Err(SupervisorError::Invalid {
                message: format!(
                    "cgroup.supervisor_leaf `{leaf}` must name one directory immediately under the \
                     delegated base"
                ),
            })
        );
    }

    #[test]
    fn enables_no_controller_when_it_could_not_move_itself_out_of_the_base() {
        // Given a base where the leaf's name is already taken by a file, so the supervisor cannot
        // move into it.
        let base = a_delegated_base();
        std::fs::write(base.path().join("supervisor"), "not a cgroup")
            .expect("occupy the leaf's name");
        let broker = a_broker_owning(base.path(), a_cgroup_policy().build());

        // When
        let prepared = broker.prepare_delegated_subtree(SUPERVISOR_PID);

        // Then — the no-internal-processes rule is an ordering contract: controllers must not be
        // enabled on a base the supervisor is still sitting in.
        assert!(prepared.is_err(), "preparing an unusable base must fail");
        assert!(
            !base.path().join("cgroup.subtree_control").exists(),
            "controllers were enabled on a base the supervisor had not left"
        );
    }

    #[test]
    fn fails_when_the_controllers_the_policy_declares_cannot_be_enabled() {
        // Given a base whose `cgroup.subtree_control` cannot be written — the shape a host that
        // delegated no controllers presents.
        let base = a_delegated_base();
        std::fs::create_dir(base.path().join("cgroup.subtree_control"))
            .expect("make the controller file unwritable");
        let broker = a_broker_owning(base.path(), a_cgroup_policy().build());

        // When
        let prepared = broker.prepare_delegated_subtree(SUPERVISOR_PID);

        // Then — fatal, not a warning: a scope whose limits have no controller behind them is a
        // ceiling the supervisor reports as applied and the kernel does not enforce. The errno text
        // belongs to the host, so only the part the supervisor writes is asserted.
        let message = prepared
            .expect_err("a subtree whose controllers cannot be enabled must not be accepted")
            .to_string();
        assert!(
            message.contains("enable controllers `+memory +cpu +pids`"),
            "the failure should name the controllers that could not be enabled, got: {message}"
        );
    }

    // -----------------------------------------------------------------------------------------
    // Removing a scope whose session has gone
    // -----------------------------------------------------------------------------------------

    #[test]
    fn removes_a_scope_directory_that_holds_nothing() {
        // Given
        let base = a_delegated_base();
        let scope = base.path().join("tddy-session-alpha.scope");
        std::fs::create_dir(&scope).expect("create a scope directory");

        // When
        let removal = remove_scope_dir(&scope);

        // Then
        assert_eq!(removal, ScopeRemoval::Removed);
        assert!(!scope.exists(), "the scope directory outlived its session");
    }

    #[test]
    fn treats_a_scope_directory_something_already_removed_as_gone() {
        // Given a scope a caller destroyed itself through `DestroyScope`.
        let base = a_delegated_base();
        let scope = base.path().join("tddy-session-destroyed.scope");

        // When
        let removal = remove_scope_dir(&scope);

        // Then — the goal is that the scope is gone, and it is. A caller cleaning up after itself is
        // cooperation, not a failure.
        assert_eq!(removal, ScopeRemoval::Removed);
    }

    #[test]
    fn reports_a_scope_directory_it_could_not_remove() {
        // Given a path that is not a directory at all.
        let base = a_delegated_base();
        let scope = base.path().join("tddy-session-file.scope");
        std::fs::write(&scope, "not a cgroup").expect("create a file where a scope should be");

        // When
        let removal = remove_scope_dir(&scope);

        // Then — the message names the path, because a scope left behind is a leak an operator has to
        // be able to find. The errno text is built from the same errno the kernel returns.
        assert_eq!(
            removal,
            ScopeRemoval::Failed {
                message: format!(
                    "remove scope directory {}: {}",
                    scope.display(),
                    std::io::Error::from_raw_os_error(libc::ENOTDIR)
                ),
            }
        );
    }

    #[test]
    fn keeps_a_scope_the_kernel_says_still_holds_processes() {
        // Given the errno `rmdir` returns for a cgroup that is not empty of processes.
        let busy = std::io::Error::from_raw_os_error(libc::EBUSY);

        // When
        let failure = classify_removal_failure(&busy);

        // Then — a session's own descendants routinely outlive it by a moment, and they are the
        // reason the directory is still there.
        assert_eq!(failure, RemovalFailure::StillPopulated);
    }

    #[test]
    fn treats_a_scope_the_kernel_no_longer_knows_about_as_removed() {
        // Given
        let absent = std::io::Error::from_raw_os_error(libc::ENOENT);

        // When
        let failure = classify_removal_failure(&absent);

        // Then
        assert_eq!(failure, RemovalFailure::AlreadyGone);
    }

    #[rstest]
    #[case::not_permitted(libc::EACCES)]
    #[case::not_a_directory(libc::ENOTDIR)]
    #[case::still_has_entries(libc::ENOTEMPTY)]
    fn gives_up_on_a_scope_no_number_of_retries_would_remove(#[case] errno: i32) {
        // Given
        let error = std::io::Error::from_raw_os_error(errno);

        // When
        let failure = classify_removal_failure(&error);

        // Then — retrying an `rmdir` that fails for a reason the passing of time does not change is a
        // loop that only logs.
        assert_eq!(failure, RemovalFailure::Permanent);
    }
}
