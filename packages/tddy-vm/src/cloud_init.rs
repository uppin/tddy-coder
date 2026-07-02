//! Cloud-init based VM image building with image-chaining.
//!
//! Copies an immutable base cloud image, chains a qcow2 delta overlay onto it
//! (`qemu-img create -b`), generates a NoCloud cloud-init seed ISO, then boots QEMU to
//! actually bake the provisioning into the overlay (watching the serial console for a
//! completion token; the guest self-shuts-down when done). The output is the chained
//! pair — `<name>-base.qcow2` (immutable) + `<name>.qcow2` (provisioned delta overlay)
//! — co-located, since the overlay uses a **relative** backing-file reference and is
//! not self-contained without its base.
//!
//! All argv/document-rendering logic is exposed as pure, unit-testable builder
//! functions; [`build_cloud_init_image`] composes them into the full pipeline.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::qemu::{send_monitor_command, QemuVmArgs};
use crate::vm::VmError;

// ── Pure argv builders ───────────────────────────────────────────────────────────

/// Build `qemu-img convert -f qcow2 -O qcow2 <base_input> <base_output>` — a plain
/// qcow2-to-qcow2 convert that flattens any prior backing chain on the source into a
/// standalone, immutable base image. Distinct from `build.rs::convert_to_qcow2`
/// (raw-to-qcow2, used by the Buildroot pipeline).
pub fn base_convert_argv(base_input: &Path, base_output: &Path) -> Vec<String> {
    vec![
        "convert".to_string(),
        "-f".to_string(),
        "qcow2".to_string(),
        "-O".to_string(),
        "qcow2".to_string(),
        base_input.display().to_string(),
        base_output.display().to_string(),
    ]
}

/// Build `qemu-img create -f qcow2 -F qcow2 -b <base_basename> <overlay> <disk_size>`.
///
/// `base_basename` must be a bare relative filename (e.g. `"demo-base.qcow2"`), not an
/// absolute path — the overlay and base can then be relocated together without
/// breaking the backing-file reference. This is intentionally different in flag order
/// and semantics from `tddy_sandbox_qemu::argv::overlay_create_argv`, which builds an
/// ephemeral, absolute-path-backed overlay with no size argument.
pub fn overlay_create_argv(base_basename: &str, overlay: &Path, disk_size: &str) -> Vec<String> {
    vec![
        "create".to_string(),
        "-f".to_string(),
        "qcow2".to_string(),
        "-F".to_string(),
        "qcow2".to_string(),
        "-b".to_string(),
        base_basename.to_string(),
        overlay.display().to_string(),
        disk_size.to_string(),
    ]
}

// ── NoCloud document types ───────────────────────────────────────────────────────

/// A single cloud-init user entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudInitUser {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sudo: Option<String>,
    /// May contain the literal placeholder `"{{SSH_PUBLIC_KEY}}"`, substituted by
    /// [`render_user_data`].
    #[serde(default)]
    pub ssh_authorized_keys: Vec<String>,
}

/// A single `write_files` entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudInitWriteFile {
    pub path: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permissions: Option<String>,
}

/// The provisioning spec rendered into a NoCloud `user-data` document by
/// [`render_user_data`].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CloudInitUserData {
    #[serde(default)]
    pub hostname: Option<String>,
    #[serde(default)]
    pub users: Vec<CloudInitUser>,
    #[serde(default)]
    pub packages: Vec<String>,
    #[serde(default)]
    pub runcmd: Vec<String>,
    #[serde(default)]
    pub write_files: Vec<CloudInitWriteFile>,
    #[serde(default)]
    pub bootcmd: Vec<String>,
}

/// Internal serialization shape for a single rendered user, after SSH key
/// substitution — kept separate from [`CloudInitUser`] so the public struct's field
/// shape (used for both YAML parsing and JSON token hashing) never has to match the
/// exact rendered-document shape.
#[derive(Debug, Clone, Serialize)]
struct RenderedUser<'a> {
    name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    shell: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sudo: Option<&'a str>,
    ssh_authorized_keys: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct RenderedWriteFile<'a> {
    path: &'a str,
    content: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    permissions: Option<&'a str>,
}

#[derive(Debug, Clone, Serialize)]
struct RenderedUserDataDoc<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    hostname: Option<&'a str>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    users: Vec<RenderedUser<'a>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    packages: Vec<&'a str>,
    write_files: Vec<RenderedWriteFile<'a>>,
    bootcmd: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    runcmd: Vec<&'a str>,
}

/// A basic netplan v2 DHCP config for the primary NIC (matches both `en*` and `eth*`
/// interface naming schemes), written as a `write_files` entry so the guest gets
/// network access on first boot. Not exercised by a unit test — needed for the real
/// VM boot in [`build_cloud_init_image`] to reach the network at all.
const NETPLAN_DHCP_CONTENT: &str = "network:\n  version: 2\n  ethernets:\n    all-en:\n      match:\n        name: \"en*\"\n      dhcp4: true\n    all-eth:\n      match:\n        name: \"eth*\"\n      dhcp4: true\n";

/// Map `users`, substituting the `{{SSH_PUBLIC_KEY}}` placeholder in each user's
/// `ssh_authorized_keys` with `ssh_public_key`.
fn render_users<'a>(users: &'a [CloudInitUser], ssh_public_key: &str) -> Vec<RenderedUser<'a>> {
    users
        .iter()
        .map(|u| RenderedUser {
            name: u.name.as_str(),
            shell: u.shell.as_deref(),
            sudo: u.sudo.as_deref(),
            ssh_authorized_keys: u
                .ssh_authorized_keys
                .iter()
                .map(|k| k.replace("{{SSH_PUBLIC_KEY}}", ssh_public_key))
                .collect(),
        })
        .collect()
}

/// The `scripts-per-boot` completion script content: waits for cloud-init to finish,
/// then echoes `completion_token` (or `<completion_token>_FAILED`) to the serial
/// console and shuts the guest down.
///
/// Decides success/failure from cloud-init's aggregated error list rather than a
/// blanket "status: error" check: some Debian genericcloud images reliably log a
/// benign `set_hostname` module failure under QEMU (systemd-hostnamed isn't ready
/// that early in boot) that flips overall status to "error" even though every
/// directive we actually care about (users, packages, write_files, runcmd) applied
/// cleanly. Filtering that one out avoids misclassifying a successful bake as failed.
/// `python3` is guaranteed present — cloud-init itself depends on it.
fn completion_script_content(completion_token: &str) -> String {
    format!(
        "#!/bin/bash\n\
         cloud-init status --wait >/dev/null 2>&1\n\
         ERRORS=\"$(cloud-init status --format json 2>/dev/null | python3 -c '\n\
         import json, sys\n\
         d = json.load(sys.stdin)\n\
         errs = [e for e in d.get(\"errors\", []) if \"set_hostname\" not in e]\n\
         print(\"\\n\".join(errs))\n\
         ' 2>/dev/null)\"\n\
         if [ -n \"$ERRORS\" ]; then\n\
         \x20 echo \"{completion_token}_FAILED\"\n\
         else\n\
         \x20 echo \"{completion_token}\"\n\
         fi\n\
         shutdown -h now\n"
    )
}

/// Render the NoCloud `user-data` document: a `#cloud-config` header followed by the
/// caller's users/packages/runcmd/write_files/bootcmd, plus:
/// - SSH public key substitution for the `{{SSH_PUBLIC_KEY}}` placeholder.
/// - A `cloud-init clean --logs --seed` `bootcmd` entry (forces cloud-init to re-run
///   against the fresh seed on a copied base image that already ran cloud-init once).
/// - A basic DHCP netplan config for the primary NIC.
/// - A `scripts-per-boot` completion script (see [`completion_script_content`]).
pub fn render_user_data(
    user_data: &CloudInitUserData,
    ssh_public_key: &str,
    completion_token: &str,
) -> String {
    let users = render_users(&user_data.users, ssh_public_key);

    let mut write_files: Vec<RenderedWriteFile> = user_data
        .write_files
        .iter()
        .map(|w| RenderedWriteFile {
            path: w.path.as_str(),
            content: w.content.as_str(),
            permissions: w.permissions.as_deref(),
        })
        .collect();
    write_files.push(RenderedWriteFile {
        path: "/etc/netplan/50-tddy-cloud-init-dhcp.yaml",
        content: NETPLAN_DHCP_CONTENT,
        permissions: Some("0644"),
    });
    let completion_script = completion_script_content(completion_token);
    write_files.push(RenderedWriteFile {
        path: "/var/lib/cloud/scripts/per-boot/99-tddy-cloud-init-complete.sh",
        content: &completion_script,
        permissions: Some("0755"),
    });

    let mut bootcmd = vec!["cloud-init clean --logs --seed".to_string()];
    bootcmd.extend(user_data.bootcmd.iter().cloned());

    let doc = RenderedUserDataDoc {
        hostname: user_data.hostname.as_deref(),
        users,
        packages: user_data.packages.iter().map(|p| p.as_str()).collect(),
        write_files,
        bootcmd,
        runcmd: user_data.runcmd.iter().map(|r| r.as_str()).collect(),
    };

    let body = serde_yml::to_string(&doc)
        .unwrap_or_else(|e| format!("# failed to render cloud-init user-data: {e}\n"));
    format!("#cloud-config\n{body}")
}

/// Render the NoCloud `meta-data` document. Hand-formatted (not a generic YAML
/// serializer) since the exact bytes are part of the NoCloud contract.
pub fn render_meta_data(instance_id: &str, local_hostname: &str) -> String {
    format!("instance-id: {instance_id}\nlocal-hostname: {local_hostname}\n")
}

// ── Completion token ──────────────────────────────────────────────────────────────

/// Derive a deterministic completion token from `name` and `token_data`:
/// `CLOUDINIT_COMPLETE_<name>_<first-12-hex-chars-of-sha256(token_data)>`.
pub fn completion_token(name: &str, token_data: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token_data.as_bytes());
    let digest = hasher.finalize();
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    format!("CLOUDINIT_COMPLETE_{name}_{}", &hex[..12])
}

// ── Seed ISO ──────────────────────────────────────────────────────────────────────

/// Build the shared mkisofs-family argv for a `cidata`-labeled ISO9660 volume (Joliet
/// + Rock Ridge extensions) from `nocloud_dir`.
pub fn seed_iso_argv(iso_output: &Path, nocloud_dir: &Path) -> Vec<String> {
    vec![
        "-output".to_string(),
        iso_output.display().to_string(),
        "-volid".to_string(),
        "cidata".to_string(),
        "-joliet".to_string(),
        "-rock".to_string(),
        nocloud_dir.display().to_string(),
    ]
}

/// Which ISO-building tool to invoke for [`iso_tool_command`].
#[derive(Debug, Clone, Copy)]
pub enum IsoTool {
    Xorriso,
    Mkisofs,
    Genisoimage,
}

/// Resolve `tool` to a `(program, args)` pair that builds the NoCloud seed ISO.
///
/// `Xorriso` runs in mkisofs-emulation mode (`-as mkisofs`) ahead of the shared
/// [`seed_iso_argv`]; `Mkisofs`/`Genisoimage` run their native binaries directly with
/// the same argv.
pub fn iso_tool_command(
    tool: IsoTool,
    iso_output: &Path,
    nocloud_dir: &Path,
) -> (String, Vec<String>) {
    let shared = seed_iso_argv(iso_output, nocloud_dir);
    match tool {
        IsoTool::Xorriso => {
            let mut args = vec!["-as".to_string(), "mkisofs".to_string()];
            args.extend(shared);
            ("xorriso".to_string(), args)
        }
        IsoTool::Mkisofs => ("mkisofs".to_string(), shared),
        IsoTool::Genisoimage => ("genisoimage".to_string(), shared),
    }
}

// ── Boot argv ─────────────────────────────────────────────────────────────────────

/// Configuration needed to boot the overlay with its seed ISO attached, for
/// [`cloud_init_boot_argv`].
#[derive(Debug, Clone)]
pub struct CloudInitBootConfig {
    pub overlay_path: String,
    pub seed_iso_path: String,
    pub memory: String,
    pub cpus: u32,
    pub ssh_host_port: u16,
}

/// Build the full `qemu-system-x86_64` argv to boot `config.overlay_path` with the
/// seed ISO attached, serial routed to stdio (so the host can watch it live for the
/// completion token), and `-no-reboot` (so the guest's `shutdown -h now` ends the
/// process instead of rebooting it).
///
/// Also pins the datasource to NoCloud via an SMBIOS type-1 serial number
/// (`ds=nocloud;`) — the standard mechanism `DataSourceNoCloud` checks for before
/// doing any datasource detection at all. Without it, cloud-init's network-stage
/// service still crawls every other supported datasource (EC2, Azure, GCE, ...) even
/// though NoCloud was already found locally, each with its own network timeout; that
/// crawl is the dominant source of highly variable (and sometimes very slow) boot
/// times observed baking real images.
pub fn cloud_init_boot_argv(config: &CloudInitBootConfig) -> Vec<String> {
    let monitor = format!(
        "unix:{},server,nowait",
        QemuVmArgs::monitor_socket_path(config.ssh_host_port)
    );
    vec![
        "-drive".to_string(),
        format!("file={},if=virtio,format=qcow2", config.overlay_path),
        "-cdrom".to_string(),
        config.seed_iso_path.clone(),
        "-m".to_string(),
        config.memory.clone(),
        "-smp".to_string(),
        config.cpus.to_string(),
        "-nographic".to_string(),
        "-serial".to_string(),
        "stdio".to_string(),
        "-netdev".to_string(),
        format!("user,id=net0,hostfwd=tcp::{}-:22", config.ssh_host_port),
        "-device".to_string(),
        "virtio-net-pci,netdev=net0".to_string(),
        "-no-reboot".to_string(),
        "-monitor".to_string(),
        monitor,
        "-smbios".to_string(),
        "type=1,serial=ds=nocloud;".to_string(),
    ]
}

// ── Serial classification ─────────────────────────────────────────────────────────

/// Outcome of classifying one line of serial console output while waiting for
/// cloud-init to finish baking provisioning into the overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloudInitOutcome {
    Pending,
    Succeeded,
    Failed,
}

/// Classify a serial console `line` against `completion_token`.
///
/// Checks for the `<completion_token>_FAILED` variant **first**: it contains the bare
/// token as a substring, so checking the bare token first would misclassify failures
/// as successes.
pub fn classify_serial_line(line: &str, completion_token: &str) -> CloudInitOutcome {
    let failed_variant = format!("{completion_token}_FAILED");
    if line.contains(&failed_variant) {
        CloudInitOutcome::Failed
    } else if line.contains(completion_token) {
        CloudInitOutcome::Succeeded
    } else {
        CloudInitOutcome::Pending
    }
}

// ── Orchestrator ──────────────────────────────────────────────────────────────────

/// Options for [`build_cloud_init_image`].
#[derive(Debug, Clone)]
pub struct CloudInitBuildOptions {
    pub name: String,
    pub base_image_src: PathBuf,
    pub output_dir: PathBuf,
    pub user_data: CloudInitUserData,
    pub disk_size: String,
    pub memory: String,
    pub cpus: u32,
    pub ssh_host_port: u16,
    pub timeout: Duration,
    pub iso_tool: IsoTool,
    /// If `Some`, read and use this key. If `None`, generate a fresh ed25519 keypair.
    pub ssh_public_key: Option<PathBuf>,
}

/// Input hashed by [`completion_token`] to derive a build-specific token —
/// deterministic for identical `(name, user_data)`, distinct otherwise.
#[derive(Serialize)]
struct TokenDataInput<'a> {
    name: &'a str,
    user_data: &'a CloudInitUserData,
}

/// Run `qemu-img` with `args`, surfacing stderr on a non-zero exit — mirrors the
/// error-handling shape of `build.rs::convert_to_qcow2`.
async fn run_qemu_img(args: &[String]) -> Result<(), String> {
    let out = tokio::process::Command::new("qemu-img")
        .args(args)
        .output()
        .await
        .map_err(|e| format!("qemu-img launch failed: {e}"))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(format!(
            "qemu-img {} failed: {stderr}",
            args.first().map(|s| s.as_str()).unwrap_or("")
        ));
    }
    Ok(())
}

/// Run the resolved ISO-building `program` with `args`, surfacing stderr on failure.
async fn run_iso_tool(program: &str, args: &[String]) -> Result<(), String> {
    let out = tokio::process::Command::new(program)
        .args(args)
        .output()
        .await
        .map_err(|e| format!("{program} launch failed: {e}"))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(format!("{program} failed: {stderr}"));
    }
    Ok(())
}

/// Generate a fresh ed25519 keypair at `<output_dir>/id_<name>` via `ssh-keygen` and
/// return the contents of the resulting `.pub` file.
async fn generate_ssh_keypair(output_dir: &Path, name: &str) -> Result<String, VmError> {
    let key_path = output_dir.join(format!("id_{name}"));
    let status = tokio::process::Command::new("ssh-keygen")
        .arg("-t")
        .arg("ed25519")
        .arg("-N")
        .arg("")
        .arg("-f")
        .arg(&key_path)
        .status()
        .await
        .map_err(|e| VmError::BuildFailed(format!("failed to spawn ssh-keygen: {e}")))?;
    if !status.success() {
        return Err(VmError::BuildFailed(format!(
            "ssh-keygen exited with {status}"
        )));
    }
    let pub_path = key_path.with_extension("pub");
    tokio::fs::read_to_string(&pub_path).await.map_err(|e| {
        VmError::BuildFailed(format!(
            "failed to read generated public key {}: {e}",
            pub_path.display()
        ))
    })
}

/// Handle one line read from the qemu serial console during [`boot_and_bake`]'s watch
/// loop: forward it to `progress`, append it to `boot_log`, and classify it.
///
/// Returns `Some(outcome)` once the loop should stop (success or failure observed),
/// or `None` to keep waiting for more lines. On `Failed`, also kills `child` so the
/// caller doesn't need a second kill site for this path.
async fn handle_boot_line(
    line: &str,
    token: &str,
    boot_log_path: &Path,
    boot_log: &mut tokio::fs::File,
    child: &mut tokio::process::Child,
    progress: &(dyn Fn(&str) + Sync),
) -> Option<Result<(), VmError>> {
    use tokio::io::AsyncWriteExt;

    progress(line);
    let _ = boot_log.write_all(line.as_bytes()).await;
    let _ = boot_log.write_all(b"\n").await;

    match classify_serial_line(line, token) {
        CloudInitOutcome::Succeeded => Some(Ok(())),
        CloudInitOutcome::Failed => {
            let _ = child.kill().await;
            Some(Err(VmError::BuildFailed(format!(
                "cloud-init reported failure on serial console: {line} (full log: {})",
                boot_log_path.display()
            ))))
        }
        CloudInitOutcome::Pending => None,
    }
}

/// Handle the watch loop's timeout branch: attempt a graceful shutdown via the QEMU
/// monitor socket, give it a short grace period, then force-kill the process
/// regardless. Always returns an `Err` — the caller's loop always breaks on this path.
async fn handle_boot_timeout(
    monitor_socket: &str,
    boot_log_path: &Path,
    boot_log: &mut tokio::fs::File,
    child: &mut tokio::process::Child,
    progress: &(dyn Fn(&str) + Sync),
) -> VmError {
    use tokio::io::AsyncWriteExt;

    let msg =
        "Timed out waiting for the cloud-init completion token; attempting graceful shutdown…";
    progress(msg);
    let _ = boot_log.write_all(format!("-- {msg}\n").as_bytes()).await;
    let _ = send_monitor_command(monitor_socket, "system_powerdown").await;
    tokio::time::sleep(Duration::from_secs(5)).await;
    let _ = child.kill().await;
    VmError::BootFailed(format!(
        "timed out waiting for cloud-init completion token (full log: {})",
        boot_log_path.display()
    ))
}

/// Boot the provisioned overlay with the seed ISO attached and watch the serial
/// console for the completion token, per the orchestration flow documented on
/// [`build_cloud_init_image`].
///
/// Every serial console line is both forwarded to `progress` (ephemeral) and
/// appended to `boot_log_path` (durable), so a failed or timed-out bake can be
/// investigated after the fact — the full boot log outlives the process that ran it.
async fn boot_and_bake(
    opts: &CloudInitBuildOptions,
    overlay_path: &Path,
    iso_path: &Path,
    token: &str,
    boot_log_path: &Path,
    progress: &(dyn Fn(&str) + Sync),
) -> Result<(), VmError> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let boot_config = CloudInitBootConfig {
        overlay_path: overlay_path.display().to_string(),
        seed_iso_path: iso_path.display().to_string(),
        memory: opts.memory.clone(),
        cpus: opts.cpus,
        ssh_host_port: opts.ssh_host_port,
    };
    let args = cloud_init_boot_argv(&boot_config);
    let monitor_socket = QemuVmArgs::monitor_socket_path(opts.ssh_host_port);

    let mut boot_log = tokio::fs::File::create(boot_log_path).await.map_err(|e| {
        VmError::BuildFailed(format!(
            "failed to create boot log {}: {e}",
            boot_log_path.display()
        ))
    })?;
    progress(&format!(
        "Watching serial console (full log: {})…",
        boot_log_path.display()
    ));

    let mut child = tokio::process::Command::new("qemu-system-x86_64")
        .args(&args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| VmError::BootFailed(format!("spawn qemu-system-x86_64: {e}")))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| VmError::BootFailed("qemu-system-x86_64 stdout unavailable".to_string()))?;
    let mut lines = BufReader::new(stdout).lines();

    let deadline = tokio::time::Instant::now() + opts.timeout;

    let outcome: Result<(), VmError> = loop {
        tokio::select! {
            line = lines.next_line() => {
                match line {
                    Ok(Some(line)) => {
                        if let Some(outcome) =
                            handle_boot_line(&line, token, boot_log_path, &mut boot_log, &mut child, progress).await
                        {
                            break outcome;
                        }
                    }
                    Ok(None) => {
                        let _ = boot_log
                            .write_all(b"-- qemu-system-x86_64 stdout closed (process exited) --\n")
                            .await;
                        break Err(VmError::BootFailed(format!(
                            "qemu-system-x86_64 exited before the cloud-init completion token was observed (full log: {})",
                            boot_log_path.display()
                        )));
                    }
                    Err(e) => {
                        break Err(VmError::BootFailed(format!(
                            "failed reading qemu serial console output: {e} (full log: {})",
                            boot_log_path.display()
                        )));
                    }
                }
            }
            _ = tokio::time::sleep_until(deadline) => {
                break Err(handle_boot_timeout(&monitor_socket, boot_log_path, &mut boot_log, &mut child, progress).await);
            }
        }
    };

    if outcome.is_ok() {
        let _ = child.wait().await;
    }
    outcome
}

/// Copy `opts.base_image_src` into a scratch file and convert it into the immutable
/// base `<output_dir>/<name>-base.qcow2` (steps 1-2 of [`build_cloud_init_image`]).
/// The scratch copy is removed afterward; the original source is never touched.
async fn copy_and_convert_base(
    opts: &CloudInitBuildOptions,
    progress: &(dyn Fn(&str) + Sync),
) -> Result<PathBuf, VmError> {
    progress(&format!(
        "Copying base image from {}…",
        opts.base_image_src.display()
    ));
    let copied_src = opts
        .output_dir
        .join(format!("{}-copied-src.qcow2", opts.name));
    tokio::fs::copy(&opts.base_image_src, &copied_src)
        .await
        .map_err(|e| {
            VmError::BuildFailed(format!(
                "failed to copy base image {} to {}: {e}",
                opts.base_image_src.display(),
                copied_src.display()
            ))
        })?;

    progress("Converting base image into an immutable qcow2…");
    let base_path = opts.output_dir.join(format!("{}-base.qcow2", opts.name));
    run_qemu_img(&base_convert_argv(&copied_src, &base_path))
        .await
        .map_err(|e| {
            progress(&e);
            VmError::BuildFailed(e)
        })?;
    let _ = tokio::fs::remove_file(&copied_src).await;

    Ok(base_path)
}

/// Resolve the SSH public key for the seed: read `opts.ssh_public_key` if given,
/// otherwise generate a fresh ed25519 keypair in `opts.output_dir` (step 3).
async fn resolve_ssh_public_key(opts: &CloudInitBuildOptions) -> Result<String, VmError> {
    match &opts.ssh_public_key {
        Some(path) => tokio::fs::read_to_string(path).await.map_err(|e| {
            VmError::BuildFailed(format!(
                "failed to read ssh public key {}: {e}",
                path.display()
            ))
        }),
        None => generate_ssh_keypair(&opts.output_dir, &opts.name).await,
    }
}

/// Derive the deterministic completion token from `(opts.name, opts.user_data)`
/// (step 4).
fn derive_completion_token(opts: &CloudInitBuildOptions) -> Result<String, VmError> {
    let token_data = serde_json::to_string(&TokenDataInput {
        name: &opts.name,
        user_data: &opts.user_data,
    })
    .map_err(|e| VmError::BuildFailed(format!("failed to serialize provisioning input: {e}")))?;
    Ok(completion_token(&opts.name, &token_data))
}

/// Render and write the NoCloud `user-data`/`meta-data` seed into `<output_dir>/seed/
/// nocloud/` (step 5). Returns the seed directory path.
async fn write_nocloud_seed(
    opts: &CloudInitBuildOptions,
    ssh_public_key: &str,
    token: &str,
) -> Result<PathBuf, VmError> {
    let nocloud_dir = opts.output_dir.join("seed").join("nocloud");
    tokio::fs::create_dir_all(&nocloud_dir).await.map_err(|e| {
        VmError::BuildFailed(format!(
            "failed to create seed dir {}: {e}",
            nocloud_dir.display()
        ))
    })?;

    let user_data_rendered = render_user_data(&opts.user_data, ssh_public_key.trim(), token);
    tokio::fs::write(nocloud_dir.join("user-data"), user_data_rendered)
        .await
        .map_err(|e| VmError::BuildFailed(format!("failed to write user-data: {e}")))?;

    let meta_data_rendered = render_meta_data(
        &format!("cloud-init-{}", opts.name),
        opts.user_data.hostname.as_deref().unwrap_or(&opts.name),
    );
    tokio::fs::write(nocloud_dir.join("meta-data"), meta_data_rendered)
        .await
        .map_err(|e| VmError::BuildFailed(format!("failed to write meta-data: {e}")))?;

    Ok(nocloud_dir)
}

/// Pack `nocloud_dir` into a `cidata` seed ISO at `<output_dir>/<name>-seed.iso` via
/// `opts.iso_tool` (step 6). Returns the ISO path.
async fn build_seed_iso(
    opts: &CloudInitBuildOptions,
    nocloud_dir: &Path,
    progress: &(dyn Fn(&str) + Sync),
) -> Result<PathBuf, VmError> {
    let iso_path = opts.output_dir.join(format!("{}-seed.iso", opts.name));
    let (program, args) = iso_tool_command(opts.iso_tool, &iso_path, nocloud_dir);
    run_iso_tool(&program, &args).await.map_err(|e| {
        progress(&e);
        VmError::BuildFailed(e)
    })?;
    Ok(iso_path)
}

/// Create the delta overlay `<output_dir>/<name>.qcow2`, chained onto the immutable
/// base via a relative backing-file reference (step 7). Returns the overlay path.
async fn create_overlay(
    opts: &CloudInitBuildOptions,
    progress: &(dyn Fn(&str) + Sync),
) -> Result<PathBuf, VmError> {
    let overlay_path = opts.output_dir.join(format!("{}.qcow2", opts.name));
    let base_basename = format!("{}-base.qcow2", opts.name);
    run_qemu_img(&overlay_create_argv(
        &base_basename,
        &overlay_path,
        &opts.disk_size,
    ))
    .await
    .map_err(|e| {
        progress(&e);
        VmError::BuildFailed(e)
    })?;
    Ok(overlay_path)
}

/// Build a cloud-init-provisioned VM image with image-chaining.
///
/// 1. Copies `opts.base_image_src` into `<output_dir>/<name>-copied-src.qcow2`.
/// 2. Converts it into the immutable base `<output_dir>/<name>-base.qcow2`.
/// 3. Resolves the SSH public key (reads `opts.ssh_public_key`, or generates a fresh
///    ed25519 keypair).
/// 4. Derives a deterministic completion token from `(opts.name, opts.user_data)`.
/// 5. Renders and writes the NoCloud `user-data`/`meta-data` seed.
/// 6. Packs the seed into a `cidata` ISO via `opts.iso_tool`.
/// 7. Creates the delta overlay `<output_dir>/<name>.qcow2`, chained onto the base via
///    a relative backing-file reference.
/// 8. Boots the overlay with the seed ISO attached and watches the serial console for
///    the completion token, baking the provisioning into the overlay. The full serial
///    console transcript is durably logged to `<output_dir>/<name>-boot.log` (in
///    addition to being streamed through `progress`), so a failed or timed-out bake
///    can be investigated after the process has exited. Returns the overlay path on
///    success.
pub async fn build_cloud_init_image(
    opts: &CloudInitBuildOptions,
    progress: &(dyn Fn(&str) + Sync),
) -> Result<PathBuf, VmError> {
    tokio::fs::create_dir_all(&opts.output_dir)
        .await
        .map_err(|e| {
            VmError::BuildFailed(format!(
                "failed to create output dir {}: {e}",
                opts.output_dir.display()
            ))
        })?;

    copy_and_convert_base(opts, progress).await?;

    progress("Resolving SSH public key…");
    let ssh_public_key = resolve_ssh_public_key(opts).await?;
    let token = derive_completion_token(opts)?;

    progress("Rendering cloud-init NoCloud seed…");
    let nocloud_dir = write_nocloud_seed(opts, &ssh_public_key, &token).await?;

    progress("Building cloud-init seed ISO…");
    let iso_path = build_seed_iso(opts, &nocloud_dir, progress).await?;

    progress("Creating chained delta overlay…");
    let overlay_path = create_overlay(opts, progress).await?;

    progress("Booting QEMU to bake cloud-init provisioning into the overlay…");
    let boot_log_path = opts.output_dir.join(format!("{}-boot.log", opts.name));
    boot_and_bake(
        opts,
        &overlay_path,
        &iso_path,
        &token,
        &boot_log_path,
        progress,
    )
    .await?;

    progress("Cloud-init image build complete");
    Ok(overlay_path)
}
