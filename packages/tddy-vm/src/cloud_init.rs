//! Cloud-init based VM image building with image-chaining.
//!
//! Chains a qcow2 delta overlay onto its parent image (`qemu-img create -b`), generates a
//! NoCloud cloud-init seed ISO, then boots QEMU to actually bake the provisioning into the
//! overlay (watching the serial console for a completion token; the guest self-shuts-down
//! when done). The output is a single image — `<name>.qcow2` — holding **only its own
//! delta** and keeping a live reference to the parent it was baked from. Nothing is copied
//! and nothing is flattened: the parent is read, never rewritten.
//!
//! The backing reference is **relative** ([`relative_backing_path`]), so a whole library of
//! layers relocates as a unit. A qcow2 resolves such a reference against the directory
//! holding the *referencing* image, which is why [`build_cloud_init_image`] creates the
//! overlay at its final `overlay_output` path and never moves it afterwards; `output_dir`
//! is scratch space for everything else the bake produces.
//!
//! All argv/document-rendering logic is exposed as pure, unit-testable builder
//! functions; [`build_cloud_init_image`] composes them into the full pipeline.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::qemu::{qemu_binary, send_monitor_command, QemuVmArgs};
use crate::vm::{UefiFirmware, VmAccel, VmArch, VmError};

// ── Pure argv builders ───────────────────────────────────────────────────────────

/// Name `parent` as seen from `from_dir`, the directory holding the image that will carry
/// the reference — a bare filename for a sibling (`"tddy-nix-base.qcow2"`), a walk out and
/// back down across a tier boundary (`"../01-base/debian-12.qcow2"`).
///
/// Pure path arithmetic: no filesystem access, no normalisation of `.`/`..` already present
/// in either argument. A trailing separator on `from_dir` makes no difference, so a caller
/// that built it by `join` gets the same answer as one that wrote it out.
///
/// **Both paths must share an origin** — either both absolute, or both relative to the same
/// directory. A mix of the two is rejected rather than answered: the walk out of an absolute
/// `from_dir` counts components that a relative `parent` never had, so the result would name
/// a location neither argument refers to, and qcow2 would record it without complaint.
///
/// A relative reference is what keeps a layered library relocatable: qcow2 resolves it
/// against the directory of the image holding it, so moving the whole tree — as moving a
/// checkout moves its repo-relative `tmp/.tddy` — leaves every link in the chain intact.
/// The same property is why an overlay must be created where it will live: created
/// elsewhere and moved, its reference would point at nothing.
pub fn relative_backing_path(from_dir: &Path, parent: &Path) -> Result<String, VmError> {
    if from_dir.is_absolute() != parent.is_absolute() {
        return Err(VmError::BuildFailed(format!(
            "cannot name {} relative to {}: one is absolute and the other is relative",
            parent.display(),
            from_dir.display()
        )));
    }

    let from: Vec<_> = from_dir.components().collect();
    let to: Vec<_> = parent.components().collect();
    let shared = from
        .iter()
        .zip(to.iter())
        .take_while(|(a, b)| a == b)
        .count();

    let mut parts = vec!["..".to_string(); from.len() - shared];
    parts.extend(
        to[shared..]
            .iter()
            .map(|c| c.as_os_str().to_string_lossy().into_owned()),
    );
    Ok(parts.join("/"))
}

/// Build `qemu-img create -f qcow2 -F qcow2 -b <backing> <overlay> <disk_size>`.
///
/// `backing` must be a path relative to the overlay's own directory, as
/// [`relative_backing_path`] produces — never an absolute one, so the layer and its
/// ancestors can be relocated together without breaking the chain. This is intentionally
/// different in flag order and semantics from `tddy_sandbox_qemu::argv::overlay_create_argv`,
/// which builds an ephemeral, absolute-path-backed overlay with no size argument.
pub fn overlay_create_argv(backing: &str, overlay: &Path, disk_size: &str) -> Vec<String> {
    qcow2_overlay_create_argv(backing, overlay, disk_size)
}

/// The one `qemu-img create` argv shape shared by [`overlay_create_argv`] and
/// [`crate::library::vm_overlay_create_argv`].
///
/// The two public functions differ only in what they pass as `backing_file` — a path
/// relative to the overlay's own directory for a library layer, an absolute path for a
/// per-VM overlay — and their doc comments explain why. Routing both through this helper is
/// what keeps the flag order itself a single definition rather than two that must be kept
/// in lockstep by hand.
pub(crate) fn qcow2_overlay_create_argv(
    backing_file: &str,
    overlay: &Path,
    disk_size: &str,
) -> Vec<String> {
    vec![
        "create".to_string(),
        "-f".to_string(),
        "qcow2".to_string(),
        "-F".to_string(),
        "qcow2".to_string(),
        "-b".to_string(),
        backing_file.to_string(),
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
    /// Console password for this user. Distro cloud images ship no password at all, so
    /// without one the guest can only be reached over SSH — set this (together with
    /// `lock_passwd: Some(false)`) when something has to log in on the **serial console**,
    /// before sshd or networking are up.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plain_text_passwd: Option<String>,
    /// cloud-init locks every created account's password by default; `Some(false)` unlocks
    /// it so [`Self::plain_text_passwd`] can actually be used to log in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lock_passwd: Option<bool>,
}

/// A single `write_files` entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudInitWriteFile {
    pub path: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permissions: Option<String>,
    /// `user:group` to chown the written file to (cloud-init's `owner` key). `None` leaves
    /// cloud-init's default of `root:root`.
    ///
    /// Naming an account from this document's `users` list requires [`Self::defer`] as
    /// well: cloud-init runs `write_files` *before* `users_groups`, and chowning to a
    /// not-yet-created user or group fails the whole module.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    /// Write this file in cloud-init's `write_files_deferred` module (final stage, after
    /// users exist and packages are installed) instead of the early `write_files` one.
    ///
    /// Still ordered before `runcmd`, which runs later in the same final stage under
    /// `scripts_user`, so a deferred config file is in place before any provisioning
    /// command reads it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub defer: Option<bool>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    plain_text_passwd: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    lock_passwd: Option<bool>,
}

/// One entry of a rendered `users:` list: cloud-init accepts the bare string `default`
/// alongside the maps describing individual accounts, and `untagged` is what renders each
/// variant as itself rather than as a tagged wrapper.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
enum RenderedUserEntry<'a> {
    DistroDefault(&'static str),
    Defined(RenderedUser<'a>),
}

/// cloud-init's keyword for "the account this distro's images create by default" — `debian`
/// on a Debian cloud image, `ubuntu` on an Ubuntu one.
const DISTRO_DEFAULT_USER: &str = "default";

#[derive(Debug, Clone, Serialize)]
struct RenderedWriteFile<'a> {
    path: &'a str,
    content: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    permissions: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    owner: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    defer: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
struct RenderedUserDataDoc<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    hostname: Option<&'a str>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    users: Vec<RenderedUserEntry<'a>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    packages: Vec<&'a str>,
    write_files: Vec<RenderedWriteFile<'a>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    bootcmd: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    runcmd: Vec<String>,
}

/// A basic netplan v2 DHCP config for the primary NIC (matches both `en*` and `eth*`
/// interface naming schemes), written as a `write_files` entry so the guest gets
/// network access on first boot. Not exercised by a unit test — needed for the real
/// VM boot in [`build_cloud_init_image`] to reach the network at all.
const NETPLAN_DHCP_CONTENT: &str = "network:\n  version: 2\n  ethernets:\n    all-en:\n      match:\n        name: \"en*\"\n      dhcp4: true\n    all-eth:\n      match:\n        name: \"eth*\"\n      dhcp4: true\n";

/// Map `users`, substituting the `{{SSH_PUBLIC_KEY}}` placeholder in each user's
/// `ssh_authorized_keys` with `ssh_public_key`, behind the distro's own default account.
///
/// **`default` leads the list because a `users:` key replaces the distro default rather than
/// adding to it**, and `cc_ssh_authkey_fingerprints` — which runs in `cloud_final_modules`
/// after `scripts-user` — still resolves that account by name. On a guest where it was never
/// created the module raises `KeyError: "getpwnam(): name not found: 'debian'"` and fails
/// `cloud-final.service` with exit 1 no matter how well provisioning itself went. Naming
/// `default` is cloud-init's own answer to that, and it costs one ordinary unprivileged
/// account in the image.
///
/// A spec that defines no accounts of its own renders no `users` key at all, which says the
/// same thing in fewer words: cloud-init's built-in default already *is* the distro account.
fn render_users<'a>(
    users: &'a [CloudInitUser],
    ssh_public_key: &str,
) -> Vec<RenderedUserEntry<'a>> {
    if users.is_empty() {
        return vec![];
    }
    let mut entries = vec![RenderedUserEntry::DistroDefault(DISTRO_DEFAULT_USER)];
    entries.extend(users.iter().map(|u| {
        RenderedUserEntry::Defined(RenderedUser {
            name: u.name.as_str(),
            shell: u.shell.as_deref(),
            sudo: u.sudo.as_deref(),
            ssh_authorized_keys: u
                .ssh_authorized_keys
                .iter()
                .map(|k| k.replace("{{SSH_PUBLIC_KEY}}", ssh_public_key))
                .collect(),
            plain_text_passwd: u.plain_text_passwd.as_deref(),
            lock_passwd: u.lock_passwd,
        })
    }));
    entries
}

/// The shell a `runcmd` step reboots the guest with when the rest of its provisioning has to
/// happen on the next boot — the kernel swap in
/// [`crate::tddy_host::ninep_capable_kernel_command`] is the workspace's one such step.
///
/// **The reset belongs here and nowhere else.** cloud-init will not re-run `runcmd` for an
/// instance it has already processed, so a reboot without one comes back up to a guest that
/// silently skips every remaining step. Resetting from `bootcmd` instead — which is where
/// this used to live — resets on *every* boot, including the first: `cloud-init clean`
/// deletes `/var/lib/cloud/instance/`, which is where the config stage writes the `runcmd`
/// script the final stage is about to run, so the boot that ran it provisioned nothing at
/// all and the bake sealed an empty image.
///
/// `--seed` drops `/var/lib/cloud/seed`; the NoCloud seed ISO stays attached as a virtio
/// disk (see [`QemuVmArgs::seed_drive_args`]), so
/// the next boot reads the same seed again. A reset that fails aborts the provisioning
/// script rather than rebooting into a guest that can never finish — the bake would
/// otherwise sit there until it timed out, which is indistinguishable from a slow one.
pub fn reset_cloud_init_and_reboot() -> String {
    "cloud-init clean --logs --seed || exit 1; sync; systemctl reboot".to_string()
}

// ── Bake completion ──────────────────────────────────────────────────────────────

/// Opens a guest log dumped onto the serial console, followed by the path of the log it
/// frames: `TDDY_GUEST_LOG_BEGIN /var/log/cloud-init.log`.
///
/// Together with [`GUEST_LOG_END_MARKER`] this is what makes the durable boot log a
/// *diagnosable* artifact rather than a timeline to guess from: everything between the two
/// markers is the guest's own `cloud-init.log`, byte for byte, and `sed -n '/BEGIN
/// <path>/,/END <path>/p' <name>-boot.log` cuts it back out.
pub const GUEST_LOG_BEGIN_MARKER: &str = "TDDY_GUEST_LOG_BEGIN";

/// Closes a guest log dumped onto the serial console — see [`GUEST_LOG_BEGIN_MARKER`].
pub const GUEST_LOG_END_MARKER: &str = "TDDY_GUEST_LOG_END";

/// The final `runcmd` step of a bake: dump the guest's logs, emit the completion token,
/// halt. Being *last* is the whole point — see [`completion_runcmd_preamble`].
const COMPLETION_RUNCMD_STEP: &str = "__tddy_complete_bake";

/// The shell the bake's completion is made of, with `@TOKEN@`, `@GUEST_LOG_BEGIN@`,
/// `@GUEST_LOG_END@` and `@COMPLETE@` substituted by [`completion_runcmd_preamble`].
///
/// Runs as part of `runcmd` — cloud-init joins every entry into one `/bin/sh` script — so
/// nothing here may assume `bash`.
const COMPLETION_RUNCMD_PREAMBLE: &str = r#"# Bake completion, rendered by tddy_vm::cloud_init.
__tddy_dump_guest_log() {
  __tddy_log="$1"
  __tddy_snapshot="/tmp/tddy-guest-log-snapshot"
  # Snapshotted first: this dump's own output is appended to cloud-init-output.log while it
  # runs, and a file that grows as fast as it is read is never read to the end.
  cp "$__tddy_log" "$__tddy_snapshot" 2>/dev/null || : >"$__tddy_snapshot"
  echo "@GUEST_LOG_BEGIN@ $__tddy_log"
  # The token is rewritten wherever the guest's own logs happen to carry it: the host
  # classifies the console line by line, so a log line quoting the token would end the
  # watch here — reporting success on a bake that may have just failed.
  sed "s/@TOKEN@/CLOUDINIT_TOKEN_ELIDED/g" "$__tddy_snapshot" 2>/dev/null
  echo "@GUEST_LOG_END@ $__tddy_log"
  rm -f "$__tddy_snapshot"
}
__tddy_signal_and_halt() {
  # The signal has to survive whatever state the failing step left the shell in.
  set +e
  __tddy_dump_guest_log /var/log/cloud-init.log
  __tddy_dump_guest_log /var/log/cloud-init-output.log
  echo "$1"
  shutdown -h now
}
__tddy_on_runcmd_exit() {
  __tddy_status=$?
  if [ "$__tddy_status" -ne 0 ]; then
    __tddy_signal_and_halt "@TOKEN@_FAILED"
  fi
}
@COMPLETE@() {
  set +e
  __tddy_errors="$(cloud-init status --format json 2>/dev/null | python3 -c '
import json, sys
d = json.load(sys.stdin)
errs = [e for e in d.get("errors", []) if "set_hostname" not in e]
print("\n".join(errs))
' 2>/dev/null)"
  if [ -n "$__tddy_errors" ]; then
    __tddy_signal_and_halt "@TOKEN@_FAILED"
  else
    __tddy_signal_and_halt "@TOKEN@"
  fi
}
trap __tddy_on_runcmd_exit EXIT"#;

/// The first `runcmd` step of a bake: define the completion shell and arm the EXIT trap
/// that carries a failed provision back to the host.
///
/// **The completion signal lives in `runcmd` because nothing else in cloud-init runs after
/// `runcmd` does.** It used to be a `scripts-per-boot` script, and `cloud_final_modules`
/// runs `scripts-per-boot` *before* `scripts-user` — so the guest halted, and the host
/// sealed the overlay and reported success, before a single provisioning command had run.
///
/// Three rules follow from where the signal now sits, and they are what the shell above
/// implements:
///
/// - **Success is announced by the last step.** [`COMPLETION_RUNCMD_STEP`] is appended
///   after the caller's own entries, so the token cannot precede the work it reports on.
/// - **Failure is announced by the trap**, armed here before any caller step exists to
///   fail. A step failing under `set -e` ends the script, and a host left waiting on a
///   token that will never come can only time out — which is indistinguishable from a slow
///   bake. The trap fires *only* on a non-zero status: a step that deliberately exits 0
///   early (the kernel-swap step reboots the guest and does exactly that) must be left to
///   cloud-init's re-run on the next boot, not mistaken for a finished bake.
/// - **The guest's own logs come first**, framed by [`GUEST_LOG_BEGIN_MARKER`] /
///   [`GUEST_LOG_END_MARKER`], because the host stops reading the console at the token.
///
/// Success is additionally withheld when cloud-init recorded an error in an earlier stage —
/// a failed `packages:` install would otherwise seal an image missing what it was asked to
/// install. `set_hostname` is filtered out of that check: Debian genericcloud images
/// reliably log a benign failure of it under QEMU (systemd-hostnamed isn't up that early),
/// which would otherwise fail every bake. `python3` is guaranteed present — cloud-init
/// itself depends on it.
fn completion_runcmd_preamble(completion_token: &str) -> String {
    COMPLETION_RUNCMD_PREAMBLE
        .replace("@GUEST_LOG_BEGIN@", GUEST_LOG_BEGIN_MARKER)
        .replace("@GUEST_LOG_END@", GUEST_LOG_END_MARKER)
        .replace("@COMPLETE@", COMPLETION_RUNCMD_STEP)
        .replace("@TOKEN@", completion_token)
}

/// The `runcmd` list of a rendered document: the caller's entries, wrapped in the bake's
/// completion steps when one is being baked ([`completion_runcmd_preamble`]) and left
/// exactly as given when one is not.
fn render_runcmd(runcmd: &[String], completion_token: Option<&str>) -> Vec<String> {
    let Some(token) = completion_token else {
        return runcmd.to_vec();
    };
    let mut steps = Vec::with_capacity(runcmd.len() + 2);
    steps.push(completion_runcmd_preamble(token));
    steps.extend(runcmd.iter().cloned());
    steps.push(COMPLETION_RUNCMD_STEP.to_string());
    steps
}

/// Render the NoCloud `user-data` document for a **bake**: a `#cloud-config` header
/// followed by the caller's users/packages/runcmd/write_files/bootcmd, plus:
/// - SSH public key substitution for the `{{SSH_PUBLIC_KEY}}` placeholder.
/// - The distro's own default account ahead of the caller's `users` — see [`render_users`].
/// - A basic DHCP netplan config for the primary NIC.
/// - The completion steps that wrap the caller's `runcmd` — see
///   [`completion_runcmd_preamble`] for why they live there and nowhere else.
///
/// **Nothing is injected into `bootcmd`.** What re-provisions a base image that already ran
/// cloud-init once is the seed's own `instance-id` — a bake names its instance after the
/// layer it is building, so cloud-init sees an instance it has never processed and applies
/// every per-instance module. A step that has to reboot mid-bake resets the instance state
/// itself, on its way out ([`reset_cloud_init_and_reboot`]).
///
/// **The completion steps halt the guest** once provisioning finishes — that is the bake
/// contract: dump the guest's cloud-init logs, signal the token, then power down so the
/// host can seal the overlay. A guest that is meant to keep running after provisioning
/// must therefore be seeded with [`render_user_data_without_completion`] instead.
pub fn render_user_data(
    user_data: &CloudInitUserData,
    ssh_public_key: &str,
    completion_token: &str,
) -> String {
    render_user_data_doc(user_data, ssh_public_key, Some(completion_token))
}

/// Render the NoCloud `user-data` document for a **long-lived guest**: identical to
/// [`render_user_data`] except that no completion step is injected — `runcmd` is exactly
/// what the caller gave — so the guest stays up after cloud-init finishes instead of
/// halting itself.
///
/// Used for guests the caller intends to keep working with — over SSH or the serial
/// console — rather than bake into an image.
pub fn render_user_data_without_completion(
    user_data: &CloudInitUserData,
    ssh_public_key: &str,
) -> String {
    render_user_data_doc(user_data, ssh_public_key, None)
}

/// Shared body of [`render_user_data`] and [`render_user_data_without_completion`].
/// `completion_token` of `None` omits the halt-on-completion steps entirely, leaving
/// `runcmd` exactly as the caller wrote it.
fn render_user_data_doc(
    user_data: &CloudInitUserData,
    ssh_public_key: &str,
    completion_token: Option<&str>,
) -> String {
    let users = render_users(&user_data.users, ssh_public_key);

    let mut write_files: Vec<RenderedWriteFile> = user_data
        .write_files
        .iter()
        .map(|w| RenderedWriteFile {
            path: w.path.as_str(),
            content: w.content.as_str(),
            permissions: w.permissions.as_deref(),
            owner: w.owner.as_deref(),
            defer: w.defer,
        })
        .collect();
    write_files.push(RenderedWriteFile {
        path: "/etc/netplan/50-tddy-cloud-init-dhcp.yaml",
        content: NETPLAN_DHCP_CONTENT,
        permissions: Some("0644"),
        owner: None,
        defer: None,
    });

    let doc = RenderedUserDataDoc {
        hostname: user_data.hostname.as_deref(),
        users,
        packages: user_data.packages.iter().map(|p| p.as_str()).collect(),
        write_files,
        bootcmd: user_data.bootcmd.clone(),
        runcmd: render_runcmd(&user_data.runcmd, completion_token),
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

/// A host directory exported into the guest over virtio-9p.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NinePShare {
    /// The directory on the host to export.
    pub host_path: String,
    /// The tag the guest mounts it by (`mount -t 9p <mount_tag> …`).
    pub mount_tag: String,
    /// Whether the guest may write through the share.
    pub writable: bool,
}

/// Configuration needed to boot the overlay with its seed ISO attached, for
/// [`cloud_init_boot_argv`].
#[derive(Debug, Clone)]
pub struct CloudInitBootConfig {
    pub overlay_path: String,
    pub seed_iso_path: String,
    pub memory: String,
    pub cpus: u32,
    pub ssh_host_port: u16,
    /// Guest architecture — selects the machine type, as it does for a normal boot.
    pub arch: VmArch,
    /// Accelerator to bake under. A TCG bake of a real distro image takes hours.
    pub accel: VmAccel,
    /// UEFI firmware pair, or `None` for a guest that boots via its own BIOS.
    pub firmware: Option<UefiFirmware>,
    /// Host directories the provisioning steps read from (e.g. a source working copy).
    pub nine_p_shares: Vec<NinePShare>,
}

/// Build the full `qemu-system-<arch>` argv to boot `config.overlay_path` with the
/// seed ISO attached and serial routed to stdio, so the host can watch it live for the
/// completion token.
///
/// **Deliberately no `-no-reboot`.** A guest *reset* has to be survivable: provisioning
/// may legitimately reboot mid-bake (the tddy host recipe swaps the cloud kernel for a
/// 9p-capable one and reboots into it), and cloud-init re-runs against the seed on the
/// next boot. Under `-no-reboot` QEMU exits on that reset, the serial reader sees EOF, and
/// the bake fails with "exited before the cloud-init completion token was observed".
/// `-no-reboot` is not needed to end the process on completion either: the completion
/// script's `shutdown -h now` is a *power-off*, which terminates QEMU on its own. The cost
/// is that a guest stuck in a boot loop is caught by the build timeout rather than by an
/// immediate EOF.
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
    let mut args = vec![
        "-machine".to_string(),
        QemuVmArgs::machine_arg(config.arch, config.accel),
        "-cpu".to_string(),
        QemuVmArgs::cpu_arg(config.arch, config.accel).to_string(),
        "-drive".to_string(),
        format!("file={},if=virtio,format=qcow2", config.overlay_path),
    ];
    args.extend(QemuVmArgs::pflash_args(config.firmware.as_ref()));
    args.extend(QemuVmArgs::seed_drive_args(&config.seed_iso_path));
    args.extend([
        "-m".to_string(),
        config.memory.clone(),
        "-smp".to_string(),
        config.cpus.to_string(),
        "-nographic".to_string(),
        "-serial".to_string(),
        "stdio".to_string(),
    ]);
    args.extend(QemuVmArgs::nine_p_args(&config.nine_p_shares));
    args.extend([
        "-netdev".to_string(),
        format!("user,id=net0,hostfwd=tcp::{}-:22", config.ssh_host_port),
        "-device".to_string(),
        "virtio-net-pci,netdev=net0".to_string(),
        "-monitor".to_string(),
        monitor,
        "-smbios".to_string(),
        "type=1,serial=ds=nocloud;".to_string(),
    ]);
    args
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

/// The form a serial console `raw` line takes in the durable boot log: the escape
/// sequences a terminal would have consumed removed, the CRLF framing trimmed, the text
/// itself untouched.
///
/// The log exists to be read after the fact — including the guest's own cloud-init logs,
/// which the bake dumps into it between [`GUEST_LOG_BEGIN_MARKER`] and
/// [`GUEST_LOG_END_MARKER`] — and a transcript still carrying colour codes and cursor
/// moves is one only a terminal can replay, not one `grep` and `sed` can cut apart.
///
/// Deliberately the *same* cleanup [`crate::serial_shell`] applies to the lines it reads
/// from a live console, rather than a second definition of what console noise is.
pub fn boot_log_line(raw: &str) -> String {
    crate::serial_shell::clean_line(raw)
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

// ── Library path mapping ────────────────────────────────────────────────────────────

/// Where a cloud-init build's inputs/outputs land in the [`crate::library::VmLibrary`],
/// instead of a bare caller-chosen `--output-dir`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloudInitLibraryPaths {
    /// The downloaded/cached input base image, imported into `images/01-base/`.
    pub base_image_in_01_base: PathBuf,
    /// The provisioned delta overlay produced by [`overlay_create_argv`] +
    /// [`build_cloud_init_image`], in `images/02-prepared-base/`.
    ///
    /// The only image the build produces: it chains onto whatever it was baked from and
    /// holds nothing of it, so there is no second, flattened half to name.
    pub prepared_overlay_output: PathBuf,
}

/// Resolve the library paths for a cloud-init build.
///
/// `base_image_name` identifies the raw, downloaded/cached input base as imported into
/// `images/01-base/` (e.g. `"debian-12"`) — distinct from `name`, the identifier for
/// *this* provisioned build's output pair in `images/02-prepared-base/` (e.g.
/// `"debian-12-nodejs"`). Multiple builds can share one imported base.
pub fn cloud_init_library_paths(
    library: &crate::library::VmLibrary,
    base_image_name: &str,
    name: &str,
) -> CloudInitLibraryPaths {
    CloudInitLibraryPaths {
        base_image_in_01_base: library
            .base_images_dir()
            .join(format!("{base_image_name}.qcow2")),
        prepared_overlay_output: library.prepared_base_dir().join(format!("{name}.qcow2")),
    }
}

// ── Orchestrator ──────────────────────────────────────────────────────────────────

/// Options for [`build_cloud_init_image`].
#[derive(Debug, Clone)]
pub struct CloudInitBuildOptions {
    pub name: String,
    /// The image this build chains onto. Read, never written, never copied — it stays the
    /// parent of the finished overlay for as long as that overlay exists.
    pub base_image_src: PathBuf,
    /// Where the finished overlay is created, and where it stays: its backing reference is
    /// relative to this file's own directory, so moving it afterwards would break the chain.
    pub overlay_output: PathBuf,
    /// Scratch space for everything the bake produces *other* than the overlay — the
    /// NoCloud seed and its ISO, a generated keypair, the boot log. Keeping it separate is
    /// what leaves `images/02-prepared-base/` holding images only.
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
    /// Architecture of the base image being baked — it must match the emulator, and on
    /// aarch64 there is no default machine type to fall back on.
    pub arch: VmArch,
    /// Accelerator to bake under.
    pub accel: VmAccel,
    /// UEFI firmware pair, or `None` for a base image that boots via its own BIOS.
    pub firmware: Option<UefiFirmware>,
    /// Host directories the provisioning steps read from.
    pub nine_p_shares: Vec<NinePShare>,
}

/// Input hashed by [`completion_token`] to derive a build-specific token —
/// deterministic for identical `(name, user_data)`, distinct otherwise.
#[derive(Serialize)]
struct TokenDataInput<'a> {
    name: &'a str,
    user_data: &'a CloudInitUserData,
}

/// Run `qemu-img` with `args`, surfacing stderr on a non-zero exit — mirrors the
/// error-handling shape of `build.rs::convert_to_qcow2`. `pub(crate)` so
/// [`crate::library::VmLibrary::create_vm`] can reuse it for its own `qemu-img create`.
///
/// `cwd` matters for exactly one argv: `create -b <relative>` validates the backing file at
/// creation time against the *process* working directory, even though the reference it then
/// records is resolved against the new image's own directory. Running from the overlay's
/// directory makes the two agree. `None` inherits the caller's, which is all an argv naming
/// every path absolutely needs.
///
/// Every argv is checked by [`crate::image_import::refuse_chain_flattening`] first: this is the
/// crate's async chokepoint for `qemu-img`, so a chain-flattening argv is refused here rather
/// than being something later readers have to notice for themselves.
pub(crate) async fn run_qemu_img(cwd: Option<&Path>, args: &[String]) -> Result<(), String> {
    crate::image_import::refuse_chain_flattening(args)?;
    let mut command = tokio::process::Command::new("qemu-img");
    command.args(args);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let out = command
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

/// Generate a fresh ed25519 keypair at `<output_dir>/id_<name>` and return the contents of
/// the resulting `.pub` file, for the seed's `authorized_keys` to carry.
async fn generate_ssh_keypair(output_dir: &Path, name: &str) -> Result<String, VmError> {
    let keys = crate::library::generate_vm_ssh_keypair(output_dir, name)?;
    tokio::fs::read_to_string(&keys.public_key_path)
        .await
        .map_err(|e| {
            VmError::BuildFailed(format!(
                "failed to read generated public key {}: {e}",
                keys.public_key_path.display()
            ))
        })
}

/// Handle one line read from the qemu serial console during [`boot_and_bake`]'s watch
/// loop: forward it to `progress`, append it to `boot_log` in its greppable form
/// ([`boot_log_line`]), and classify it.
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
    let _ = boot_log.write_all(boot_log_line(line).as_bytes()).await;
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

/// How long a timed-out bake is given to act on the monitor's `system_powerdown` before it
/// is force-killed. Short on purpose: the guest has already missed its deadline, and the
/// overlay it leaves behind is discarded either way.
const POWERDOWN_GRACE: Duration = Duration::from_secs(5);

/// How long the QEMU process is given to exit after the completion token is observed.
///
/// The completion step powers the guest off immediately after printing the token, so
/// this is normally a few seconds of orderly shutdown. It is bounded rather than an
/// unqualified `wait()` because the boot argv carries no `-no-reboot`: a guest that resets
/// instead of halting would otherwise keep the bake blocked forever.
const HALT_GRACE: Duration = Duration::from_secs(120);

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
    tokio::time::sleep(POWERDOWN_GRACE).await;
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
/// Every serial console line is both forwarded to `progress` (ephemeral) and appended to
/// `boot_log_path` (durable), so a failed or timed-out bake can be investigated after the
/// fact — the full boot log outlives the process that ran it. The durable copy is written
/// in its greppable form ([`boot_log_line`]), and the guest's own `/var/log/cloud-init.log`
/// and `/var/log/cloud-init-output.log` end up in it too: the completion step dumps both
/// onto the console between [`GUEST_LOG_BEGIN_MARKER`] and [`GUEST_LOG_END_MARKER`] before
/// signalling, on the success and the failure path alike.
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
        arch: opts.arch,
        accel: opts.accel,
        firmware: opts.firmware.clone(),
        nine_p_shares: opts.nine_p_shares.clone(),
    };
    let args = cloud_init_boot_argv(&boot_config);
    let binary = qemu_binary(opts.arch);
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

    let mut child = tokio::process::Command::new(binary)
        .args(&args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| VmError::BootFailed(format!("spawn {binary}: {e}")))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| VmError::BootFailed(format!("{binary} stdout unavailable")))?;
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
                            .write_all(format!("-- {binary} stdout closed (process exited) --\n").as_bytes())
                            .await;
                        break Err(VmError::BootFailed(format!(
                            "{binary} exited before the cloud-init completion token was observed (full log: {})",
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

    if outcome.is_ok()
        && tokio::time::timeout(HALT_GRACE, child.wait())
            .await
            .is_err()
    {
        let _ = child.kill().await;
    }
    outcome
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

/// Write `contents` to `path` restricted to its owner (`0600`) from creation.
///
/// The rendered `user-data` carries every credential the seed injects into the guest — SSH
/// keys, and whatever the caller's `write_files` hold — so it must not be readable by other
/// local accounts, not even for the span of the bake.
async fn write_owner_only(path: &Path, contents: &str) -> Result<(), VmError> {
    use tokio::io::AsyncWriteExt;

    let mut options = tokio::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options
        .open(path)
        .await
        .map_err(|e| VmError::BuildFailed(format!("failed to create {}: {e}", path.display())))?;
    file.write_all(contents.as_bytes())
        .await
        .map_err(|e| VmError::BuildFailed(format!("failed to write {}: {e}", path.display())))?;
    // `mode` above only applies when the file is created; an earlier attempt may have left
    // a more permissive one behind.
    crate::library::set_owner_only_file(path)
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
    write_owner_only(&nocloud_dir.join("user-data"), &user_data_rendered).await?;

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

/// Create the delta overlay at `overlay_output`, chained onto `base_image_src` through a
/// reference relative to the overlay's own directory. Returns the overlay's absolute path.
///
/// Created **in place**, from that same directory: the overlay is the one artifact of a bake
/// that cannot be produced somewhere convenient and moved, because moving it is precisely
/// what a relative backing reference does not survive.
///
/// Both paths are resolved to absolute before `qemu-img` sees them. They arrive relative
/// whenever the library root is — `tddy-vm-build cloud-init` defaults to
/// `default_tddy_data_dir()`, the repo-relative `tmp/.tddy`, in a debug build — and a
/// relative path handed to a process whose working directory has been set to the overlay's
/// own directory names something else entirely.
///
/// **Overwrites an existing overlay at that path**, unlocking it first: a finished layer is
/// sealed `0444`, and `qemu-img create` opens its output `O_WRONLY|O_CREAT|O_TRUNC`, which
/// such a file refuses. Re-baking a layer replaces it, so anything already chained onto the
/// old one is orphaned — the same contract [`crate::library::VmLibrary::import_base_image`]
/// has for `images/01-base/`.
pub async fn create_chained_overlay(
    overlay_output: &Path,
    base_image_src: &Path,
    disk_size: &str,
) -> Result<PathBuf, VmError> {
    let overlay_dir = match overlay_output.parent() {
        // A bare filename names the working directory, which `create_dir_all("")` cannot.
        Some(dir) if dir.as_os_str().is_empty() => Path::new("."),
        Some(dir) => dir,
        None => {
            return Err(VmError::BuildFailed(format!(
                "overlay output {} has no parent directory to chain from",
                overlay_output.display()
            )))
        }
    };
    let file_name = overlay_output.file_name().ok_or_else(|| {
        VmError::BuildFailed(format!(
            "overlay output {} does not name a file",
            overlay_output.display()
        ))
    })?;
    tokio::fs::create_dir_all(overlay_dir).await.map_err(|e| {
        VmError::BuildFailed(format!(
            "failed to create overlay dir {}: {e}",
            overlay_dir.display()
        ))
    })?;

    // The directory now exists and the parent image must already; the overlay itself does
    // not yet, so it is named from its canonical directory rather than canonicalized.
    let overlay_dir = canonical(overlay_dir).await?;
    let base_image_src = canonical(base_image_src).await?;
    let overlay_path = overlay_dir.join(file_name);

    unseal_existing_overlay(&overlay_path).await?;

    let backing = relative_backing_path(&overlay_dir, &base_image_src)?;
    run_qemu_img(
        Some(&overlay_dir),
        &overlay_create_argv(&backing, &overlay_path, disk_size),
    )
    .await
    .map_err(VmError::BuildFailed)?;
    Ok(overlay_path)
}

/// Resolve `path` — which must already exist — to an absolute, symlink-free path.
async fn canonical(path: &Path) -> Result<PathBuf, VmError> {
    tokio::fs::canonicalize(path)
        .await
        .map_err(|e| VmError::BuildFailed(format!("failed to resolve {}: {e}", path.display())))
}

/// Clear a file already occupying the overlay's path, so `qemu-img create` can write there.
///
/// Removed rather than chmod'ed back: the bytes of the previous layer are not the bytes of
/// the new one, and a truncate-in-place would leave a file that is neither until the create
/// finishes.
async fn unseal_existing_overlay(overlay_path: &Path) -> Result<(), VmError> {
    match tokio::fs::remove_file(overlay_path).await {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(VmError::BuildFailed(format!(
            "failed to remove the existing overlay {}: {e}",
            overlay_path.display()
        ))),
    }
}

/// [`create_chained_overlay`] for a bake, reporting a failure through `progress` on the way
/// out — the bake's only channel to whoever is watching it.
async fn create_overlay(
    opts: &CloudInitBuildOptions,
    progress: &(dyn Fn(&str) + Sync),
) -> Result<PathBuf, VmError> {
    create_chained_overlay(&opts.overlay_output, &opts.base_image_src, &opts.disk_size)
        .await
        .inspect_err(|e| progress(&e.to_string()))
}

/// Remove the overlay a failed bake left behind, reporting through `progress` if it cannot
/// be removed.
///
/// The overlay is created at its final, published path — a relative backing reference gives
/// it nowhere else to be — so a bake that dies mid-provisioning leaves a file that looks
/// exactly like a finished layer to the next run's existence check. Callers cache on
/// "the output is there", and only a finished layer may answer to that.
async fn discard_unbaked_overlay(overlay_path: &Path, progress: &(dyn Fn(&str) + Sync)) {
    if let Err(e) = tokio::fs::remove_file(overlay_path).await {
        progress(&format!(
            "failed to remove the unfinished overlay {}: {e} — remove it before retrying, \
             or the next build will mistake it for a finished image",
            overlay_path.display()
        ));
    }
}

/// Build a cloud-init-provisioned VM image as a delta chained onto `opts.base_image_src`.
///
/// 1. Resolves the SSH public key (reads `opts.ssh_public_key`, or generates a fresh
///    ed25519 keypair into `<output_dir>/`).
/// 2. Derives a deterministic completion token from `(opts.name, opts.user_data)`.
/// 3. Renders and writes the NoCloud `user-data`/`meta-data` seed.
/// 4. Packs the seed into a `cidata` ISO via `opts.iso_tool`.
/// 5. Creates the delta overlay at `opts.overlay_output`, chained onto
///    `opts.base_image_src` through a relative backing-file reference.
/// 6. Boots the overlay with the seed ISO attached and watches the serial console for
///    the completion token, baking the provisioning into the overlay. The full serial
///    console transcript — the guest's own cloud-init logs included — is durably logged
///    to `<output_dir>/<name>-boot.log` with its terminal escapes stripped (in addition
///    to being streamed through `progress`), so a failed or timed-out bake can be
///    investigated, and grepped, after the process has exited.
/// 7. Seals the finished overlay `0444`. Every layer chained onto it from then on depends
///    on its bytes staying exactly as baked: qcow2 has no way to detect a parent that
///    changed under its children, so the format's own answer is never to let it happen.
///
/// Returns the overlay path on success. `opts.base_image_src` is only ever read. A bake that
/// fails leaves no overlay behind (see [`discard_unbaked_overlay`]), so the presence of the
/// output file remains a reliable answer to "is this layer built?".
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

    progress("Resolving SSH public key…");
    let ssh_public_key = resolve_ssh_public_key(opts).await?;
    let token = derive_completion_token(opts)?;

    progress("Rendering cloud-init NoCloud seed…");
    let nocloud_dir = write_nocloud_seed(opts, &ssh_public_key, &token).await?;

    progress("Building cloud-init seed ISO…");
    let iso_path = build_seed_iso(opts, &nocloud_dir, progress).await?;

    progress(&format!(
        "Creating a delta overlay chained onto {}…",
        opts.base_image_src.display()
    ));
    let overlay_path = create_overlay(opts, progress).await?;

    progress("Booting QEMU to bake cloud-init provisioning into the overlay…");
    let boot_log_path = opts.output_dir.join(format!("{}-boot.log", opts.name));
    let baked = boot_and_bake(
        opts,
        &overlay_path,
        &iso_path,
        &token,
        &boot_log_path,
        progress,
    )
    .await;
    if let Err(e) = baked {
        discard_unbaked_overlay(&overlay_path, progress).await;
        return Err(e);
    }

    crate::library::set_readonly_file(&overlay_path)?;

    progress("Cloud-init image build complete");
    Ok(overlay_path)
}

// ── Per-VM login seed ─────────────────────────────────────────────────────────────

/// The provisioning document a *VM's own* NoCloud seed carries: authorize the key the
/// document is rendered against for `username`, and nothing else.
///
/// A prepared base only ever authorized the key its own bake was seeded with — the layers
/// chained below it re-render nothing — so a VM built off it and handed a fresh keypair has
/// no account its private key opens. This document is how that key reaches the guest, and
/// cloud-init's `users_groups` module applies `ssh_authorized_keys` to an account that
/// already exists just as it does to one it creates.
pub fn vm_login_user_data(username: &str) -> CloudInitUserData {
    CloudInitUserData {
        users: vec![CloudInitUser {
            name: username.to_string(),
            shell: None,
            sudo: None,
            ssh_authorized_keys: vec!["{{SSH_PUBLIC_KEY}}".to_string()],
            plain_text_passwd: None,
            // Explicitly *not* locked. This seed exists to add one SSH key to an account
            // the image already created; omitting the field lets cloud-init apply its own
            // default of `lock_passwd: true`, which locks the password the prepared base
            // set — and the serial console is the only way into a guest whose network or
            // sshd has not come up, so losing it costs the diagnostic of last resort.
            lock_passwd: Some(false),
        }],
        ..Default::default()
    }
}

/// The NoCloud instance id a VM boots under.
///
/// Distinct from the `cloud-init-<layer>` id every bake uses, and distinct per VM, so
/// cloud-init sees a new instance on an overlay whose parent has already run once and
/// applies its per-instance modules — the ssh one included — instead of skipping them.
pub fn vm_instance_id(vm_name: &str) -> String {
    format!("tddy-vm-{vm_name}")
}

/// Write the NoCloud seed authorizing `ssh_public_key` for `username` into `seed_dir`, and
/// pack it into a `cidata` ISO at `iso_output`.
///
/// Rendered with [`render_user_data_without_completion`]: the bake renderer's completion
/// script halts the guest the moment cloud-init finishes, which for a VM meant to be worked
/// with is a power-off seconds after boot.
pub async fn write_vm_login_seed_iso(
    seed_dir: &Path,
    iso_output: &Path,
    vm_name: &str,
    username: &str,
    ssh_public_key: &str,
) -> Result<PathBuf, VmError> {
    tokio::fs::create_dir_all(seed_dir).await.map_err(|e| {
        VmError::BuildFailed(format!(
            "failed to create seed dir {}: {e}",
            seed_dir.display()
        ))
    })?;

    let user_data =
        render_user_data_without_completion(&vm_login_user_data(username), ssh_public_key.trim());
    write_owner_only(&seed_dir.join("user-data"), &user_data).await?;

    let meta_data = render_meta_data(&vm_instance_id(vm_name), vm_name);
    tokio::fs::write(seed_dir.join("meta-data"), meta_data)
        .await
        .map_err(|e| VmError::BuildFailed(format!("failed to write meta-data: {e}")))?;

    let (program, args) = iso_tool_command(IsoTool::Xorriso, iso_output, seed_dir);
    run_iso_tool(&program, &args)
        .await
        .map_err(VmError::BuildFailed)?;
    Ok(iso_output.to_path_buf())
}
