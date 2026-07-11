//! Linux rootless sandbox backend: cgroup v2 resource limits + user/mount/network/pid namespaces.
//!
//! Mirrors `tddy-sandbox-darwin`'s contract — `spawn(SandboxSpec) -> Result<SandboxHandle>` — but
//! confines the runner with unprivileged user namespaces, cgroup v2 resource limits, and a
//! no-egress network namespace that forces outbound traffic through the in-jail `HTTPS_PROXY`. The
//! gRPC control channel is served over an AF_UNIX socket (in the runner argv), which survives the
//! network namespace where loopback TCP cannot.
#![cfg(target_os = "linux")]

use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

use nix::mount::MsFlags;
use nix::sched::{unshare, CloneFlags};
use tddy_sandbox::{SandboxError, SandboxHandle, SandboxPlan, SandboxSpec};

/// Map a plan's read grants to read-only bind-mount operations applied inside the rootless jail.
///
/// Each [`tddy_sandbox::ReadSpec`] becomes a `(source, target, flags)` tuple: an `MS_BIND` mount
/// remounted `MS_RDONLY` (plus `MS_NOEXEC` for non-exec reads). Pure (no syscalls) so the mapping
/// is unit-testable without mounting — the actual `mount(2)` calls happen in `enter_rootless_jail`.
pub fn plan_to_bind_mounts(plan: &SandboxPlan) -> Vec<(PathBuf, PathBuf, MsFlags)> {
    use tddy_sandbox::ReadKind;
    let mut mounts: Vec<(PathBuf, PathBuf, MsFlags)> = plan
        .reads
        .iter()
        .filter(|r| !matches!(r.kind, ReadKind::Regex(_)))
        .map(|r| {
            let target = r.jail.clone().unwrap_or_else(|| r.host.clone());
            let mut flags = MsFlags::MS_BIND | MsFlags::MS_RDONLY;
            if !r.exec {
                flags |= MsFlags::MS_NOEXEC;
            }
            (r.host.clone(), target, flags)
        })
        .collect();
    // Mounted directories (e.g. the project repo): read-write when requested, exec allowed.
    for m in &plan.mounts {
        let target = m.jail.clone().unwrap_or_else(|| m.host.clone());
        let mut flags = MsFlags::MS_BIND;
        if !m.writable {
            flags |= MsFlags::MS_RDONLY;
        }
        mounts.push((m.host.clone(), target, flags));
    }
    mounts
}

/// Map a plan's [`tddy_sandbox::ResourceLimits`] onto the cgroup v2 [`CgroupLimits`] applied to the
/// jail scope. Pure so the mapping is unit-testable.
pub fn cgroup_limits_from(limits: &tddy_sandbox::ResourceLimits) -> CgroupLimits {
    CgroupLimits {
        memory_max: limits.memory_max,
        cpu_max: limits.cpu_max.clone(),
        pids_max: limits.pids_max,
    }
}

/// Spawn a sandboxed process from an explicit [`SandboxPlan`]: RO bind-mount the declared reads,
/// materialize copies/symlinks/secrets, apply env + cgroup limits from the plan.
///
/// FIXME(fs-confinement): the declared reads become read-only bind mounts, but the jail still shares
/// the host filesystem root — full minimal-root `pivot_root` write-confinement is a follow-up.
pub fn spawn_plan(plan: SandboxPlan) -> Result<SandboxHandle, SandboxError> {
    plan.spec.validate()?;
    for dir in [
        &plan.spec.project_root,
        &plan.spec.scratch_dir,
        &plan.spec.egress_dir,
    ] {
        std::fs::create_dir_all(dir).map_err(|e| SandboxError::Io(e.to_string()))?;
    }

    if !unprivileged_userns_available() {
        return Err(userns_unsupported_error());
    }

    tddy_sandbox::materialize_copies(&plan.copies).map_err(SandboxError::Io)?;
    tddy_sandbox::materialize_symlinks(&plan.symlinks).map_err(SandboxError::Io)?;
    tddy_sandbox::materialize_secrets(&plan.env.secrets, &plan.spec.scratch_dir)
        .map_err(SandboxError::Io)?;

    let grpc_socket = arg_value(&plan.spec.command, "--grpc-uds")
        .map(PathBuf::from)
        .unwrap_or_else(|| plan.spec.project_root.join("sandbox.grpc.sock"));
    let ready_marker = arg_value(&plan.spec.command, "--ready-marker")
        .map(PathBuf::from)
        .unwrap_or_else(|| plan.spec.project_root.join("sandbox.ready"));

    // Derive (and, once per daemon, prepare) the delegated cgroup base before spawning, so the
    // daemon is relocated into its supervisor leaf before it forks the child. Fails fast when no
    // writable cgroup v2 subtree is available — never a silent degrade to an unconfined process.
    let base = detect_and_prepare_base(&plan.cgroup)?;
    let scope = scope_dir_in(&base, &session_name_from(&plan.spec), next_seq());
    std::fs::create_dir_all(&scope).map_err(|e| cgroup_unsupported_error(&scope, &e))?;

    let uid = nix::unistd::geteuid().as_raw();
    let gid = nix::unistd::getegid().as_raw();
    let uid_map = format!("0 {uid} 1\n");
    let gid_map = format!("0 {gid} 1\n");
    let bind_mounts = plan_to_bind_mounts(&plan);

    // `--stdio` dedicates the sandboxed process's stdin/stdout to RPC framing (see
    // `tddy_core::stdio_safety`, `tddy-stdio`) instead of the `--grpc-uds`/`--grpc-listen-port`
    // transport — pipe both back to the caller, mirroring `tddy-sandbox-darwin::spawn_plan`.
    let stdio_mode = plan.spec.command.iter().any(|arg| arg == "--stdio");

    let mut cmd = Command::new(&plan.spec.command[0]);
    cmd.args(&plan.spec.command[1..]);
    cmd.env_clear();
    cmd.envs(&plan.spec.env);
    cmd.current_dir(plan.spec.cwd.as_ref().unwrap_or(&plan.spec.project_root));
    cmd.stdin(if stdio_mode || plan.stdin.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    });
    if stdio_mode {
        cmd.stdout(Stdio::piped());
    }

    // SAFETY: runs in the forked child before `execve`; only namespace setup + RO bind mounts.
    unsafe {
        cmd.pre_exec(move || {
            enter_rootless_jail(&uid_map, &gid_map)?;
            apply_bind_mounts(&bind_mounts)?;
            Ok(())
        });
    }

    let mut child = cmd.spawn().map_err(|e| {
        SandboxError::Io(format!(
            "spawn sandbox runner in cgroups jail failed: {e} \
             (the host may forbid unprivileged user namespaces)"
        ))
    })?;

    if let Some(stdin_bytes) = &plan.stdin {
        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            stdin
                .write_all(stdin_bytes)
                .map_err(|e| SandboxError::Io(format!("write sandbox stdin: {e}")))?;
        }
    }

    if let Err(e) = move_pid_into_scope(&scope, child.id()) {
        let _ = child.kill();
        return Err(cgroup_unsupported_error(&scope, &e));
    }
    let limits = cgroup_limits_from(&plan.limits);
    let limits =
        if limits.memory_max.is_none() && limits.cpu_max.is_none() && limits.pids_max.is_none() {
            default_limits()
        } else {
            limits
        };
    if let Err(e) = write_cgroup_limits(&scope, &limits) {
        log::warn!(
            target: "tddy_sandbox_cgroups",
            "some cgroup limits could not be applied in {}: {e}",
            scope.display()
        );
    }

    Ok(SandboxHandle::new(
        child,
        plan.spec.profile_path,
        grpc_socket,
        ready_marker,
    ))
}

/// Apply each `(source, target, flags)` as a read-only bind mount in the child's mount namespace.
/// Missing sources are skipped so an absent optional toolchain dir never aborts the spawn.
fn apply_bind_mounts(mounts: &[(PathBuf, PathBuf, MsFlags)]) -> std::io::Result<()> {
    let errno = |e: nix::Error| std::io::Error::from_raw_os_error(e as i32);
    for (src, target, flags) in mounts {
        if !src.exists() || !target.exists() {
            continue;
        }
        nix::mount::mount(
            Some(src.as_path()),
            target.as_path(),
            None::<&str>,
            MsFlags::MS_BIND | MsFlags::MS_REC,
            None::<&str>,
        )
        .map_err(errno)?;
        nix::mount::mount(
            None::<&str>,
            target.as_path(),
            None::<&str>,
            MsFlags::MS_BIND | MsFlags::MS_REMOUNT | *flags,
            None::<&str>,
        )
        .map_err(errno)?;
    }
    Ok(())
}

/// Default resource limits applied when the spec carries none (memory 2 GiB, 1 CPU, 512 pids).
fn default_limits() -> CgroupLimits {
    CgroupLimits {
        memory_max: Some(2 * 1024 * 1024 * 1024),
        cpu_max: Some("100000 100000".to_string()),
        pids_max: Some(512),
    }
}

/// cgroup v2 resource limits applied to the jail's scope. Green maps the new `SandboxSpec` limit
/// fields onto this.
#[derive(Debug, Clone, Default)]
pub struct CgroupLimits {
    /// `memory.max` in bytes.
    pub memory_max: Option<u64>,
    /// `cpu.max` value, e.g. `"50000 100000"` (50% of one CPU) or `"max"`.
    pub cpu_max: Option<String>,
    /// `pids.max`.
    pub pids_max: Option<u64>,
}

/// Spawn the sandbox runner confined by a rootless jail: an unprivileged user namespace (uid/gid
/// mapped to root-in-ns), a network namespace with only loopback up (no direct egress — outbound
/// traffic must go through the in-jail `HTTPS_PROXY`), a private mount namespace, and a cgroup v2
/// scope with resource limits. The gRPC `SessionChannel` is served over the AF_UNIX path in the
/// runner argv (`--grpc-uds`), which the host reaches on the shared filesystem.
///
/// Fails fast — never degrades to an unconfined or unlimited process — when the host forbids
/// unprivileged user namespaces ([`userns_unsupported_error`]) or has no writable cgroup v2 subtree.
///
/// FIXME(fs-confinement): this first cut isolates network + resources + uids but does not yet
/// `pivot_root` into a minimal read-only root; full filesystem write-confinement via bind mounts is
/// a follow-up. The network namespace (the egress guarantee) and cgroup limits are in place.
pub fn spawn(spec: SandboxSpec) -> Result<SandboxHandle, SandboxError> {
    spec.validate()?;
    for dir in [&spec.project_root, &spec.scratch_dir, &spec.egress_dir] {
        std::fs::create_dir_all(dir).map_err(|e| SandboxError::Io(e.to_string()))?;
    }

    if !unprivileged_userns_available() {
        return Err(userns_unsupported_error());
    }

    let grpc_socket = arg_value(&spec.command, "--grpc-uds")
        .map(PathBuf::from)
        .unwrap_or_else(|| spec.project_root.join("sandbox.grpc.sock"));
    let ready_marker = arg_value(&spec.command, "--ready-marker")
        .map(PathBuf::from)
        .unwrap_or_else(|| spec.project_root.join("sandbox.ready"));

    // cgroup scope created before spawn so limits apply from the start. A host without a writable
    // cgroup v2 subtree fails fast (no silent degrade to an unlimited process). The delegated base
    // is prepared (and the daemon relocated) before the child is forked.
    let base = detect_and_prepare_base(&tddy_sandbox::CgroupConfig::default())?;
    let scope = scope_dir_in(&base, &session_name_from(&spec), next_seq());
    std::fs::create_dir_all(&scope).map_err(|e| cgroup_unsupported_error(&scope, &e))?;

    let uid = nix::unistd::geteuid().as_raw();
    let gid = nix::unistd::getegid().as_raw();
    let uid_map = format!("0 {uid} 1\n");
    let gid_map = format!("0 {gid} 1\n");

    let mut cmd = Command::new(&spec.command[0]);
    cmd.args(&spec.command[1..]);
    cmd.env_clear();
    cmd.envs(&spec.env);

    // SAFETY: the closure runs in the forked child before `execve`. It performs only namespace
    // setup syscalls and small `/proc` writes; no shared state is mutated.
    unsafe {
        cmd.pre_exec(move || {
            enter_rootless_jail(&uid_map, &gid_map)?;
            Ok(())
        });
    }

    let mut child = cmd.spawn().map_err(|e| {
        // A failure here is almost always the userns/namespace setup being denied.
        SandboxError::Io(format!(
            "spawn sandbox runner in cgroups jail failed: {e} \
             (the host may forbid unprivileged user namespaces)"
        ))
    })?;

    // Move the jailed process into its scope so the limits bind it. A failure here means it would
    // run uncgrouped — kill it and fail rather than continue unconfined.
    if let Err(e) = move_pid_into_scope(&scope, child.id()) {
        let _ = child.kill();
        return Err(cgroup_unsupported_error(&scope, &e));
    }
    // Per-controller limit writes are best-effort: the process is already confined to the scope,
    // and a missing controller (e.g. cpu not delegated) shouldn't tear down an otherwise valid jail.
    if let Err(e) = write_cgroup_limits(&scope, &default_limits()) {
        log::warn!(
            target: "tddy_sandbox_cgroups",
            "some cgroup limits could not be applied in {}: {e}",
            scope.display()
        );
    }

    Ok(SandboxHandle::new(
        child,
        spec.profile_path,
        grpc_socket,
        ready_marker,
    ))
}

/// Find the value following `flag` in an argv vector.
fn arg_value(argv: &[String], flag: &str) -> Option<String> {
    argv.iter()
        .position(|a| a == flag)
        .and_then(|i| argv.get(i + 1))
        .cloned()
}

/// Whether the current process can create an unprivileged user namespace. Root always can; an
/// unprivileged process is decided by a real functional probe (a fork that attempts
/// `unshare(CLONE_NEWUSER)`), which sees a per-binary AppArmor `userns` grant that a sysctl read
/// cannot.
pub fn unprivileged_userns_available() -> bool {
    unprivileged_userns_available_with(nix::unistd::geteuid().is_root(), probe_unprivileged_userns)
}

// ---------------------------------------------------------------------------------------------
// Unprivileged-`User=tddy` cgroups sandbox support (functional userns probe + delegated cgroup
// base under systemd `Delegate=yes`). Stubs below are pinned by RED-phase tests; `/green` fills
// in the bodies. Exposed `pub` for testability, matching the crate's existing helper convention.
// ---------------------------------------------------------------------------------------------

/// Outcome of attempting to enter a fresh unprivileged user namespace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsernsProbe {
    Available,
    Denied,
}

/// Default cgroup v2 unified mount point (systemd's conventional location). A *default*, not a
/// hardcoded assumption: overridable via config and derivable from `/proc/self/mountinfo`.
const DEFAULT_CGROUP_MOUNT_ROOT: &str = "/sys/fs/cgroup";

/// Pure decision: root can always create a user namespace; otherwise trust the functional probe.
/// Kept pure (no host state) so it is unit-testable without root or AppArmor.
pub fn userns_available_from(is_root: bool, probe: UsernsProbe) -> bool {
    is_root || probe == UsernsProbe::Available
}

/// Injectable seam over the functional probe: root short-circuits without attempting; otherwise the
/// decision follows the injected attempt's result (never the sysctl). Unit-testable via a closure.
pub fn unprivileged_userns_available_with(
    is_root: bool,
    attempt: impl FnOnce() -> UsernsProbe,
) -> bool {
    if is_root {
        true
    } else {
        userns_available_from(false, attempt())
    }
}

/// Real functional probe (host-touching, not unit-tested): fork a child that calls
/// `unshare(CLONE_NEWUSER)` and `_exit(0/1)` using only async-signal-safe ops; parent `waitpid`s and
/// maps exit status. Detects a per-binary AppArmor `userns` grant that the sysctl read cannot see.
fn probe_unprivileged_userns() -> UsernsProbe {
    // Format the id-maps in the parent (heap is fine here). The strings stay valid in the forked
    // child via copy-on-write, so the child needs no allocation to reference them.
    let uid_map = format!("0 {} 1\n", unsafe { libc::geteuid() });
    let gid_map = format!("0 {} 1\n", unsafe { libc::getegid() });

    // SAFETY: `fork` in a multi-threaded process yields a child in which only the forking thread
    // exists, so the child must call only async-signal-safe functions. It does exactly that:
    // `unshare(2)`, raw `open`/`write`/`close` (via `probe_write`), and `_exit(2)` — no heap
    // allocation, no locking, no logging, no Rust destructors.
    unsafe {
        let pid = libc::fork();
        if pid < 0 {
            return UsernsProbe::Denied;
        }
        if pid == 0 {
            // Child: replicate the jail's userns setup end-to-end. The AppArmor restriction gates the
            // uid/gid *mapping* writes, not `unshare` itself (which succeeds unprivileged), so probing
            // only `unshare` would falsely report availability without the grant. Mirror
            // `enter_rootless_jail`: map uid, deny setgroups (best-effort), then map gid.
            if libc::unshare(libc::CLONE_NEWUSER) != 0 {
                libc::_exit(1);
            }
            if !probe_write(c"/proc/self/uid_map".as_ptr(), uid_map.as_bytes()) {
                libc::_exit(1);
            }
            probe_write(c"/proc/self/setgroups".as_ptr(), b"deny");
            if !probe_write(c"/proc/self/gid_map".as_ptr(), gid_map.as_bytes()) {
                libc::_exit(1);
            }
            libc::_exit(0);
        }
        // Parent: reap the child and map its exit status.
        let mut status: libc::c_int = 0;
        if libc::waitpid(pid, &mut status, 0) < 0 {
            return UsernsProbe::Denied;
        }
        if libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 0 {
            UsernsProbe::Available
        } else {
            UsernsProbe::Denied
        }
    }
}

/// Async-signal-safe write of `buf` to the nul-terminated `path`. Called only from the forked probe
/// child, so it uses raw libc `open`/`write`/`close` — no allocation, locking, or Rust destructors.
/// Returns true only when the entire buffer was written.
///
/// # Safety
/// `path` must be a valid nul-terminated C string pointer.
unsafe fn probe_write(path: *const libc::c_char, buf: &[u8]) -> bool {
    let fd = libc::open(path, libc::O_WRONLY);
    if fd < 0 {
        return false;
    }
    let mut written = 0usize;
    let mut ok = true;
    while written < buf.len() {
        let n = libc::write(
            fd,
            buf[written..].as_ptr() as *const libc::c_void,
            buf.len() - written,
        );
        if n <= 0 {
            ok = false;
            break;
        }
        written += n as usize;
    }
    libc::close(fd);
    ok
}

/// Pure: extract the cgroup v2 relative path from `/proc/self/cgroup` contents — the single
/// `0::<path>` line. `"0::/system.slice/x.service\n"` -> `Some("/system.slice/x.service")`;
/// `"0::/"` -> `Some("/")`; no `0::` line (v1-only host) -> `None`.
pub fn cgroup_v2_relative_path(proc_self_cgroup: &str) -> Option<String> {
    proc_self_cgroup
        .lines()
        .find_map(|line| line.strip_prefix("0::"))
        .map(|path| path.trim_end_matches('\n').to_string())
}

/// Pure: find the cgroup2 mount point from `/proc/self/mountinfo` contents (fstype `cgroup2`).
pub fn cgroup2_mount_root_from(mountinfo: &str) -> Option<PathBuf> {
    for line in mountinfo.lines() {
        let Some((left, right)) = line.split_once(" - ") else {
            continue;
        };
        if right.split_whitespace().next() == Some("cgroup2") {
            if let Some(mount_point) = left.split_whitespace().nth(4) {
                return Some(PathBuf::from(mount_point));
            }
        }
    }
    None
}

/// Pure: join the mount root with the v2 relative path (strip the leading `/` so `join` does not
/// replace the root). `("/sys/fs/cgroup", "/a/b")` -> `/sys/fs/cgroup/a/b`; `(_, "/")` -> mount root.
pub fn delegated_cgroup_base_from(mount_root: &Path, relative: &str) -> PathBuf {
    mount_root.join(relative.trim_start_matches('/'))
}

/// Pure precedence resolver: config `base_override` wins; else derive from `/proc/self/cgroup`
/// joined with (config `mount_root` -> `cgroup2_mount_root_from(mountinfo)` -> `default_mount_root`).
pub fn resolve_cgroup_base(
    cfg: &tddy_sandbox::CgroupConfig,
    proc_self_cgroup: &str,
    mountinfo: &str,
    default_mount_root: &Path,
) -> Option<PathBuf> {
    if let Some(base) = &cfg.base_override {
        return Some(base.clone());
    }
    let relative = cgroup_v2_relative_path(proc_self_cgroup)?;
    let mount_root = cfg
        .mount_root
        .clone()
        .or_else(|| cgroup2_mount_root_from(mountinfo))
        .unwrap_or_else(|| default_mount_root.to_path_buf());
    Some(delegated_cgroup_base_from(&mount_root, &relative))
}

/// Pure: configured controllers, or `[memory, cpu, pids]` when unset.
pub fn controllers_or_default(cfg: &tddy_sandbox::CgroupConfig) -> Vec<String> {
    if cfg.controllers.is_empty() {
        vec!["memory".to_string(), "cpu".to_string(), "pids".to_string()]
    } else {
        cfg.controllers.clone()
    }
}

/// Pure: format the `cgroup.subtree_control` enable line, e.g. `"+memory +cpu +pids"`.
pub fn subtree_control_line(controllers: &[String]) -> String {
    controllers
        .iter()
        .map(|c| format!("+{c}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Pure: the per-session scope directory under `base`, uniquely named via `seq`.
pub fn scope_dir_in(base: &Path, session_name: &str, seq: u64) -> PathBuf {
    base.join(format!("tddy-{session_name}-{seq}.scope"))
}

/// Seam: create `base/<leaf>` and move the daemon's own thread group into it by writing `self_pid`
/// (the TGID) to `base/<leaf>/cgroup.procs`, satisfying cgroup v2's no-internal-processes rule.
pub fn relocate_self_into_leaf(base: &Path, self_pid: u32, leaf: &str) -> std::io::Result<()> {
    let leaf_dir = base.join(leaf);
    std::fs::create_dir_all(&leaf_dir)?;
    std::fs::write(leaf_dir.join("cgroup.procs"), self_pid.to_string())
}

/// Seam: enable the given controllers in `base/cgroup.subtree_control`.
pub fn enable_controllers(base: &Path, controllers: &[String]) -> std::io::Result<()> {
    std::fs::write(
        base.join("cgroup.subtree_control"),
        subtree_control_line(controllers),
    )
}

/// Seam: move `pid` into `scope` by writing it to `scope/cgroup.procs`.
pub fn move_pid_into_scope(scope: &Path, pid: u32) -> std::io::Result<()> {
    std::fs::write(scope.join("cgroup.procs"), pid.to_string())
}

/// Host-touching orchestrator (one-time per daemon process, `OnceLock`-guarded): resolve the
/// delegated base, relocate self into the supervisor leaf, and enable controllers. Caches the base
/// or the error so every later spawn is fail-fast and never re-degrades.
fn detect_and_prepare_base(cfg: &tddy_sandbox::CgroupConfig) -> Result<PathBuf, SandboxError> {
    static PREPARED_BASE: OnceLock<Result<PathBuf, String>> = OnceLock::new();
    let result = PREPARED_BASE.get_or_init(|| {
        let proc_self_cgroup = std::fs::read_to_string("/proc/self/cgroup").unwrap_or_default();
        let mountinfo = std::fs::read_to_string("/proc/self/mountinfo").unwrap_or_default();
        let base = match resolve_cgroup_base(
            cfg,
            &proc_self_cgroup,
            &mountinfo,
            Path::new(DEFAULT_CGROUP_MOUNT_ROOT),
        ) {
            Some(base) => base,
            None => {
                return Err("cgroup v2 delegation unavailable (no unified cgroup v2 hierarchy in \
                     /proc/self/cgroup); the cgroups sandbox requires a writable cgroup v2 subtree. \
                     Run the daemon via systemd with `Delegate=yes` (or as a root service)."
                    .to_string());
            }
        };
        // cgroup v2 forbids a cgroup from both holding processes and delegating controllers, so the
        // daemon moves its own thread group into a leaf before enabling controllers on the base.
        let leaf = cfg.supervisor_leaf.as_deref().unwrap_or("supervisor");
        if let Err(e) = relocate_self_into_leaf(&base, std::process::id(), leaf) {
            return Err(format!(
                "cgroup v2 delegation unavailable for {} ({e}); the cgroups sandbox requires a \
                 writable cgroup v2 subtree. Run the daemon via systemd with `Delegate=yes` (or as \
                 a root service).",
                base.display()
            ));
        }
        // Best-effort: some hosts do not delegate every controller. The base is still usable for
        // scopes even if a controller can't be enabled, so warn rather than fail.
        if let Err(e) = enable_controllers(&base, &controllers_or_default(cfg)) {
            log::warn!(
                target: "tddy_sandbox_cgroups",
                "some cgroup controllers could not be enabled in {}: {e}",
                base.display()
            );
        }
        Ok(base)
    });
    match result {
        Ok(base) => Ok(base.clone()),
        Err(message) => Err(SandboxError::Unsupported {
            platform: "linux".to_string(),
            message: message.clone(),
        }),
    }
}

/// Session name for a spawn's scope, derived from the project root's final component.
fn session_name_from(spec: &SandboxSpec) -> String {
    spec.project_root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("session")
        .to_string()
}

/// Monotonic, process-global sequence so two concurrent sessions of one project never share a scope
/// name (the old `tddy-<project>-<daemon_pid>.scope` scheme collided on the constant daemon pid).
fn next_seq() -> u64 {
    static SCOPE_SEQ: AtomicU64 = AtomicU64::new(0);
    SCOPE_SEQ.fetch_add(1, Ordering::Relaxed)
}

/// Child-side jail setup (runs in the forked child before `execve`): user namespace with the
/// caller mapped to root-in-ns, then a private mount namespace and a network namespace with only
/// loopback up. Returns an `io::Error` so a failure aborts the spawn.
fn enter_rootless_jail(uid_map: &str, gid_map: &str) -> std::io::Result<()> {
    let errno = |e: nix::Error| std::io::Error::from_raw_os_error(e as i32);

    unshare(CloneFlags::CLONE_NEWUSER).map_err(errno)?;
    std::fs::write("/proc/self/uid_map", uid_map)?;
    // setgroups must be denied before writing gid_map for an unprivileged single-id mapping.
    let _ = std::fs::write("/proc/self/setgroups", "deny");
    std::fs::write("/proc/self/gid_map", gid_map)?;

    unshare(CloneFlags::CLONE_NEWNS | CloneFlags::CLONE_NEWNET).map_err(errno)?;

    // Don't leak mount changes back to the host.
    nix::mount::mount(
        None::<&str>,
        "/",
        None::<&str>,
        nix::mount::MsFlags::MS_REC | nix::mount::MsFlags::MS_PRIVATE,
        None::<&str>,
    )
    .map_err(errno)?;

    bring_loopback_up()?;
    Ok(())
}

/// Bring the `lo` interface up inside the new network namespace (so the in-jail `HTTPS_PROXY`
/// shim can bind `127.0.0.1`). Uses `SIOCSIFFLAGS` — there is no `ip` binary to rely on.
fn bring_loopback_up() -> std::io::Result<()> {
    // struct ifreq: char ifr_name[16]; short ifr_flags; (+ padding)
    #[repr(C)]
    struct IfReq {
        ifr_name: [libc::c_char; libc::IF_NAMESIZE],
        ifr_flags: libc::c_short,
        _pad: [u8; 22],
    }

    let fd = unsafe { libc::socket(libc::AF_INET, libc::SOCK_DGRAM, 0) };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    let mut req: IfReq = unsafe { std::mem::zeroed() };
    let name = b"lo";
    for (i, b) in name.iter().enumerate() {
        req.ifr_name[i] = *b as libc::c_char;
    }
    let rc = unsafe { libc::ioctl(fd, libc::SIOCGIFFLAGS, &mut req) };
    if rc < 0 {
        let e = std::io::Error::last_os_error();
        unsafe { libc::close(fd) };
        return Err(e);
    }
    req.ifr_flags |= (libc::IFF_UP | libc::IFF_RUNNING) as libc::c_short;
    let rc = unsafe { libc::ioctl(fd, libc::SIOCSIFFLAGS, &req) };
    let result = if rc < 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    };
    unsafe { libc::close(fd) };
    result
}

/// Linux toolchain/library read-allow paths for the jail (analogue of the Darwin detector).
///
/// The runner (a dynamically-linked Rust/Node process) needs the system interpreter, shared
/// libraries, and the standard executable dirs available read-only inside the jail. Only paths
/// that exist on the host are returned so the bind-mount step never fails on a missing source.
pub fn detect_allow_read_paths() -> Vec<PathBuf> {
    [
        "/usr/bin",
        "/bin",
        "/usr/lib",
        "/lib",
        "/lib64",
        "/usr/lib64",
        "/etc/ssl/certs",
        "/etc/resolv.conf",
    ]
    .into_iter()
    .map(PathBuf::from)
    .filter(|p| p.exists())
    .collect()
}

/// Write cgroup v2 limit files (`memory.max`, `cpu.max`, `pids.max`) into the delegated scope dir.
/// Only fields that are `Some` are written.
pub fn write_cgroup_limits(scope_dir: &Path, limits: &CgroupLimits) -> std::io::Result<()> {
    if let Some(memory_max) = limits.memory_max {
        std::fs::write(scope_dir.join("memory.max"), memory_max.to_string())?;
    }
    if let Some(cpu_max) = &limits.cpu_max {
        std::fs::write(scope_dir.join("cpu.max"), cpu_max)?;
    }
    if let Some(pids_max) = limits.pids_max {
        std::fs::write(scope_dir.join("pids.max"), pids_max.to_string())?;
    }
    Ok(())
}

/// The error returned when the host has no writable cgroup v2 subtree. Fails fast — the jail never
/// silently degrades to an unlimited (uncgrouped) process.
fn cgroup_unsupported_error(scope: &Path, err: &std::io::Error) -> SandboxError {
    SandboxError::Unsupported {
        platform: "linux".to_string(),
        message: format!(
            "cgroup v2 delegation unavailable for {} ({err}); the cgroups sandbox requires a \
             writable cgroup v2 subtree. Run the daemon via systemd with `Delegate=yes` (or as a \
             root service).",
            scope.display()
        ),
    }
}

/// The error returned when the host cannot provide unprivileged user namespaces. Fails fast — the
/// jail never silently degrades to an unconfined session.
pub fn userns_unsupported_error() -> SandboxError {
    SandboxError::Unsupported {
        platform: "linux".to_string(),
        message: "unprivileged user namespaces are unavailable on this host; the cgroups sandbox \
                  requires them. Enable `kernel.unprivileged_userns_clone=1` (and, on Ubuntu, the \
                  AppArmor `kernel.apparmor_restrict_unprivileged_userns=0`) or run the daemon in \
                  an environment that permits user namespaces."
            .to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tddy_sandbox::{
        CgroupConfig, EnvSpec, NetworkSpec, PolicySpec, ReadReason, ReadSpec, ResourceLimits,
        SandboxPlan, SandboxSpec,
    };

    fn a_plan(reads: Vec<ReadSpec>, limits: ResourceLimits) -> SandboxPlan {
        let spec = SandboxSpec {
            project_root: PathBuf::from("/tmp/tddy-cgroups-test"),
            scratch_dir: PathBuf::from("/tmp/tddy-cgroups-test/.work"),
            egress_dir: PathBuf::from("/tmp/tddy-cgroups-test/out"),
            allow_read_paths: vec![],
            command: vec!["/bin/true".into()],
            env: Default::default(),
            profile_path: PathBuf::from("/tmp/tddy-cgroups-test/profile.sb"),
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
            network: NetworkSpec::default(),
            limits,
            stdin: None,
            cgroup: Default::default(),
        }
    }

    #[test]
    fn maps_a_writable_mount_to_a_read_write_bind_mount() {
        // Given — a writable mount
        let mut plan = a_plan(vec![], ResourceLimits::default());
        plan.mounts = vec![tddy_sandbox::MountSpec::read_write("/work/proj")];

        // When
        let mounts = plan_to_bind_mounts(&plan);

        // Then — bound read-write (no MS_RDONLY) at its own path
        let (_, target, flags) = mounts
            .iter()
            .find(|(src, _, _)| src == &PathBuf::from("/work/proj"))
            .expect("mount must be present");
        assert_eq!(target, &PathBuf::from("/work/proj"));
        assert!(flags.contains(MsFlags::MS_BIND));
        assert!(
            !flags.contains(MsFlags::MS_RDONLY),
            "writable mount must not be read-only"
        );
    }

    #[test]
    fn maps_each_declared_read_to_a_readonly_bind_mount() {
        // Given — one declared read
        let plan = a_plan(
            vec![ReadSpec::subpath("/usr/lib", ReadReason::SystemLibs)],
            ResourceLimits::default(),
        );

        // When
        let mounts = plan_to_bind_mounts(&plan);

        // Then — a single read-only bind mount source==target==/usr/lib
        assert_eq!(mounts.len(), 1);
        let (src, dst, flags) = &mounts[0];
        assert_eq!(src, &PathBuf::from("/usr/lib"));
        assert_eq!(dst, &PathBuf::from("/usr/lib"));
        assert!(flags.contains(MsFlags::MS_BIND));
        assert!(flags.contains(MsFlags::MS_RDONLY));
    }

    #[test]
    fn marks_non_exec_reads_with_the_noexec_flag() {
        // Given — a non-exec read
        let plan = a_plan(
            vec![ReadSpec::subpath("/etc/ssl/certs", ReadReason::SystemLibs)],
            ResourceLimits::default(),
        );

        // When
        let mounts = plan_to_bind_mounts(&plan);

        // Then
        assert!(mounts[0].2.contains(MsFlags::MS_NOEXEC));
    }

    #[test]
    fn maps_plan_limits_onto_cgroup_values() {
        // Given
        let limits = ResourceLimits {
            memory_max: Some(123),
            cpu_max: Some("50000 100000".to_string()),
            pids_max: Some(7),
        };

        // When
        let cgroup = cgroup_limits_from(&limits);

        // Then
        assert_eq!(cgroup.memory_max, Some(123));
        assert_eq!(cgroup.cpu_max, Some("50000 100000".to_string()));
        assert_eq!(cgroup.pids_max, Some(7));
    }

    #[test]
    fn reports_userns_available_when_the_functional_attempt_succeeds() {
        // Given — a non-root process whose functional userns attempt succeeds

        // When
        let available = userns_available_from(false, UsernsProbe::Available);

        // Then
        assert!(available);
    }

    #[test]
    fn reports_userns_unavailable_when_the_functional_attempt_is_denied() {
        // Given — a non-root process whose functional userns attempt is denied

        // When
        let available = userns_available_from(false, UsernsProbe::Denied);

        // Then
        assert!(!available);
    }

    #[test]
    fn reports_userns_available_for_root_without_attempting() {
        // Given — root, with an attempt closure that must never be invoked

        // When
        let available = unprivileged_userns_available_with(true, || {
            panic!("root must short-circuit before attempting the probe")
        });

        // Then
        assert!(available);
    }

    #[test]
    fn parses_the_v2_cgroup_path_from_proc_self_cgroup() {
        // Given
        let contents = "0::/system.slice/tddy-daemon.service\n";

        // When
        let relative = cgroup_v2_relative_path(contents);

        // Then
        assert_eq!(
            relative.as_deref(),
            Some("/system.slice/tddy-daemon.service")
        );
    }

    #[test]
    fn treats_the_root_v2_cgroup_as_the_mount_root() {
        // Given — a service living at the cgroup v2 root

        // When
        let base = delegated_cgroup_base_from(Path::new("/sys/fs/cgroup"), "/");

        // Then
        assert_eq!(base, PathBuf::from("/sys/fs/cgroup"));
    }

    #[test]
    fn reports_no_v2_path_on_a_cgroup_v1_only_host() {
        // Given — a v1-only /proc/self/cgroup with no `0::` unified line
        let contents = "3:cpu,cpuacct:/user.slice\n2:memory:/user.slice\n";

        // When
        let relative = cgroup_v2_relative_path(contents);

        // Then
        assert_eq!(relative, None);
    }

    #[test]
    fn finds_the_cgroup2_mount_root_from_mountinfo() {
        // Given — a mountinfo whose cgroup2 mount is at /sys/fs/cgroup
        let mountinfo = "23 66 0:22 / /proc rw,nosuid - proc proc rw\n\
             31 23 0:27 / /sys/fs/cgroup rw,nosuid,nodev,noexec relatime shared:9 - cgroup2 cgroup2 rw,nsdelegate\n";

        // When
        let root = cgroup2_mount_root_from(mountinfo);

        // Then
        assert_eq!(root, Some(PathBuf::from("/sys/fs/cgroup")));
    }

    #[test]
    fn prefers_the_configured_base_override_over_proc_derivation() {
        // Given — an explicit base override
        let cfg = CgroupConfig {
            base_override: Some(PathBuf::from("/custom/delegated/base")),
            ..Default::default()
        };

        // When
        let base = resolve_cgroup_base(
            &cfg,
            "0::/system.slice/tddy-daemon.service\n",
            "",
            Path::new("/sys/fs/cgroup"),
        );

        // Then
        assert_eq!(base, Some(PathBuf::from("/custom/delegated/base")));
    }

    #[test]
    fn derives_the_base_from_proc_when_no_override_is_configured() {
        // Given — no override; derive from /proc/self/cgroup joined with the default mount root
        let cfg = CgroupConfig::default();

        // When
        let base = resolve_cgroup_base(
            &cfg,
            "0::/system.slice/tddy-daemon.service\n",
            "",
            Path::new("/sys/fs/cgroup"),
        );

        // Then
        assert_eq!(
            base,
            Some(PathBuf::from(
                "/sys/fs/cgroup/system.slice/tddy-daemon.service"
            ))
        );
    }

    #[test]
    fn defaults_controllers_to_memory_cpu_pids_when_unconfigured() {
        // Given
        let cfg = CgroupConfig::default();

        // When
        let controllers = controllers_or_default(&cfg);

        // Then
        assert_eq!(controllers, vec!["memory", "cpu", "pids"]);
    }

    #[test]
    fn uses_configured_controllers_when_present() {
        // Given
        let cfg = CgroupConfig {
            controllers: vec!["memory".to_string(), "pids".to_string()],
            ..Default::default()
        };

        // When
        let controllers = controllers_or_default(&cfg);

        // Then
        assert_eq!(controllers, vec!["memory", "pids"]);
    }

    #[test]
    fn formats_the_subtree_control_enable_line() {
        // Given
        let controllers = vec!["memory".to_string(), "cpu".to_string(), "pids".to_string()];

        // When
        let line = subtree_control_line(&controllers);

        // Then
        assert_eq!(line, "+memory +cpu +pids");
    }

    #[test]
    fn names_each_session_scope_uniquely_under_the_base() {
        // Given — the same base and session name across two spawns with distinct sequence numbers
        let base = Path::new("/sys/fs/cgroup/system.slice/tddy-daemon.service");

        // When
        let first = scope_dir_in(base, "proj", 1);
        let second = scope_dir_in(base, "proj", 2);

        // Then — both live under the base and never collide
        assert!(first.starts_with(base));
        assert!(second.starts_with(base));
        assert_ne!(first, second);
    }
}
