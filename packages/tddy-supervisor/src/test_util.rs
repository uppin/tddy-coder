//! Builders shared by the unit tests in this crate.
//!
//! A bare `a_*()` call always produces a valid, usable value, so a test only spells out the one or
//! two fields its behavior actually depends on.

#![cfg(test)]

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::config::{CgroupPolicy, ManagedService, RestartPolicy, SpawnPolicy};
use crate::request::RequestedLimits;

pub const MIB: u64 = 1024 * 1024;

// ---------------------------------------------------------------------------------------------
// Restart policy
// ---------------------------------------------------------------------------------------------

pub struct RestartPolicyBuilder {
    policy: RestartPolicy,
}

/// Two retries, 100ms initial backoff, 1s cap, 10s stability threshold.
pub fn a_restart_policy() -> RestartPolicyBuilder {
    RestartPolicyBuilder {
        policy: RestartPolicy {
            max_retries: 2,
            initial_backoff_ms: 100,
            max_backoff_ms: 1_000,
            stability_threshold_ms: 10_000,
        },
    }
}

impl RestartPolicyBuilder {
    pub fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.policy.max_retries = max_retries;
        self
    }

    pub fn with_initial_backoff_ms(mut self, initial_backoff_ms: u64) -> Self {
        self.policy.initial_backoff_ms = initial_backoff_ms;
        self
    }

    pub fn with_max_backoff_ms(mut self, max_backoff_ms: u64) -> Self {
        self.policy.max_backoff_ms = max_backoff_ms;
        self
    }

    pub fn with_stability_threshold_ms(mut self, stability_threshold_ms: u64) -> Self {
        self.policy.stability_threshold_ms = stability_threshold_ms;
        self
    }

    pub fn build(self) -> RestartPolicy {
        self.policy
    }
}

// ---------------------------------------------------------------------------------------------
// Managed service
// ---------------------------------------------------------------------------------------------

pub struct ManagedServiceBuilder {
    service: ManagedService,
}

/// A service named `tddy-daemon`, running as `tddy`, with the default restart policy.
pub fn a_managed_service() -> ManagedServiceBuilder {
    ManagedServiceBuilder {
        service: ManagedService {
            name: "tddy-daemon".to_string(),
            exec_start: PathBuf::from("/usr/local/bin/tddy-daemon"),
            args: vec!["-c".to_string(), "/etc/tddy/daemon.yaml".to_string()],
            user: "tddy".to_string(),
            group: None,
            working_dir: None,
            env: BTreeMap::new(),
            restart: a_restart_policy().build(),
            socket: None,
        },
    }
}

impl ManagedServiceBuilder {
    pub fn named(mut self, name: &str) -> Self {
        self.service.name = name.to_string();
        self
    }

    pub fn running_as(mut self, user: &str) -> Self {
        self.service.user = user.to_string();
        self
    }

    pub fn with_restart_policy(mut self, restart: RestartPolicy) -> Self {
        self.service.restart = restart;
        self
    }

    pub fn build(self) -> ManagedService {
        self.service
    }
}

// ---------------------------------------------------------------------------------------------
// Spawn policy
// ---------------------------------------------------------------------------------------------

pub struct SpawnPolicyBuilder {
    policy: SpawnPolicy,
}

/// Denies everything. Each test opts in to exactly what it needs.
pub fn a_spawn_policy() -> SpawnPolicyBuilder {
    SpawnPolicyBuilder {
        policy: SpawnPolicy::default(),
    }
}

impl SpawnPolicyBuilder {
    pub fn allowing_session_user(mut self, user: &str) -> Self {
        self.policy.allowed_session_users.push(user.to_string());
        self
    }

    pub fn allowing_tool(mut self, path: &str) -> Self {
        self.policy.allowed_tool_paths.push(PathBuf::from(path));
        self
    }

    pub fn allowing_mount_root(mut self, path: &str) -> Self {
        self.policy.allowed_mount_roots.push(PathBuf::from(path));
        self
    }

    pub fn allowing_env_key(mut self, key: &str) -> Self {
        self.policy.allowed_env_keys.push(key.to_string());
        self
    }

    pub fn build(self) -> SpawnPolicy {
        self.policy
    }
}

// ---------------------------------------------------------------------------------------------
// Cgroup policy
// ---------------------------------------------------------------------------------------------

pub struct CgroupPolicyBuilder {
    policy: CgroupPolicy,
}

/// No ceilings — a request passes through untouched until a test sets one.
pub fn a_cgroup_policy() -> CgroupPolicyBuilder {
    CgroupPolicyBuilder {
        policy: CgroupPolicy::default(),
    }
}

impl CgroupPolicyBuilder {
    pub fn with_memory_ceiling(mut self, bytes: u64) -> Self {
        self.policy.memory_max_ceiling = Some(bytes);
        self
    }

    pub fn with_cpu_ceiling(mut self, cpu_max: &str) -> Self {
        self.policy.cpu_max_ceiling = Some(cpu_max.to_string());
        self
    }

    pub fn with_pids_ceiling(mut self, pids: u64) -> Self {
        self.policy.pids_max_ceiling = Some(pids);
        self
    }

    pub fn build(self) -> CgroupPolicy {
        self.policy
    }
}

// ---------------------------------------------------------------------------------------------
// Requested limits
// ---------------------------------------------------------------------------------------------

/// A request that names no limits at all.
pub fn unlimited() -> RequestedLimits {
    RequestedLimits::default()
}

pub fn requesting_memory(bytes: u64) -> RequestedLimits {
    RequestedLimits {
        memory_max: Some(bytes),
        ..RequestedLimits::default()
    }
}

pub fn requesting_cpu(cpu_max: &str) -> RequestedLimits {
    RequestedLimits {
        cpu_max: Some(cpu_max.to_string()),
        ..RequestedLimits::default()
    }
}

pub fn requesting_pids(pids: u64) -> RequestedLimits {
    RequestedLimits {
        pids_max: Some(pids),
        ..RequestedLimits::default()
    }
}
