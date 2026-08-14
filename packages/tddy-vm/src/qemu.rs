//! QEMU concrete implementation of `Vm`.
//!
//! `QemuVm` boots `qemu-system-<arch>` (nix-provided) with:
//! - the machine type and accelerator the config names
//! - virtio drive from the qcow2 image, plus an optional UEFI pflash pair
//! - user-mode networking (slirp) with `hostfwd` specs for SSH + app ports
//! - optional cloud-init seed ISO and virtio-9p host directory shares
//! - optional VNC (`-vnc :<n>`) for the deferred ScreenShare mode
//! - QEMU monitor unix socket for graceful shutdown
//!
//! `QemuVmArgs` assembles the argv vector from a `VmConfig` so the arg-builder logic
//! is unit-testable independently of process spawning.

use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use tokio::io::AsyncWriteExt;
use tokio::process::{Child, ChildStdin, ChildStdout};

use crate::cloud_init::NinePShare;
use crate::serial_shell::SerialConsole;
use crate::vm::{
    ForwardHandle, PortForward, RunningVm, UefiFirmware, VerifyResult, Vm, VmAccel, VmArch,
    VmConfig, VmError,
};

/// Size of the writable UEFI variables store QEMU expects on the second pflash unit.
const UEFI_VARS_SIZE_BYTES: u64 = 64 * 1024 * 1024;

/// The environment variable naming the `edk2-<arch>-code.fd` firmware explicitly,
/// overriding the derivation from the emulator's own installation.
pub const UEFI_CODE_ENV: &str = "TDDY_VM_UEFI_CODE";

/// The `qemu-system-*` emulator that runs `arch`.
pub fn qemu_binary(arch: VmArch) -> &'static str {
    match arch {
        VmArch::Aarch64 => "qemu-system-aarch64",
        VmArch::X86_64 => "qemu-system-x86_64",
    }
}

/// The `edk2-<arch>-code.fd` basename QEMU installs its UEFI firmware under.
fn uefi_code_filename(arch: VmArch) -> &'static str {
    match arch {
        VmArch::Aarch64 => "edk2-aarch64-code.fd",
        VmArch::X86_64 => "edk2-x86_64-code.fd",
    }
}

/// Locate `program` by walking `$PATH`, as the shell would.
fn find_on_path(program: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|path| {
        std::env::split_paths(&path)
            .map(|dir| dir.join(program))
            .find(|candidate| candidate.is_file())
    })
}

/// Resolve the read-only UEFI firmware image for `arch`.
///
/// Checks [`UEFI_CODE_ENV`] first; otherwise derives `share/qemu/edk2-<arch>-code.fd` from
/// the location of the `qemu-system-*` binary on `$PATH`, which is where a QEMU
/// installation (including the nix one) keeps it.
///
/// An unresolvable firmware image is an error. There is deliberately no fallback to a BIOS
/// boot: the aarch64 `virt` machine has no legacy BIOS, so a silent fallback would produce
/// a guest that never boots and a failure far from its cause.
pub fn resolve_uefi_code_path(arch: VmArch) -> Result<PathBuf, VmError> {
    let filename = uefi_code_filename(arch);

    if let Some(configured) = std::env::var_os(UEFI_CODE_ENV) {
        let path = PathBuf::from(configured);
        if !path.is_file() {
            return Err(VmError::BootFailed(format!(
                "{UEFI_CODE_ENV} points at {}, which is not a readable file",
                path.display()
            )));
        }
        return Ok(path);
    }

    let binary = qemu_binary(arch);
    let emulator = find_on_path(binary).ok_or_else(|| {
        VmError::BootFailed(format!(
            "cannot locate UEFI firmware {filename}: {binary} is not on $PATH \
             (set {UEFI_CODE_ENV} to the firmware image to override)"
        ))
    })?;
    let prefix = emulator.parent().and_then(Path::parent).ok_or_else(|| {
        VmError::BootFailed(format!(
            "cannot derive a QEMU installation prefix from {}",
            emulator.display()
        ))
    })?;
    let firmware = prefix.join("share").join("qemu").join(filename);
    if !firmware.is_file() {
        return Err(VmError::BootFailed(format!(
            "UEFI firmware {} not found in the {} installation \
             (set {UEFI_CODE_ENV} to the firmware image to override)",
            firmware.display(),
            binary
        )));
    }
    Ok(firmware)
}

/// Create the writable UEFI variables store at `path` if it does not exist yet: a raw file
/// of exactly [`UEFI_VARS_SIZE_BYTES`] zero bytes, the size QEMU's pflash device requires.
///
/// Idempotent — an existing store is left untouched, so a VM keeps its boot entries across
/// restarts.
pub fn ensure_uefi_vars_file(path: &Path) -> Result<(), VmError> {
    if path.exists() {
        return Ok(());
    }
    let file = std::fs::File::create(path).map_err(|e| {
        VmError::BootFailed(format!(
            "failed to create UEFI vars file {}: {e}",
            path.display()
        ))
    })?;
    file.set_len(UEFI_VARS_SIZE_BYTES).map_err(|e| {
        VmError::BootFailed(format!(
            "failed to size UEFI vars file {} to {UEFI_VARS_SIZE_BYTES} bytes: {e}",
            path.display()
        ))
    })
}

/// The UEFI firmware pair a guest of `arch` boots through, with its writable variables store
/// placed at `<dir>/<name>-vars.fd` (created if absent, along with `dir` itself).
///
/// `None` on x86_64, whose `q35` machine boots the image's own bootloader through SeaBIOS.
/// The aarch64 `virt` machine has no BIOS at all, so firmware there is mandatory and an
/// unresolvable firmware image is an error rather than a guest that silently never boots.
pub fn uefi_firmware_for(
    arch: VmArch,
    dir: &Path,
    name: &str,
) -> Result<Option<UefiFirmware>, VmError> {
    if arch == VmArch::X86_64 {
        return Ok(None);
    }
    std::fs::create_dir_all(dir).map_err(|e| {
        VmError::BootFailed(format!(
            "failed to create the UEFI variables directory {}: {e}",
            dir.display()
        ))
    })?;
    let vars_path = dir.join(format!("{name}-vars.fd"));
    ensure_uefi_vars_file(&vars_path)?;
    Ok(Some(UefiFirmware {
        code_path: resolve_uefi_code_path(arch)?.display().to_string(),
        vars_path: vars_path.display().to_string(),
    }))
}

/// Poll `host:port` via TCP every 100 ms until either a connection succeeds or `timeout`
/// elapses.
///
/// Returns `Ok(())` on the first successful connection.
/// Returns `Err(VmError::BootFailed(...))` when the timeout expires without a
/// successful connection.
pub async fn wait_for_ssh_port(host: &str, port: u16, timeout: Duration) -> Result<(), VmError> {
    let addr = format!("{host}:{port}");
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        match tokio::net::TcpStream::connect(&addr).await {
            Ok(_) => return Ok(()),
            Err(_) => {
                if tokio::time::Instant::now() >= deadline {
                    return Err(VmError::BootFailed(format!(
                        "timed out waiting for SSH port {port} on {host}"
                    )));
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }
}

/// Connect to the QEMU monitor Unix socket at `socket_path`, write `"{command}\n"`, then
/// close the connection.
///
/// Returns `Err(VmError::ShutdownFailed(...))` if the socket cannot be reached or the
/// write fails.
pub async fn send_monitor_command(socket_path: &str, command: &str) -> Result<(), VmError> {
    let mut stream = tokio::net::UnixStream::connect(socket_path)
        .await
        .map_err(|e| VmError::ShutdownFailed(format!("connect to monitor socket: {e}")))?;
    let msg = format!("{command}\n");
    stream
        .write_all(msg.as_bytes())
        .await
        .map_err(|e| VmError::ShutdownFailed(format!("write to monitor socket: {e}")))?;
    stream
        .flush()
        .await
        .map_err(|e| VmError::ShutdownFailed(format!("flush monitor socket: {e}")))?;
    Ok(())
}

/// Assembles `qemu-system-<arch>` argument vectors from a `VmConfig`.
///
/// This struct is the pure, unit-testable core of the QEMU runner — no process spawning.
pub struct QemuVmArgs;

impl QemuVmArgs {
    /// Build the full argv for [`qemu_binary`] from the given config, with the serial
    /// console written to a per-VM log file.
    ///
    /// The SSH forward (`tcp::<ssh_host_port>-:22`) is always included first so the
    /// caller can reach the guest regardless of `extra_hostfwd`.
    ///
    /// # Example output
    /// ```text
    /// qemu-system-aarch64
    ///   -machine virt,accel=hvf
    ///   -cpu host
    ///   -m 2048M
    ///   -smp 4
    ///   -drive file=<qcow2>,if=virtio,format=qcow2
    ///   -drive if=pflash,format=raw,unit=0,readonly=on,file=<edk2-aarch64-code.fd>
    ///   -drive if=pflash,format=raw,unit=1,file=<vars.fd>
    ///   -cdrom <seed.iso>
    ///   -nographic
    ///   -fsdev local,id=fsdev0,path=<host dir>,security_model=none,readonly=on
    ///   -device virtio-9p-pci,fsdev=fsdev0,mount_tag=<tag>
    ///   -netdev user,id=net0,hostfwd=tcp::2222-:22,hostfwd=tcp::8080-:80
    ///   -device virtio-net-pci,netdev=net0
    ///   -monitor unix:/tmp/tddy-vm-monitor-<port>.sock,server,nowait
    ///   -serial file:/tmp/tddy-vm-serial-<port>.log
    /// ```
    pub fn build(config: &VmConfig) -> Vec<String> {
        Self::build_with_serial(
            config,
            &format!("file:/tmp/tddy-vm-serial-{}.log", config.ssh_host_port),
        )
    }

    /// Build the argv with an explicit QEMU `-serial` backend — `file:<path>` for a
    /// detached boot, `stdio` when the caller drives the console over the child's pipes.
    pub fn build_with_serial(config: &VmConfig, serial: &str) -> Vec<String> {
        let monitor = format!(
            "unix:{},server,nowait",
            Self::monitor_socket_path(config.ssh_host_port)
        );

        let mut args = vec![
            "-machine".to_string(),
            Self::machine_arg(config.arch, config.accel),
            "-cpu".to_string(),
            Self::cpu_arg(config.arch, config.accel).to_string(),
            "-m".to_string(),
            config.memory.clone(),
            "-smp".to_string(),
            config.cpus.to_string(),
            "-drive".to_string(),
            format!("file={},if=virtio,format=qcow2", config.qcow2_path),
        ];
        args.extend(Self::pflash_args(config.firmware.as_ref()));
        if let Some(seed_iso) = &config.seed_iso {
            args.extend(["-cdrom".to_string(), seed_iso.clone()]);
        }
        args.push("-nographic".to_string());
        args.extend(Self::nine_p_args(&config.nine_p_shares));
        args.extend([
            "-netdev".to_string(),
            Self::netdev_arg(config),
            "-device".to_string(),
            "virtio-net-pci,netdev=net0".to_string(),
            "-monitor".to_string(),
            monitor,
            "-serial".to_string(),
            serial.to_string(),
        ]);
        args
    }

    /// The `-machine` value: the architecture's machine type carrying the accelerator.
    ///
    /// aarch64 has no default machine at all, so the type is never left implicit.
    pub fn machine_arg(arch: VmArch, accel: VmAccel) -> String {
        let machine = match arch {
            VmArch::Aarch64 => "virt",
            VmArch::X86_64 => "q35",
        };
        let accel = match accel {
            VmAccel::Hvf => "hvf",
            VmAccel::Kvm => "kvm",
            VmAccel::Tcg => "tcg",
        };
        format!("{machine},accel={accel}")
    }

    /// The `-cpu` value.
    ///
    /// A hardware-accelerated guest passes the host CPU straight through. TCG cannot do
    /// that — `-cpu host` is rejected outright without an accelerator — so an emulated
    /// model is named instead.
    pub fn cpu_arg(arch: VmArch, accel: VmAccel) -> &'static str {
        match (arch, accel) {
            (_, VmAccel::Hvf) | (_, VmAccel::Kvm) => "host",
            (VmArch::Aarch64, VmAccel::Tcg) => "cortex-a72",
            (VmArch::X86_64, VmAccel::Tcg) => "max",
        }
    }

    /// The UEFI pflash pair: read-only firmware code on unit 0, writable variables on
    /// unit 1. Empty when the guest boots through its own BIOS instead.
    pub fn pflash_args(firmware: Option<&UefiFirmware>) -> Vec<String> {
        let Some(firmware) = firmware else {
            return Vec::new();
        };
        vec![
            "-drive".to_string(),
            format!(
                "if=pflash,format=raw,unit=0,readonly=on,file={}",
                firmware.code_path
            ),
            "-drive".to_string(),
            format!("if=pflash,format=raw,unit=1,file={}", firmware.vars_path),
        ]
    }

    /// The `-fsdev`/`-device` pair exporting each host directory over virtio-9p.
    pub fn nine_p_args(shares: &[NinePShare]) -> Vec<String> {
        let mut args = Vec::new();
        for (index, share) in shares.iter().enumerate() {
            let id = format!("fsdev{index}");
            let readonly = if share.writable { "" } else { ",readonly=on" };
            args.extend([
                "-fsdev".to_string(),
                format!(
                    "local,id={id},path={},security_model=none{readonly}",
                    share.host_path
                ),
                "-device".to_string(),
                format!("virtio-9p-pci,fsdev={id},mount_tag={}", share.mount_tag),
            ]);
        }
        args
    }

    /// Derive the monitor Unix socket path from the SSH host port so concurrent VM
    /// instances (each with a unique SSH port) don't collide.
    pub fn monitor_socket_path(ssh_host_port: u16) -> String {
        format!("/tmp/tddy-vm-monitor-{ssh_host_port}.sock")
    }

    /// Where [`QemuVm::boot_with_serial_console`] keeps the guest's QEMU stderr: beside the
    /// guest's own disk image, one per VM.
    ///
    /// Deliberately *not* under `/tmp`. A predictable name in a world-writable directory is
    /// a symlink target any local user can pre-create, which would let them pick the file
    /// this process truncates — and, because the log is read back into error messages, the
    /// file it discloses. The image's own directory is as private as the VM it belongs to.
    pub fn console_stderr_log_path(qcow2_path: &str) -> PathBuf {
        Path::new(qcow2_path).with_extension("console-stderr.log")
    }

    /// Format a single `hostfwd` spec from a `PortForward`.
    ///
    /// Returns `"tcp::<host_port>-:<guest_port>"` (the slirp `-netdev user,hostfwd=` value format).
    pub fn hostfwd_spec(port_forward: &PortForward) -> String {
        format!(
            "tcp::{}-:{}",
            port_forward.host_port, port_forward.guest_port
        )
    }

    /// Build the combined `-netdev` argument including all hostfwd specs.
    ///
    /// SSH forward (`tcp::<ssh_host_port>-:22`) is prepended; `extra_hostfwd` follows.
    pub fn netdev_arg(config: &VmConfig) -> String {
        let mut arg = format!("user,id=net0,hostfwd=tcp::{}-:22", config.ssh_host_port);
        for port_forward in &config.extra_hostfwd {
            arg.push_str(&format!(",hostfwd={}", Self::hostfwd_spec(port_forward)));
        }
        arg
    }
}

/// QEMU concrete implementation of [`Vm`].
///
/// Boots `qemu-system-<arch>` (resolved via `$PATH`, provided by the nix dev shell),
/// deploys via SSH over the slirp hostfwd port, and verifies the app via SSH command.
#[derive(Debug, Default)]
pub struct QemuVm;

impl QemuVm {
    pub fn new() -> Self {
        Self
    }

    /// Boot the VM with its serial console routed to a pipe and hand that pipe back as a
    /// [`SerialConsole`].
    ///
    /// Differs from [`Vm::boot`] in two ways, both because the caller is driving the guest
    /// over the UART rather than over the network:
    /// - serial goes to `stdio` on a piped child instead of to a log file, so the console
    ///   can be both read and written;
    /// - the SSH port is *not* waited for. A guest reached over the console may have no
    ///   sshd, no networking, and no cloud-init run yet; the caller decides what ready
    ///   means. Use [`wait_for_ssh_port`] explicitly when SSH is what is wanted.
    pub async fn boot_with_serial_console(
        &self,
        config: &VmConfig,
    ) -> Result<BootedWithConsole, VmError> {
        let args = QemuVmArgs::build_with_serial(config, "stdio");

        // QEMU's own diagnostics (bad argv, a stale monitor socket, an unreadable image)
        // arrive on stderr and are the only explanation for a console that closes without
        // ever producing output, so they are kept in a per-VM log rather than discarded.
        let stderr_log_path = QemuVmArgs::console_stderr_log_path(&config.qcow2_path);
        let stderr_log = create_stderr_log(&stderr_log_path)?;

        let (vm, mut child) = spawn_emulator(
            config,
            &args,
            Stdio::piped(),
            Stdio::piped(),
            Stdio::from(stderr_log),
        )?;

        let (stdin, stdout) = match take_serial_pipes(&mut child, qemu_binary(config.arch)) {
            Ok(pipes) => pipes,
            Err(e) => {
                // A console this process cannot drive is not a booted guest. Leaving the
                // emulator running would hold this VM's forwarded ports, its monitor
                // socket and a write handle on its disk image, and the next start on the
                // same port would then fail for an unrelated-looking reason.
                let _ = child.start_kill();
                return Err(e);
            }
        };

        // Drop the Child without awaiting: the process runs detached, exactly as in
        // `boot`, until `shutdown` powers it down through the monitor socket. The console
        // pipes taken above outlive it.
        drop(child);

        Ok(BootedWithConsole {
            vm,
            console: SerialConsole::new(stdin, stdout).with_stderr_log(stderr_log_path),
        })
    }
}

/// Spawn `qemu-system-<arch>` for `config` with `args` and the given stdio, and describe
/// the guest now running as a [`RunningVm`].
///
/// The `Child` comes back alongside the handle so the caller can take the console pipes off
/// it. Dropping the `Child` does **not** stop the emulator — see [`BootedWithConsole`].
fn spawn_emulator(
    config: &VmConfig,
    args: &[String],
    stdin: Stdio,
    stdout: Stdio,
    stderr: Stdio,
) -> Result<(RunningVm, Child), VmError> {
    let binary = qemu_binary(config.arch);
    let child = tokio::process::Command::new(binary)
        .args(args)
        .stdin(stdin)
        .stdout(stdout)
        .stderr(stderr)
        .spawn()
        .map_err(|e| VmError::BootFailed(format!("spawn {binary}: {e}")))?;

    // `id()` is only `None` once the child has been reaped, which cannot have happened
    // here — nothing has awaited it — so there is no emulator left behind on this path.
    let pid = child
        .id()
        .ok_or_else(|| VmError::BootFailed("qemu exited immediately after spawn".into()))?;

    Ok((
        RunningVm {
            ssh_host_port: config.ssh_host_port,
            monitor_socket: QemuVmArgs::monitor_socket_path(config.ssh_host_port),
            pid,
            login: config.login.clone(),
        },
        child,
    ))
}

/// Take the serial console pipes off a child of `binary` spawned with piped stdio.
fn take_serial_pipes(
    child: &mut Child,
    binary: &str,
) -> Result<(ChildStdin, ChildStdout), VmError> {
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| VmError::BootFailed(format!("{binary} serial stdin unavailable")))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| VmError::BootFailed(format!("{binary} serial stdout unavailable")))?;
    Ok((stdin, stdout))
}

/// Create the emulator's stderr log at `path`, readable only by its owner and refusing to
/// follow a symlink.
///
/// `O_NOFOLLOW` rather than a plain create: a symlink standing where the log belongs would
/// otherwise redirect this truncating write — and the read-back in
/// [`crate::serial_shell::SerialConsole`] — at a file chosen by whoever planted it.
fn create_stderr_log(path: &Path) -> Result<std::fs::File, VmError> {
    std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .custom_flags(libc::O_NOFOLLOW)
        .mode(0o600)
        .open(path)
        .map_err(|e| VmError::BootFailed(format!("create {}: {e}", path.display())))
}

/// A guest booted with its serial console routed to a pipe this process owns, rather than
/// to a log file.
///
/// The emulator is detached: dropping this handle does **not** stop the guest. It keeps
/// running — holding its forwarded ports, its monitor socket, and a write handle on its
/// disk image — until the holder passes [`Self::vm`] to [`Vm::shutdown`].
pub struct BootedWithConsole {
    pub vm: RunningVm,
    /// The driveable console. The guest is *not* waited for — the caller decides what
    /// "ready" means by watching this.
    pub console: SerialConsole,
}

/// Build the SSH argument list for connecting to a guest.
///
/// Options suppress host-key prompts (`StrictHostKeyChecking=no`,
/// `UserKnownHostsFile=/dev/null`), prevent interactive password prompts
/// (`BatchMode=yes`), cap the connect wait (`ConnectTimeout=10`), and silence
/// the "Warning: Permanently added" banner (`LogLevel=ERROR`).
///
/// When the VM has a per-VM private key, it is offered with `-i` and `IdentitiesOnly=yes`
/// so the ambient agent's keys cannot be tried instead and exhaust `MaxAuthTries`.
pub fn ssh_opts(vm: &RunningVm) -> Vec<String> {
    client_opts(vm, "-p")
}

/// Build the `scp` argument list for copying into a guest.
///
/// Identical to [`ssh_opts`] except for the port flag: `scp` spells it `-P`, and reads a
/// lowercase `-p` as "preserve modification times" — which would leave the port number
/// sitting in the argv as another source path. That one-letter difference is the whole
/// reason this is a separate function rather than a reuse of [`ssh_opts`].
pub fn scp_opts(vm: &RunningVm) -> Vec<String> {
    client_opts(vm, "-P")
}

/// The options `ssh` and `scp` share, with the port introduced by the caller's flag.
fn client_opts(vm: &RunningVm, port_flag: &str) -> Vec<String> {
    let mut opts = vec![
        port_flag.to_string(),
        vm.ssh_host_port.to_string(),
        "-o".to_string(),
        "StrictHostKeyChecking=no".to_string(),
        "-o".to_string(),
        "UserKnownHostsFile=/dev/null".to_string(),
        "-o".to_string(),
        "BatchMode=yes".to_string(),
        "-o".to_string(),
        "ConnectTimeout=10".to_string(),
        "-o".to_string(),
        "LogLevel=ERROR".to_string(),
    ];
    if let Some(key) = &vm.login.private_key_path {
        opts.extend([
            "-i".to_string(),
            key.clone(),
            "-o".to_string(),
            "IdentitiesOnly=yes".to_string(),
        ]);
    }
    opts
}

/// The `<user>@127.0.0.1` destination SSH connects to, as the VM's login policy names it.
pub fn ssh_destination(vm: &RunningVm) -> String {
    format!("{}@127.0.0.1", vm.login.username)
}

/// Build the full `scp` argv copying every path in `local_paths` to `remote_dest` in the
/// guest.
///
/// All sources share one invocation: scp pays a full SSH connection and key exchange per
/// call, and a supervised host needs several binaries staged at once.
pub fn scp_to_guest_argv(
    vm: &RunningVm,
    local_paths: &[PathBuf],
    remote_dest: &str,
) -> Vec<String> {
    let mut argv = scp_opts(vm);
    argv.extend(local_paths.iter().map(|p| p.display().to_string()));
    argv.push(format!("{}:{remote_dest}", ssh_destination(vm)));
    argv
}

/// Copy `local_paths` into `remote_dest` in the guest over the forwarded SSH port.
///
/// Used instead of a 9p share because the guest under test runs Debian's stock `-cloud`
/// kernel, which ships no 9p modules at all — and swapping it for the generic flavour
/// would diverge the kernel under test from the one a real host runs.
pub async fn scp_to_guest(
    vm: &RunningVm,
    local_paths: &[PathBuf],
    remote_dest: &str,
) -> Result<(), VmError> {
    let argv = scp_to_guest_argv(vm, local_paths, remote_dest);
    let output = tokio::process::Command::new("scp")
        .args(&argv)
        .output()
        .await
        .map_err(|e| VmError::DeployFailed(format!("scp spawn error: {e}")))?;

    if !output.status.success() {
        return Err(VmError::DeployFailed(format!(
            "scp to {remote_dest} failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(())
}

#[async_trait::async_trait]
impl Vm for QemuVm {
    /// Boot the VM from the given config.
    ///
    /// Spawns `qemu-system-x86_64` with args from [`QemuVmArgs::build`] and waits
    /// up to 5 minutes for the guest SSH port to become reachable before returning.
    /// The QEMU process is detached (not killed when the `Child` is dropped) so it
    /// outlives this call and runs until [`shutdown`][Self::shutdown] is called.
    async fn boot(&self, config: &VmConfig) -> Result<RunningVm, VmError> {
        let args = QemuVmArgs::build(config);
        let (vm, child) =
            spawn_emulator(config, &args, Stdio::null(), Stdio::null(), Stdio::null())?;

        // Drop the Child without awaiting: the process runs as a detached daemon
        // until shutdown() sends system_powerdown to the monitor socket.
        drop(child);

        wait_for_ssh_port("127.0.0.1", config.ssh_host_port, Duration::from_secs(300)).await?;

        Ok(vm)
    }

    /// Run each deploy step inside the guest via SSH.
    ///
    /// Steps are executed sequentially as the VM's login-policy user. The first step that
    /// exits non-zero returns `Err(DeployFailed)` with the step text and exit status.
    async fn deploy(&self, vm: &RunningVm, steps: &[String]) -> Result<(), VmError> {
        for step in steps {
            let status = tokio::process::Command::new("ssh")
                .args(ssh_opts(vm))
                .arg(ssh_destination(vm))
                .arg(step)
                .status()
                .await
                .map_err(|e| VmError::DeployFailed(format!("ssh spawn error: {e}")))?;

            if !status.success() {
                return Err(VmError::DeployFailed(format!(
                    "step `{step}` failed with {status}"
                )));
            }
        }
        Ok(())
    }

    /// Run `command` inside the guest via SSH and return its output and exit code.
    ///
    /// Stdout and stderr are captured **separately**, so a caller can assert on what the
    /// command answered without sshd's own chatter in the same string. A non-zero exit code
    /// sets `success = false` but does **not** return `Err` — the caller decides whether to
    /// treat verification failure as fatal.
    async fn verify(&self, vm: &RunningVm, command: &str) -> Result<VerifyResult, VmError> {
        let output = tokio::process::Command::new("ssh")
            .args(ssh_opts(vm))
            .arg(ssh_destination(vm))
            .arg(command)
            .output()
            .await
            .map_err(|e| VmError::VerifyFailed(format!("ssh spawn error: {e}")))?;

        Ok(VerifyResult {
            success: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            exit_code: output.status.code().unwrap_or(-1),
        })
    }

    async fn forward(
        &self,
        _vm: &RunningVm,
        port_forward: &PortForward,
    ) -> Result<ForwardHandle, VmError> {
        let addr = format!("127.0.0.1:{}", port_forward.host_port);
        tokio::time::timeout(
            Duration::from_secs(1),
            tokio::net::TcpStream::connect(&addr),
        )
        .await
        .map_err(|_| {
            VmError::ForwardFailed(format!(
                "timed out connecting to host port {}",
                port_forward.host_port
            ))
        })?
        .map_err(|e| {
            VmError::ForwardFailed(format!(
                "host port {} not reachable: {e}",
                port_forward.host_port
            ))
        })?;

        Ok(ForwardHandle {
            host_port: port_forward.host_port,
            guest_port: port_forward.guest_port,
            share_url: format!("http://localhost:{}", port_forward.host_port),
        })
    }

    /// Gracefully shut down the VM by sending `system_powerdown` to the QEMU monitor socket.
    async fn shutdown(&self, vm: RunningVm) -> Result<(), VmError> {
        send_monitor_command(&vm.monitor_socket, "system_powerdown").await
    }
}
