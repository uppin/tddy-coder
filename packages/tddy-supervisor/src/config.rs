//! The root-owned supervisor configuration.
//!
//! This file is the entire policy surface. Nothing the daemon sends can widen it: target users,
//! tool paths, mount roots and resource ceilings are all decided here, by root, at load time.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::ConfigError;

/// The one OS user the supervisor never spawns anything as, in any capacity.
pub(crate) const ROOT_USER: &str = "root";

/// The credentials the supervisor never hands to a child.
///
/// Separate facts from [`ROOT_USER`], and both have to be checked: a passwd entry can carry uid 0
/// under any name it likes, so rejecting the *name* `root` says nothing about the ids an account
/// actually resolves to. See [`crate::spawn_broker::refuse_root_credentials`].
pub(crate) const ROOT_UID: u32 = 0;
pub(crate) const ROOT_GID: u32 = 0;

/// Environment variables that decide what code a process loads, whatever binary it was told to
/// exec. Prefixes, because the loader reads a family of each.
const LOADER_ENV_PREFIXES: [&str; 2] = ["LD_", "MALLOC_"];

/// The rest of the same family, which have no common prefix.
const LOADER_ENV_KEYS: [&str; 5] = [
    "GCONV_PATH",
    "NLSPATH",
    "LOCPATH",
    "HOSTALIASES",
    "RESOLV_HOST_CONF",
];

/// Whether `key` is one of the variables that chooses code rather than configuring behavior.
///
/// `LD_PRELOAD` on its own turns any allowlisted tool into a loader for a library the caller picked,
/// which is the `allowed_tool_paths` allowlist bypassed without being touched. It works because the
/// child execs *after* `setuid` to an ordinary account, so the binary is not setuid, `AT_SECURE` is
/// unset, and the dynamic loader honors all of these.
pub(crate) fn is_loader_env_key(key: &str) -> bool {
    LOADER_ENV_PREFIXES
        .iter()
        .any(|prefix| key.starts_with(prefix))
        || LOADER_ENV_KEYS.contains(&key)
}

/// Top-level supervisor configuration, loaded from `/etc/tddy/supervisor.yaml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SupervisorConfig {
    pub socket: SocketConfig,
    #[serde(default)]
    pub services: Vec<ManagedService>,
    #[serde(default)]
    pub spawn_policy: SpawnPolicy,
    #[serde(default)]
    pub cgroup: CgroupPolicy,
    /// How long managed services get to exit after `SIGTERM` before they are `SIGKILL`ed.
    #[serde(default = "default_shutdown_grace_secs")]
    pub shutdown_grace_secs: u64,
}

fn default_shutdown_grace_secs() -> u64 {
    20
}

/// Where the privileged RPC socket lives and who may open it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SocketConfig {
    pub path: PathBuf,
    /// Group granted access to the socket. The socket itself is always owned by root.
    #[serde(default)]
    pub group: Option<String>,
    /// Octal permission string, e.g. `"0660"`.
    #[serde(default = "default_socket_mode")]
    pub mode: String,
}

fn default_socket_mode() -> String {
    "0660".to_string()
}

/// A process the supervisor starts, keeps alive, and shuts down.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedService {
    pub name: String,
    pub exec_start: PathBuf,
    #[serde(default)]
    pub args: Vec<String>,
    /// OS user the service runs as. `root` is rejected at load time.
    pub user: String,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub working_dir: Option<PathBuf>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub restart: RestartPolicy,
    /// A listening socket the supervisor creates as root and hands to the service.
    ///
    /// This is how an unprivileged managed service gets a socket in a directory it cannot write —
    /// exactly the job `tddy-daemon.socket` used to do for the daemon, which is why the daemon
    /// already implements the receiving half (`SocketSource::Activated`).
    #[serde(default)]
    pub socket: Option<ServiceSocket>,
}

/// A listening socket created on a managed service's behalf, before it starts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceSocket {
    /// Absolute path to bind. Created and owned by root.
    pub path: PathBuf,
    /// Group granted access. Without one, only the service user can connect.
    #[serde(default)]
    pub group: Option<String>,
    /// Octal permission string, e.g. `"0660"`.
    #[serde(default = "default_socket_mode")]
    pub mode: String,
}

/// Restart behavior for a single managed service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RestartPolicy {
    /// Consecutive failed restarts tolerated before the service is given up on.
    pub max_retries: u32,
    pub initial_backoff_ms: u64,
    pub max_backoff_ms: u64,
    /// Uptime after which a service is considered healthy and its backoff resets.
    pub stability_threshold_ms: u64,
}

impl Default for RestartPolicy {
    fn default() -> Self {
        RestartPolicy {
            max_retries: 5,
            initial_backoff_ms: 500,
            max_backoff_ms: 30_000,
            stability_threshold_ms: 10_000,
        }
    }
}

/// What the daemon is allowed to ask the supervisor to spawn, and on whose behalf.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpawnPolicy {
    /// OS users a session may run as. `root` is rejected at load time.
    #[serde(default)]
    pub allowed_session_users: Vec<String>,
    /// Absolute paths of tools a session may exec.
    #[serde(default)]
    pub allowed_tool_paths: Vec<PathBuf>,
    /// Roots under which a sandbox bind-mount source must fall.
    #[serde(default)]
    pub allowed_mount_roots: Vec<PathBuf>,
    /// Environment variables a caller may set on a session it asks for.
    ///
    /// Empty by default, so a caller's `env` is refused rather than passed on. The caller is the
    /// daemon; a variable it can put into the environment of a process running as *another* OS user
    /// is a grant, and grants belong in this file. Loader variables ([`is_loader_env_key`]) are
    /// rejected here at load time — listing one would hand over the tool allowlist with it.
    #[serde(default)]
    pub allowed_env_keys: Vec<String>,
}

/// Delegation settings and the ceilings every requested resource limit is clamped to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CgroupPolicy {
    /// Explicit cgroup v2 base. When absent the supervisor derives it from `/proc/self/cgroup`.
    #[serde(default)]
    pub base_override: Option<PathBuf>,
    #[serde(default = "default_mount_root")]
    pub mount_root: PathBuf,
    #[serde(default = "default_controllers")]
    pub controllers: Vec<String>,
    /// Leaf the supervisor moves *itself* into to satisfy cgroup v2's no-internal-processes
    /// rule. Unrelated to the `tddy-supervisor` process name despite sharing the word.
    #[serde(default = "default_supervisor_leaf")]
    pub supervisor_leaf: String,
    #[serde(default)]
    pub memory_max_ceiling: Option<u64>,
    /// `"<quota_us> <period_us>"`, matching the kernel's `cpu.max` format.
    #[serde(default)]
    pub cpu_max_ceiling: Option<String>,
    #[serde(default)]
    pub pids_max_ceiling: Option<u64>,
}

fn default_mount_root() -> PathBuf {
    PathBuf::from("/sys/fs/cgroup")
}

fn default_controllers() -> Vec<String> {
    vec!["memory".to_string(), "cpu".to_string(), "pids".to_string()]
}

fn default_supervisor_leaf() -> String {
    "supervisor".to_string()
}

impl Default for CgroupPolicy {
    fn default() -> Self {
        CgroupPolicy {
            base_override: None,
            mount_root: default_mount_root(),
            controllers: default_controllers(),
            supervisor_leaf: default_supervisor_leaf(),
            memory_max_ceiling: None,
            cpu_max_ceiling: None,
            pids_max_ceiling: None,
        }
    }
}

impl SocketConfig {
    /// Permission bits from the octal `mode` string.
    pub fn mode_bits(&self) -> Result<u32, ConfigError> {
        u32::from_str_radix(&self.mode, 8).map_err(|_| ConfigError {
            message: format!(
                "socket mode `{}` is not an octal permission string",
                self.mode
            ),
        })
    }
}

impl ServiceSocket {
    /// Permission bits from the octal `mode` string.
    pub fn mode_bits(&self) -> Result<u32, ConfigError> {
        u32::from_str_radix(&self.mode, 8).map_err(|_| ConfigError {
            message: format!(
                "service socket mode `{}` is not an octal permission string",
                self.mode
            ),
        })
    }
}

impl SupervisorConfig {
    /// Read and validate the configuration at `path`, refusing one anybody else could have written.
    ///
    /// Everything in this crate rests on this file being authored by the identity that enforces it —
    /// "the root-owned policy file" is the premise of the whole design — and until now that was an
    /// assumption about how the file got there rather than something the supervisor checked. It is
    /// worth checking independently: `install` creating `/etc/tddy` with an inherited `umask` of `000`
    /// is enough for a local user to rewrite `allowed_session_users` and have the root broker enforce
    /// it.
    pub fn load(path: &Path) -> Result<SupervisorConfig, ConfigError> {
        // Opened once and read through the same handle the ownership check `fstat`s, so the file that
        // is parsed is the file that was checked and not one swapped in between the two.
        let file = std::fs::File::open(path).map_err(|error| ConfigError {
            message: format!("could not read {}: {error}", path.display()),
        })?;
        let metadata = file.metadata().map_err(|error| ConfigError {
            message: format!("could not inspect {}: {error}", path.display()),
        })?;
        require_trustworthy_config_file(path, &metadata)?;
        // The file being safe is not enough: a directory on the way to it that somebody else can write
        // is a directory in which they can replace the file wholesale.
        //
        // Walked over the canonical path, so `..`, a relative `--config`, and a symlinked component
        // are all checked as the directories they actually resolve to rather than as the ones the
        // operator typed.
        let resolved = std::fs::canonicalize(path).map_err(|error| ConfigError {
            message: format!("could not resolve {}: {error}", path.display()),
        })?;
        let mut ancestor = resolved.parent();
        while let Some(directory) = ancestor {
            let metadata = std::fs::metadata(directory).map_err(|error| ConfigError {
                message: format!("could not inspect {}: {error}", directory.display()),
            })?;
            require_trustworthy_config_directory(directory, &metadata)?;
            ancestor = directory.parent();
        }

        let mut yaml = String::new();
        {
            use std::io::Read;
            let mut file = file;
            file.read_to_string(&mut yaml)
                .map_err(|error| ConfigError {
                    message: format!("could not read {}: {error}", path.display()),
                })?;
        }
        SupervisorConfig::from_yaml(&yaml)
    }

    /// Parse and validate configuration from YAML text.
    ///
    /// Validation is part of parsing on purpose: a config that would let the daemon escalate is
    /// a startup failure, not a runtime surprise.
    pub fn from_yaml(yaml: &str) -> Result<SupervisorConfig, ConfigError> {
        let config: SupervisorConfig = serde_yaml::from_str(yaml).map_err(|error| ConfigError {
            message: error.to_string(),
        })?;
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<(), ConfigError> {
        self.socket.mode_bits()?;
        require_absolute("socket path", &self.socket.path)?;

        let mut names = BTreeSet::new();
        let mut socket_paths = BTreeSet::new();
        for service in &self.services {
            if service.user == ROOT_USER {
                return Err(ConfigError {
                    message: format!(
                        "service `{}` declares user `{ROOT_USER}`; the supervisor is the only \
                         privileged process",
                        service.name
                    ),
                });
            }
            require_absolute("exec_start", &service.exec_start)?;
            // Names address services over RPC, so a duplicate makes Start/Stop ambiguous.
            if !names.insert(service.name.as_str()) {
                return Err(ConfigError {
                    message: format!("two services share the name `{}`", service.name),
                });
            }
            if let Some(socket) = &service.socket {
                socket.mode_bits()?;
                require_absolute("service socket path", &socket.path)?;
                // One listener has one owner. Whichever service bound the path second would
                // silently take over the first's clients.
                if !socket_paths.insert(socket.path.as_path()) {
                    return Err(ConfigError {
                        message: format!(
                            "two services declare the socket path `{}`",
                            socket.path.display()
                        ),
                    });
                }
            }
        }

        if self
            .spawn_policy
            .allowed_session_users
            .iter()
            .any(|user| user == ROOT_USER)
        {
            return Err(ConfigError {
                message: format!("allowed_session_users may not contain `{ROOT_USER}`"),
            });
        }
        for tool in &self.spawn_policy.allowed_tool_paths {
            require_absolute("allowed_tool_paths entry", tool)?;
        }
        for root in &self.spawn_policy.allowed_mount_roots {
            require_absolute("allowed_mount_roots entry", root)?;
        }
        for key in &self.spawn_policy.allowed_env_keys {
            if is_loader_env_key(key) {
                return Err(ConfigError {
                    message: format!(
                        "allowed_env_keys may not contain `{key}`: it chooses what code a session \
                         loads, which would hand a caller everything allowed_tool_paths withholds"
                    ),
                });
            }
        }

        Ok(())
    }
}

/// Refuse a policy file that somebody other than the supervisor's own identity could have written.
///
/// "Owned by uid 0 **or** by the supervisor's own effective uid" is one rule, not a concession: on a
/// host the supervisor runs as root, both halves are uid 0, and the rule reads "root-owned". Run
/// unprivileged — a developer, or the acceptance suite driving the real binary — it reads "owned by
/// whoever is enforcing it", which is the same statement about the same trust relationship. What it
/// never permits is a policy file authored by a *third* party, which is the only case that matters.
fn require_trustworthy_config_file(
    path: &Path,
    metadata: &std::fs::Metadata,
) -> Result<(), ConfigError> {
    use std::os::unix::fs::MetadataExt;

    // SAFETY: reads this process's own effective uid and nothing else.
    let enforcing_uid = unsafe { libc::geteuid() };
    if metadata.uid() != ROOT_UID && metadata.uid() != enforcing_uid {
        return Err(ConfigError {
            message: format!(
                "{} is owned by uid {} but only uid {ROOT_UID} or uid {enforcing_uid} may write the \
                 policy this supervisor enforces",
                path.display(),
                metadata.uid()
            ),
        });
    }
    if metadata.mode() & GROUP_OR_OTHER_WRITE != 0 {
        return Err(ConfigError {
            message: format!(
                "{} is writable by its group or by everyone (mode {:o}); anyone who can write it \
                 chooses what the supervisor permits",
                path.display(),
                metadata.mode() & 0o7777
            ),
        });
    }
    Ok(())
}

/// Refuse a directory on the way to the policy file that a third party could write.
///
/// Group- or world-writable is refused *unless* the sticky bit is set, and that exception is the
/// kernel's own rule rather than a loophole: in a sticky directory only the owner of an entry may
/// rename or unlink it, so `/tmp` being `1777` does not let anybody replace somebody else's file.
/// A `/etc/tddy` left at `0777` has no such protection, and that is precisely the case being caught.
fn require_trustworthy_config_directory(
    path: &Path,
    metadata: &std::fs::Metadata,
) -> Result<(), ConfigError> {
    use std::os::unix::fs::MetadataExt;

    // SAFETY: reads this process's own effective uid and nothing else.
    let enforcing_uid = unsafe { libc::geteuid() };
    if metadata.uid() != ROOT_UID && metadata.uid() != enforcing_uid {
        return Err(ConfigError {
            message: format!(
                "{} is owned by uid {}, so uid {} could replace the supervisor's policy file",
                path.display(),
                metadata.uid(),
                metadata.uid()
            ),
        });
    }
    let sticky = metadata.mode() & 0o1000 != 0;
    if metadata.mode() & GROUP_OR_OTHER_WRITE != 0 && !sticky {
        return Err(ConfigError {
            message: format!(
                "{} is writable by its group or by everyone (mode {:o}) and is not sticky, so the \
                 supervisor's policy file could be replaced",
                path.display(),
                metadata.mode() & 0o7777
            ),
        });
    }
    Ok(())
}

/// The two write bits that decide whether anybody but the owner can change something.
const GROUP_OR_OTHER_WRITE: u32 = 0o022;

/// A relative path in this config is never usable: it would resolve against whatever directory
/// systemd happened to start the supervisor in, so an allowlist entry would be a silently dead
/// grant rather than a working one.
fn require_absolute(label: &str, path: &Path) -> Result<(), ConfigError> {
    if path.is_absolute() {
        return Ok(());
    }
    Err(ConfigError {
        message: format!("{label} `{}` must be absolute", path.display()),
    })
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    /// The smallest config that should load: a socket and nothing else.
    fn a_minimal_config() -> &'static str {
        "socket:\n  path: /run/tddy-supervisor.sock\n"
    }

    fn assert_rejected_because(result: Result<SupervisorConfig, ConfigError>, reason: &str) {
        let error = result.expect_err("expected the config to be rejected");
        assert!(
            error.message.contains(reason),
            "rejection should mention `{reason}`, got: {}",
            error.message
        );
    }

    #[test]
    fn parses_a_minimal_config_with_only_a_socket_path() {
        // Given / When
        let config = SupervisorConfig::from_yaml(a_minimal_config()).expect("parse config");

        // Then
        assert_eq!(
            config.socket.path,
            PathBuf::from("/run/tddy-supervisor.sock")
        );
        assert_eq!(config.socket.group, None);
        assert_eq!(config.services, Vec::new());
    }

    #[test]
    fn defaults_the_socket_mode_to_owner_and_group_read_write() {
        // Given / When
        let config = SupervisorConfig::from_yaml(a_minimal_config()).expect("parse config");

        // Then — the socket is root-owned, so group access is the whole grant.
        assert_eq!(config.socket.mode, "0660");
        assert_eq!(config.socket.mode_bits(), Ok(0o660));
    }

    #[test]
    fn parses_an_explicit_octal_socket_mode() {
        // Given
        let yaml = "socket:\n  path: /run/tddy-supervisor.sock\n  mode: \"0600\"\n";

        // When
        let config = SupervisorConfig::from_yaml(yaml).expect("parse config");

        // Then
        assert_eq!(config.socket.mode_bits(), Ok(0o600));
    }

    #[test]
    fn rejects_a_socket_mode_that_is_not_octal() {
        // Given
        let yaml = "socket:\n  path: /run/tddy-supervisor.sock\n  mode: \"rw-rw----\"\n";

        // When / Then
        assert_rejected_because(SupervisorConfig::from_yaml(yaml), "mode");
    }

    #[test]
    fn rejects_a_relative_socket_path() {
        // Given
        let yaml = "socket:\n  path: tddy-supervisor.sock\n";

        // When / Then
        assert_rejected_because(SupervisorConfig::from_yaml(yaml), "absolute");
    }

    #[test]
    fn defaults_the_shutdown_grace_to_twenty_seconds() {
        // Given / When
        let config = SupervisorConfig::from_yaml(a_minimal_config()).expect("parse config");

        // Then
        assert_eq!(config.shutdown_grace_secs, 20);
    }

    #[test]
    fn defaults_the_cgroup_controllers_to_memory_cpu_and_pids() {
        // Given / When
        let config = SupervisorConfig::from_yaml(a_minimal_config()).expect("parse config");

        // Then
        assert_eq!(config.cgroup.controllers, vec!["memory", "cpu", "pids"]);
        assert_eq!(config.cgroup.mount_root, PathBuf::from("/sys/fs/cgroup"));
        assert_eq!(config.cgroup.supervisor_leaf, "supervisor");
    }

    #[test]
    fn defaults_to_denying_every_session_user_tool_and_mount_root() {
        // Given / When
        let config = SupervisorConfig::from_yaml(a_minimal_config()).expect("parse config");

        // Then — an operator who writes no policy grants no privilege. Asserted against literal
        // empties rather than `SpawnPolicy::default()`, which would be satisfied by a permissive
        // default on both sides of the comparison.
        assert_eq!(
            config.spawn_policy.allowed_session_users,
            Vec::<String>::new()
        );
        assert_eq!(
            config.spawn_policy.allowed_tool_paths,
            Vec::<PathBuf>::new()
        );
        assert_eq!(
            config.spawn_policy.allowed_mount_roots,
            Vec::<PathBuf>::new()
        );
    }

    #[test]
    fn defaults_to_passing_on_no_environment_variable_a_caller_asks_for() {
        // Given / When
        let config = SupervisorConfig::from_yaml(a_minimal_config()).expect("parse config");

        // Then — a variable the caller can set on a process running as another user is a grant, so
        // an operator who wrote none has granted none.
        assert_eq!(config.spawn_policy.allowed_env_keys, Vec::<String>::new());
    }

    #[test]
    fn rejects_a_spawn_policy_that_lets_a_caller_preload_a_library_into_a_session() {
        // Given
        let yaml = "\
socket:
  path: /run/tddy-supervisor.sock
spawn_policy:
  allowed_env_keys: [RUST_LOG, LD_PRELOAD]
";

        // When / Then — `LD_PRELOAD` makes any allowlisted tool load code the caller chose, so
        // granting it would hand over everything `allowed_tool_paths` exists to withhold.
        assert_rejected_because(SupervisorConfig::from_yaml(yaml), "LD_PRELOAD");
    }

    #[test]
    fn parses_declared_services_in_order_with_their_restart_policy() {
        // Given
        let yaml = "\
socket:
  path: /run/tddy-supervisor.sock
services:
  - name: tddy-daemon
    exec_start: /usr/local/bin/tddy-daemon
    args: [\"-c\", \"/etc/tddy/daemon.yaml\"]
    user: tddy
    group: tddy
    working_dir: /var/lib/tddy
    env:
      RUST_LOG: info
    restart:
      max_retries: 3
      initial_backoff_ms: 250
      max_backoff_ms: 5000
      stability_threshold_ms: 15000
  - name: tddy-relay
    exec_start: /usr/local/bin/tddy-daemon
    user: tddy
";

        // When
        let config = SupervisorConfig::from_yaml(yaml).expect("parse config");

        // Then
        let names: Vec<&str> = config
            .services
            .iter()
            .map(|service| service.name.as_str())
            .collect();
        assert_eq!(names, vec!["tddy-daemon", "tddy-relay"]);

        let daemon = &config.services[0];
        assert_eq!(daemon.args, vec!["-c", "/etc/tddy/daemon.yaml"]);
        assert_eq!(daemon.user, "tddy");
        assert_eq!(daemon.group, Some("tddy".to_string()));
        assert_eq!(daemon.working_dir, Some(PathBuf::from("/var/lib/tddy")));
        assert_eq!(daemon.env.get("RUST_LOG"), Some(&"info".to_string()));
        assert_eq!(
            daemon.restart,
            RestartPolicy {
                max_retries: 3,
                initial_backoff_ms: 250,
                max_backoff_ms: 5_000,
                stability_threshold_ms: 15_000,
            }
        );
        assert_eq!(config.services[1].restart, RestartPolicy::default());
    }

    #[test]
    fn parses_a_service_that_declares_a_listening_socket() {
        // Given
        let yaml = "\
socket:
  path: /run/tddy-supervisor.sock
services:
  - name: tddy-daemon
    exec_start: /usr/local/bin/tddy-daemon
    user: tddy
    socket:
      path: /run/tddy-daemon.sock
      group: tddy-clients
      mode: \"0660\"
";

        // When
        let config = SupervisorConfig::from_yaml(yaml).expect("parse config");

        // Then
        assert_eq!(
            config.services[0].socket,
            Some(ServiceSocket {
                path: PathBuf::from("/run/tddy-daemon.sock"),
                group: Some("tddy-clients".to_string()),
                mode: "0660".to_string(),
            })
        );
    }

    #[test]
    fn declares_no_service_socket_by_default() {
        // Given a service that says nothing about a socket.
        let yaml = "\
socket:
  path: /run/tddy-supervisor.sock
services:
  - name: tddy-daemon
    exec_start: /usr/local/bin/tddy-daemon
    user: tddy
";

        // When
        let config = SupervisorConfig::from_yaml(yaml).expect("parse config");

        // Then — a service only gets a root-bound listening socket if somebody wrote one down.
        assert_eq!(config.services[0].socket, None);
    }

    #[test]
    fn rejects_a_relative_service_socket_path() {
        // Given
        let yaml = "\
socket:
  path: /run/tddy-supervisor.sock
services:
  - name: tddy-daemon
    exec_start: /usr/local/bin/tddy-daemon
    user: tddy
    socket:
      path: run/tddy-daemon.sock
";

        // When / Then
        assert_rejected_because(SupervisorConfig::from_yaml(yaml), "absolute");
    }

    #[test]
    fn rejects_a_service_socket_mode_that_is_not_octal() {
        // Given
        let yaml = "\
socket:
  path: /run/tddy-supervisor.sock
services:
  - name: tddy-daemon
    exec_start: /usr/local/bin/tddy-daemon
    user: tddy
    socket:
      path: /run/tddy-daemon.sock
      mode: \"srw-rw----\"
";

        // When / Then
        assert_rejected_because(SupervisorConfig::from_yaml(yaml), "mode");
    }

    #[test]
    fn rejects_two_services_declaring_the_same_socket_path() {
        // Given
        let yaml = "\
socket:
  path: /run/tddy-supervisor.sock
services:
  - name: tddy-daemon
    exec_start: /usr/local/bin/tddy-daemon
    user: tddy
    socket:
      path: /run/tddy-shared.sock
  - name: tddy-relay
    exec_start: /usr/local/bin/tddy-daemon
    user: tddy
    socket:
      path: /run/tddy-shared.sock
";

        // When / Then — two services cannot both own one listener; whichever bound second would
        // silently take over the first's clients.
        assert_rejected_because(SupervisorConfig::from_yaml(yaml), "/run/tddy-shared.sock");
    }

    #[test]
    fn rejects_an_unknown_top_level_key() {
        // Given
        let yaml = "socket:\n  path: /run/tddy-supervisor.sock\nallow_everything: true\n";

        // When / Then — a typo'd security setting must fail loudly, not be ignored.
        assert_rejected_because(SupervisorConfig::from_yaml(yaml), "allow_everything");
    }

    #[test]
    fn rejects_an_unknown_key_inside_a_declared_service() {
        // Given
        let yaml = "\
socket:
  path: /run/tddy-supervisor.sock
services:
  - name: tddy-daemon
    exec_start: /usr/local/bin/tddy-daemon
    user: tddy
    capabilities: [CAP_SYS_ADMIN]
";

        // When / Then
        assert_rejected_because(SupervisorConfig::from_yaml(yaml), "capabilities");
    }

    #[test]
    fn rejects_a_service_declared_to_run_as_root() {
        // Given
        let yaml = "\
socket:
  path: /run/tddy-supervisor.sock
services:
  - name: tddy-daemon
    exec_start: /usr/local/bin/tddy-daemon
    user: root
";

        // When / Then — a root child would defeat the entire point of the supervisor.
        assert_rejected_because(SupervisorConfig::from_yaml(yaml), "root");
    }

    #[test]
    fn rejects_a_spawn_policy_that_allows_sessions_to_run_as_root() {
        // Given
        let yaml = "\
socket:
  path: /run/tddy-supervisor.sock
spawn_policy:
  allowed_session_users: [alice, root]
";

        // When / Then
        assert_rejected_because(SupervisorConfig::from_yaml(yaml), "root");
    }

    #[test]
    fn rejects_two_services_sharing_a_name() {
        // Given
        let yaml = "\
socket:
  path: /run/tddy-supervisor.sock
services:
  - name: tddy-daemon
    exec_start: /usr/local/bin/tddy-daemon
    user: tddy
  - name: tddy-daemon
    exec_start: /usr/local/bin/tddy-daemon
    user: tddy
";

        // When / Then — names address services over RPC, so duplicates make Start/Stop ambiguous.
        // Matched on the reason rather than on the name, which also appears in `exec_start` and
        // would let an unrelated rejection satisfy this.
        assert_rejected_because(SupervisorConfig::from_yaml(yaml), "share the name");
    }

    #[test]
    fn rejects_a_relative_exec_start_path() {
        // Given
        let yaml = "\
socket:
  path: /run/tddy-supervisor.sock
services:
  - name: tddy-daemon
    exec_start: target/release/tddy-daemon
    user: tddy
";

        // When / Then
        assert_rejected_because(SupervisorConfig::from_yaml(yaml), "absolute");
    }

    #[test]
    fn rejects_a_relative_tool_path_in_the_spawn_policy() {
        // Given
        let yaml = "\
socket:
  path: /run/tddy-supervisor.sock
spawn_policy:
  allowed_tool_paths: [\"bin/tddy-coder\"]
";

        // When / Then — a relative entry could never match a resolved request anyway, so it is a
        // silently dead grant rather than a working one.
        assert_rejected_because(SupervisorConfig::from_yaml(yaml), "absolute");
    }

    // -----------------------------------------------------------------------------------------
    // Trusting the file the policy came from
    // -----------------------------------------------------------------------------------------

    /// Writes `a_minimal_config()` into a fresh directory and hands back both paths, so a test can
    /// spoil exactly one of them.
    fn a_policy_file_on_disk() -> (tempfile::TempDir, PathBuf) {
        let directory = tempfile::TempDir::new().expect("create a config directory");
        let path = directory.path().join("supervisor.yaml");
        std::fs::write(&path, a_minimal_config()).expect("write the policy file");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640))
            .expect("restrict the policy file");
        (directory, path)
    }

    #[test]
    fn loads_a_policy_file_that_only_the_supervisor_can_write() {
        // Given
        let (_directory, path) = a_policy_file_on_disk();

        // When
        let config = SupervisorConfig::load(&path).expect("load the policy file");

        // Then
        assert_eq!(
            config.socket.path,
            PathBuf::from("/run/tddy-supervisor.sock")
        );
    }

    #[test]
    fn refuses_a_policy_file_that_anybody_on_the_host_may_write() {
        // Given a policy file left world-writable — `install` creating `/etc/tddy` under an
        // inherited `umask 000` is enough to produce exactly this.
        let (_directory, path) = a_policy_file_on_disk();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o666))
            .expect("make the policy file world-writable");

        // When / Then — whoever can write this file decides which OS users the root broker will
        // spawn as, so it is not a policy file at all.
        assert_rejected_because(SupervisorConfig::load(&path), "writable");
    }

    #[test]
    fn refuses_a_policy_file_that_anybody_may_replace_through_its_directory() {
        // Given a directory anybody may write, and no sticky bit to stop them unlinking what is in
        // it. The file itself is untouched: the escalation is replacing it, not editing it.
        let (directory, path) = a_policy_file_on_disk();
        std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o777))
            .expect("make the config directory world-writable");

        // When / Then
        assert_rejected_because(SupervisorConfig::load(&path), "could be replaced");
    }

    #[test]
    fn loads_a_policy_file_named_by_a_relative_path() {
        // Given a config named the way an operator running the binary by hand would name it.
        let (directory, path) = a_policy_file_on_disk();
        let relative = pathdiff_from_current_directory(&path);

        // When
        let config = SupervisorConfig::load(&relative).expect("load the policy file");

        // Then — the directories checked are the ones the path resolves to, not the ones it spells.
        assert_eq!(
            config.socket.path,
            PathBuf::from("/run/tddy-supervisor.sock")
        );
        drop(directory);
    }

    /// `path` as a relative path from the process's current directory.
    fn pathdiff_from_current_directory(path: &Path) -> PathBuf {
        let current = std::env::current_dir().expect("read the current directory");
        let mut relative = PathBuf::new();
        for _ in current.components().skip(1) {
            relative.push("..");
        }
        relative.join(path.strip_prefix("/").expect("an absolute path"))
    }

    #[test]
    fn refuses_a_policy_file_that_is_not_there() {
        // Given
        let directory = tempfile::TempDir::new().expect("create a config directory");

        // When / Then — no default policy is invented for a missing file; the supervisor has nothing
        // to enforce and says so.
        assert_rejected_because(
            SupervisorConfig::load(&directory.path().join("absent.yaml")),
            "could not read",
        );
    }

    #[test]
    fn rejects_a_relative_mount_root_in_the_spawn_policy() {
        // Given
        let yaml = "\
socket:
  path: /run/tddy-supervisor.sock
spawn_policy:
  allowed_mount_roots: [\"repos\"]
";

        // When / Then
        assert_rejected_because(SupervisorConfig::from_yaml(yaml), "absolute");
    }
}
