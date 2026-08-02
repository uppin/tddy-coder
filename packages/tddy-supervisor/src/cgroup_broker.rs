//! cgroup v2 scope lifecycle inside the subtree the supervisor owns.
//!
//! Every write is relative to an injected base. Production points that base at a delegated slice
//! under `/sys/fs/cgroup`; an operator (or an acceptance test) can point it anywhere with
//! `CgroupPolicy::base_override`. There is one code path either way — the files a scope is made of
//! are ordinary writes, and the kernel is what makes them mean something.

use std::fs;
use std::path::{Path, PathBuf};

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
        // `cgroup.procs` takes one pid per write and ignores the file offset, so appending is both
        // what the kernel expects and what keeps a plain-directory base readable.
        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(&procs)
            .map_err(|error| self.failure("open cgroup.procs", error))?;
        writeln!(file, "{pid}").map_err(|error| self.failure("write cgroup.procs", error))
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

    // TODO(supervisor/milestone-4): on a real cgroupfs host the supervisor must also relocate its
    // own process into `policy.supervisor_leaf` and enable `policy.controllers` in the base's
    // `cgroup.subtree_control` before a scope can carry limits — cgroup v2 refuses controllers on
    // a directory that still holds processes. Until then a supervised host needs that prepared by
    // its systemd unit (`Delegate=yes` plus the slice it is started in).
    Ok(policy.mount_root.join(relative.trim_start_matches('/')))
}

/// The unified-hierarchy path from a `/proc/<pid>/cgroup` body, i.e. the `0::` line's payload.
fn cgroup_v2_path(proc_cgroup: &str) -> Option<&str> {
    proc_cgroup
        .lines()
        .find_map(|line| line.strip_prefix("0::"))
        .map(str::trim)
}
