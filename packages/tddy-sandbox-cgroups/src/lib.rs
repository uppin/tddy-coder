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
use std::process::Command;

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
    plan.reads
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
        .collect()
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

    let scope = cgroup_scope_path(&plan.spec);
    prepare_cgroup_scope(&scope).map_err(|e| cgroup_unsupported_error(&scope, &e))?;

    let uid = nix::unistd::geteuid().as_raw();
    let gid = nix::unistd::getegid().as_raw();
    let uid_map = format!("0 {uid} 1\n");
    let gid_map = format!("0 {gid} 1\n");
    let bind_mounts = plan_to_bind_mounts(&plan);

    let mut cmd = Command::new(&plan.spec.command[0]);
    cmd.args(&plan.spec.command[1..]);
    cmd.env_clear();
    cmd.envs(&plan.spec.env);

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

    if let Err(e) = std::fs::write(scope.join("cgroup.procs"), child.id().to_string()) {
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

/// cgroup v2 unified hierarchy root.
const CGROUP_ROOT: &str = "/sys/fs/cgroup";

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
    // cgroup v2 subtree fails fast (no silent degrade to an unlimited process).
    let scope = cgroup_scope_path(&spec);
    prepare_cgroup_scope(&scope).map_err(|e| cgroup_unsupported_error(&scope, &e))?;

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
    if let Err(e) = std::fs::write(scope.join("cgroup.procs"), child.id().to_string()) {
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
/// unprivileged process is blocked when Ubuntu's AppArmor restriction or the userns sysctls deny it.
pub fn unprivileged_userns_available() -> bool {
    if nix::unistd::geteuid().is_root() {
        return true;
    }
    let read = |p: &str| {
        std::fs::read_to_string(p)
            .ok()
            .map(|s| s.trim().to_string())
    };
    if read("/proc/sys/kernel/apparmor_restrict_unprivileged_userns").as_deref() == Some("1") {
        return false;
    }
    if read("/proc/sys/kernel/unprivileged_userns_clone").as_deref() == Some("0") {
        return false;
    }
    !matches!(
        read("/proc/sys/user/max_user_namespaces").and_then(|s| s.parse::<u64>().ok()),
        Some(0)
    )
}

/// cgroup v2 scope directory for a session, derived from the project root's final component.
fn cgroup_scope_path(spec: &SandboxSpec) -> PathBuf {
    let name = spec
        .project_root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("session");
    let pid = std::process::id();
    PathBuf::from(CGROUP_ROOT).join(format!("tddy-{name}-{pid}.scope"))
}

/// Create the cgroup scope and enable the controllers we limit. Returns Err if the cgroup root is
/// not writable (no delegation) — the caller degrades to no-limits rather than failing the spawn.
fn prepare_cgroup_scope(scope: &Path) -> std::io::Result<()> {
    // Enable controllers in the root's subtree_control (ignored if already enabled / not permitted).
    let _ = std::fs::write(
        Path::new(CGROUP_ROOT).join("cgroup.subtree_control"),
        "+memory +cpu +pids",
    );
    std::fs::create_dir_all(scope)?;
    Ok(())
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
        EnvSpec, NetworkSpec, PolicySpec, ReadReason, ReadSpec, ResourceLimits, SandboxPlan,
        SandboxSpec,
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
        };
        SandboxPlan {
            spec,
            reads,
            copies: vec![],
            symlinks: vec![],
            env: EnvSpec::default(),
            policy: PolicySpec::default(),
            network: NetworkSpec::default(),
            limits,
        }
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
}
