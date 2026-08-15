//! Unit tests for the cloud-init image-chaining pure argv/config builders in
//! `tddy_vm::cloud_init`.
//!
//! These tests fully specify the expected qemu-img/mkisofs-family argv and rendered
//! cloud-init documents so the implementation can be verified independently of actually
//! spawning `qemu-img`, an ISO tool, or QEMU itself.

use pretty_assertions::assert_eq;
use std::path::PathBuf;
use tddy_vm::cloud_init::{
    boot_log_line, classify_serial_line, cloud_init_boot_argv, completion_token, iso_tool_command,
    overlay_create_argv, render_meta_data, render_user_data, render_user_data_without_completion,
    reset_cloud_init_and_reboot, seed_iso_argv, CloudInitBootConfig, CloudInitOutcome,
    CloudInitUser, CloudInitUserData, IsoTool, NinePShare,
};
use tddy_vm::{UefiFirmware, VmAccel, VmArch};

/// The `runcmd` entries of a rendered document, in the order cloud-init will run them.
fn rendered_runcmd(rendered: &str) -> Vec<String> {
    let doc: serde_yml::Value =
        serde_yml::from_str(rendered).expect("the rendered user-data must be YAML");
    doc["runcmd"]
        .as_sequence()
        .expect("the rendered user-data must carry a runcmd list")
        .iter()
        .map(|entry| {
            entry
                .as_str()
                .expect("every runcmd entry must be a string")
                .to_string()
        })
        .collect()
}

/// The first `runcmd` step of a rendered document — for a bake, the shell everything else
/// about completion is defined in.
fn first_runcmd_step(rendered: &str) -> String {
    rendered_runcmd(rendered)
        .first()
        .expect("the rendered user-data must carry at least one runcmd step")
        .clone()
}

/// The `bootcmd` entries of a rendered document, or an empty list when it carries no
/// `bootcmd` key at all.
fn rendered_bootcmd(rendered: &str) -> Vec<String> {
    let doc: serde_yml::Value =
        serde_yml::from_str(rendered).expect("the rendered user-data must be YAML");
    doc["bootcmd"]
        .as_sequence()
        .map(|entries| {
            entries
                .iter()
                .map(|entry| {
                    entry
                        .as_str()
                        .expect("every bootcmd entry must be a string")
                        .to_string()
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The names in a rendered document's `users` list. cloud-init accepts a mixed list — the
/// bare string `default` standing for the distro's own account, maps for the rest — so a
/// string entry names itself.
fn rendered_user_names(rendered: &str) -> Vec<String> {
    let doc: serde_yml::Value =
        serde_yml::from_str(rendered).expect("the rendered user-data must be YAML");
    doc["users"]
        .as_sequence()
        .expect("the rendered user-data must carry a users list")
        .iter()
        .map(|entry| {
            entry
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| entry["name"].as_str().unwrap_or_default().to_string())
        })
        .collect()
}

/// The `path` of every `write_files` entry of a rendered document.
fn rendered_write_file_paths(rendered: &str) -> Vec<String> {
    let doc: serde_yml::Value =
        serde_yml::from_str(rendered).expect("the rendered user-data must be YAML");
    doc["write_files"]
        .as_sequence()
        .expect("the rendered user-data must carry a write_files list")
        .iter()
        .map(|entry| {
            entry["path"]
                .as_str()
                .expect("every write_files entry must have a path")
                .to_string()
        })
        .collect()
}

fn a_cloud_init_user_data() -> CloudInitUserData {
    CloudInitUserData {
        hostname: Some("demo-vm".to_string()),
        users: vec![CloudInitUser {
            name: "tddy".to_string(),
            shell: Some("/bin/bash".to_string()),
            sudo: Some("ALL=(ALL) NOPASSWD:ALL".to_string()),
            ssh_authorized_keys: vec!["{{SSH_PUBLIC_KEY}}".to_string()],
            plain_text_passwd: None,
            lock_passwd: None,
        }],
        packages: vec!["curl".to_string()],
        runcmd: vec![],
        write_files: vec![],
        bootcmd: vec![],
    }
}

fn a_boot_config() -> CloudInitBootConfig {
    CloudInitBootConfig {
        overlay_path: "/images/demo.qcow2".to_string(),
        seed_iso_path: "/images/demo-seed.iso".to_string(),
        memory: "2048M".to_string(),
        cpus: 2,
        ssh_host_port: 2222,
        arch: VmArch::host(),
        accel: VmAccel::host_default(),
        firmware: None,
        nine_p_shares: vec![],
    }
}

// ── overlay_create_argv ──────────────────────────────────────────────────────

#[test]
fn overlay_create_argv_uses_the_parent_filename_as_a_relative_backing_file() {
    // Given a parent named relative to the overlay's own directory, not absolutely
    let overlay = PathBuf::from("/images/demo.qcow2");

    // When building the overlay-create argv
    let args = overlay_create_argv("demo-base.qcow2", &overlay, "20G");

    // Then the -b value is that relative reference, so the layer and its ancestors can be
    // relocated together without breaking the chain
    let b_index = args.iter().position(|a| a == "-b").unwrap();
    assert_eq!(args[b_index + 1], "demo-base.qcow2");
}

#[test]
fn overlay_create_argv_appends_the_requested_disk_size_as_the_final_argument() {
    // Given a disk size of 20G
    let overlay = PathBuf::from("/images/demo.qcow2");

    // When building the overlay-create argv
    let args = overlay_create_argv("demo-base.qcow2", &overlay, "20G");

    // Then the disk size is the last argument
    assert_eq!(args.last(), Some(&"20G".to_string()));
}

#[test]
fn overlay_create_argv_orders_the_flags_as_create_f_qcow2_capital_f_qcow2_b_base_overlay_size() {
    // Given a base filename, overlay path, and disk size
    let overlay = PathBuf::from("/images/demo.qcow2");

    // When building the overlay-create argv
    let args = overlay_create_argv("demo-base.qcow2", &overlay, "20G");

    // Then the full argv matches the expected flag order exactly
    assert_eq!(
        args,
        vec![
            "create".to_string(),
            "-f".to_string(),
            "qcow2".to_string(),
            "-F".to_string(),
            "qcow2".to_string(),
            "-b".to_string(),
            "demo-base.qcow2".to_string(),
            "/images/demo.qcow2".to_string(),
            "20G".to_string(),
        ]
    );
}

// ── render_user_data ─────────────────────────────────────────────────────────

#[test]
fn rendered_user_data_begins_with_the_cloud_config_header() {
    // Given a minimal provisioning spec
    let user_data = a_cloud_init_user_data();

    // When rendering the NoCloud user-data document
    let rendered = render_user_data(
        &user_data,
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5 demo@tddy",
        "CLOUDINIT_COMPLETE_demo_abc123456789",
    );

    // Then it starts with the #cloud-config header cloud-init requires
    assert!(
        rendered.starts_with("#cloud-config\n"),
        "rendered user-data must start with the #cloud-config header, got: {rendered}"
    );
}

#[test]
fn rendered_user_data_replaces_the_ssh_public_key_placeholder_with_the_provided_key() {
    // Given a user with the {{SSH_PUBLIC_KEY}} placeholder
    let user_data = a_cloud_init_user_data();

    // When rendering with a real public key
    let rendered = render_user_data(
        &user_data,
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5 demo@tddy",
        "CLOUDINIT_COMPLETE_demo_abc123456789",
    );

    // Then the placeholder is gone and the real key is embedded
    assert!(
        !rendered.contains("{{SSH_PUBLIC_KEY}}"),
        "placeholder must be replaced, got: {rendered}"
    );
    assert!(
        rendered.contains("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5 demo@tddy"),
        "real public key must be embedded, got: {rendered}"
    );
}

#[test]
fn rendered_user_data_prints_the_completion_token_and_shuts_the_guest_down() {
    // Given a completion token identifying this build
    let user_data = a_cloud_init_user_data();
    let token = "CLOUDINIT_COMPLETE_demo_abc123456789";

    // When rendering user-data
    let rendered = render_user_data(&user_data, "ssh-ed25519 AAAA...", token);

    // Then the guest is told to print the token and halt, which is what lets the host seal
    // the overlay
    assert!(
        rendered.contains(token),
        "rendered user-data must embed the completion token, got: {rendered}"
    );
    assert!(
        rendered.contains("shutdown -h now"),
        "rendered user-data must shut the guest down on completion, got: {rendered}"
    );
}

// ── completion ordering ──────────────────────────────────────────────────────

#[test]
fn the_completion_signal_is_the_last_runcmd_step_so_it_runs_after_every_caller_step() {
    // Given a bake whose provisioning is two runcmd steps
    let user_data = CloudInitUserData {
        runcmd: vec!["set -e".to_string(), "./release".to_string()],
        ..a_cloud_init_user_data()
    };

    // When rendering user-data for a bake
    let rendered = render_user_data(
        &user_data,
        "ssh-ed25519 AAAA...",
        "CLOUDINIT_COMPLETE_demo_abc123456789",
    );

    // Then the caller's steps keep their order and the completion call closes the list —
    // a token emitted anywhere earlier would report on provisioning that had not run
    let runcmd = rendered_runcmd(&rendered);
    assert_eq!(
        runcmd[runcmd.len() - 3..],
        ["set -e", "./release", "__tddy_complete_bake"]
    );
}

#[test]
fn the_completion_signal_is_not_written_as_a_per_boot_script() {
    // Given a bake spec that writes no files of its own
    let user_data = a_cloud_init_user_data();

    // When rendering user-data for a bake
    let rendered = render_user_data(
        &user_data,
        "ssh-ed25519 AAAA...",
        "CLOUDINIT_COMPLETE_demo_abc123456789",
    );

    // Then the netplan config is the only file written. cloud-init's final stage runs
    // `scripts-per-boot` *before* `scripts-user`, so a completion script under
    // /var/lib/cloud/scripts/per-boot/ halts the guest before `runcmd` ever runs and seals
    // an image that applied none of its provisioning.
    assert_eq!(
        rendered_write_file_paths(&rendered),
        ["/etc/netplan/50-tddy-cloud-init-dhcp.yaml"]
    );
}

#[test]
fn a_caller_step_that_fails_signals_failure_instead_of_leaving_the_host_to_time_out() {
    // Given a bake
    let user_data = a_cloud_init_user_data();

    // When rendering user-data for a bake
    let rendered = render_user_data(
        &user_data,
        "ssh-ed25519 AAAA...",
        "CLOUDINIT_COMPLETE_demo_abc123456789",
    );

    // Then the first runcmd step arms an EXIT trap, before any caller step can fail under
    // `set -e`, and that trap emits the failed variant — a timeout is indistinguishable
    // from a slow bake, so a failure must say so
    let completion_shell = first_runcmd_step(&rendered);
    assert!(
        completion_shell.contains("trap __tddy_on_runcmd_exit EXIT"),
        "the failure trap must be armed by the first runcmd step, got: {completion_shell}"
    );
    assert!(
        completion_shell
            .contains("__tddy_signal_and_halt \"CLOUDINIT_COMPLETE_demo_abc123456789_FAILED\""),
        "the trap must emit the failed token variant, got: {completion_shell}"
    );
}

// ── guest cloud-init logs on the serial console ──────────────────────────────

#[test]
fn both_guest_cloud_init_logs_are_dumped_before_the_completion_token_reaches_the_host() {
    // Given a bake
    let user_data = a_cloud_init_user_data();

    // When rendering user-data for a bake
    let rendered = render_user_data(
        &user_data,
        "ssh-ed25519 AAAA...",
        "CLOUDINIT_COMPLETE_demo_abc123456789",
    );

    // Then the guest dumps both logs and only then echoes the token — the host stops
    // reading the console the moment it sees the token
    let completion_shell = first_runcmd_step(&rendered);
    assert!(
        completion_shell.contains(
            "  __tddy_dump_guest_log /var/log/cloud-init.log\n  \
             __tddy_dump_guest_log /var/log/cloud-init-output.log\n  echo \"$1\"\n"
        ),
        "both guest logs must be dumped ahead of the token, got: {completion_shell}"
    );
}

#[test]
fn each_dumped_guest_log_is_framed_by_markers_the_host_can_grep_for() {
    // Given a bake
    let user_data = a_cloud_init_user_data();

    // When rendering user-data for a bake
    let rendered = render_user_data(
        &user_data,
        "ssh-ed25519 AAAA...",
        "CLOUDINIT_COMPLETE_demo_abc123456789",
    );

    // Then each dump is bracketed by a begin/end marker naming the log it frames, so one
    // log can be cut out of the boot log by `grep`/`sed`
    let completion_shell = first_runcmd_step(&rendered);
    assert!(
        completion_shell.contains("echo \"TDDY_GUEST_LOG_BEGIN $__tddy_log\""),
        "each dumped guest log must open with a greppable marker, got: {completion_shell}"
    );
    assert!(
        completion_shell.contains("echo \"TDDY_GUEST_LOG_END $__tddy_log\""),
        "each dumped guest log must close with a greppable marker, got: {completion_shell}"
    );
}

#[test]
fn a_dumped_guest_log_cannot_impersonate_the_completion_token() {
    // Given a bake
    let user_data = a_cloud_init_user_data();

    // When rendering user-data for a bake
    let rendered = render_user_data(
        &user_data,
        "ssh-ed25519 AAAA...",
        "CLOUDINIT_COMPLETE_demo_abc123456789",
    );

    // Then the dump rewrites the token wherever the guest's own logs happen to carry it:
    // the host classifies the console line by line, so a log line quoting the token would
    // otherwise end the watch — reporting success on a bake that had just failed
    let completion_shell = first_runcmd_step(&rendered);
    assert!(
        completion_shell.contains(
            "sed \"s/CLOUDINIT_COMPLETE_demo_abc123456789/CLOUDINIT_TOKEN_ELIDED/g\" \
             \"$__tddy_snapshot\""
        ),
        "the guest log dump must elide the completion token, got: {completion_shell}"
    );
}

// ── boot_log_line ────────────────────────────────────────────────────────────

#[test]
fn a_serial_line_reaches_the_durable_boot_log_with_its_terminal_escapes_stripped() {
    // Given a serial console line wrapped in colour escapes and CRLF framing
    let raw = "\u{1b}[0;32m[  OK  ] Started cloud-final.service.\u{1b}[0m\r";

    // When deriving what the durable boot log records for it
    let logged = boot_log_line(raw);

    // Then only the text a human would have seen survives, so the log is greppable
    assert_eq!(logged, "[  OK  ] Started cloud-final.service.");
}

#[test]
fn a_guest_log_marker_reaches_the_durable_boot_log_verbatim() {
    // Given the begin marker as the guest's console emits it, escape-prefixed and CR-framed
    let raw = "\u{1b}[0mTDDY_GUEST_LOG_BEGIN /var/log/cloud-init.log\r";

    // When deriving what the durable boot log records for it
    let logged = boot_log_line(raw);

    // Then the marker line is exactly what a caller greps for — stripping escapes must not
    // cost content
    assert_eq!(logged, "TDDY_GUEST_LOG_BEGIN /var/log/cloud-init.log");
}

// ── resetting cloud-init ─────────────────────────────────────────────────────

/// `cloud-init clean` deletes `/var/lib/cloud/instance/`, which is where cloud-init writes
/// the `runcmd` script it is about to run. From `bootcmd` — the init stage, before the
/// config stage renders that script — it therefore destroys the provisioning of the very
/// boot it belongs to. Observed in a live guest after a bake: `/var/lib/cloud/instance/` was
/// gone, every script module finished in under a millisecond because there was nothing left
/// to run, and the host sealed an image with none of its `packages:` or `runcmd:` applied.
#[test]
fn a_bake_never_resets_cloud_init_at_boot_which_would_delete_the_runcmd_it_is_about_to_run() {
    // Given a bake spec carrying a bootcmd of its own
    let user_data = CloudInitUserData {
        bootcmd: vec!["modprobe 9pnet_virtio".to_string()],
        ..a_cloud_init_user_data()
    };

    // When rendering user-data for a bake
    let rendered = render_user_data(
        &user_data,
        "ssh-ed25519 AAAA...",
        "CLOUDINIT_COMPLETE_demo_abc123456789",
    );

    // Then the caller's own entry is the whole of bootcmd
    assert_eq!(rendered_bootcmd(&rendered), ["modprobe 9pnet_virtio"]);
}

#[test]
fn a_mid_bake_reboot_discards_the_instance_state_first_so_the_next_boot_re_runs_provisioning() {
    // Given a provisioning step that has to reboot the guest to continue

    // When building the shell it reboots with
    let command = reset_cloud_init_and_reboot();

    // Then cloud-init's record of this instance goes first — a boot that finds an instance
    // it has already provisioned skips `runcmd` entirely, so the steps after the reboot
    // would never run — and a reset that fails aborts the bake instead of rebooting into a
    // guest that will never finish
    assert_eq!(
        command,
        "cloud-init clean --logs --seed || exit 1; sync; systemctl reboot"
    );
}

// ── render_user_data_without_completion ──────────────────────────────────────

/// A guest seeded for real work — over SSH or the serial console — must not power itself
/// off the moment cloud-init finishes. Re-introducing the completion steps here would kill
/// every real-boot test the moment provisioning completed.
#[test]
fn user_data_without_completion_omits_the_halt_on_completion_steps() {
    // Given a provisioning spec for a guest that must stay up
    let user_data = a_cloud_init_user_data();

    // When rendering user-data without a completion token
    let rendered = render_user_data_without_completion(&user_data, "ssh-ed25519 AAAA...");

    // Then nothing in the document powers the guest down
    assert!(
        !rendered.contains("shutdown -h now"),
        "a long-lived guest must not be told to halt, got: {rendered}"
    );
    assert!(
        !rendered.contains("__tddy_"),
        "none of the completion machinery belongs here, got: {rendered}"
    );
    assert!(
        !rendered.contains("CLOUDINIT_COMPLETE"),
        "no completion token belongs in a long-lived guest's seed, got: {rendered}"
    );
}

#[test]
fn user_data_without_completion_runs_exactly_the_runcmd_steps_it_was_given() {
    // Given a provisioning spec for a guest that must stay up
    let user_data = CloudInitUserData {
        runcmd: vec!["set -e".to_string(), "./release".to_string()],
        ..a_cloud_init_user_data()
    };

    // When rendering user-data without a completion token
    let rendered = render_user_data_without_completion(&user_data, "ssh-ed25519 AAAA...");

    // Then the caller's steps are the whole of runcmd — no completion step is appended
    assert_eq!(rendered_runcmd(&rendered), ["set -e", "./release"]);
}

#[test]
fn user_data_with_a_completion_token_still_halts_the_guest() {
    // Given the same provisioning spec, rendered for a bake
    let user_data = a_cloud_init_user_data();

    // When rendering user-data with a completion token
    let rendered = render_user_data(
        &user_data,
        "ssh-ed25519 AAAA...",
        "CLOUDINIT_COMPLETE_demo_abc123456789",
    );

    // Then the halt-on-completion steps are present — the two renderers differ in exactly
    // this, and nothing else
    assert!(
        rendered.contains("shutdown -h now"),
        "a bake must halt so the host can seal the overlay, got: {rendered}"
    );
    assert_eq!(
        rendered_runcmd(&rendered).last().map(String::as_str),
        Some("__tddy_complete_bake")
    );
}

#[test]
fn user_data_without_completion_renders_the_users_packages_and_runcmd_it_was_given() {
    // Given a provisioning spec with a user, a package, and a command
    let user_data = CloudInitUserData {
        packages: vec!["curl".to_string()],
        runcmd: vec!["systemctl enable tddy-daemon".to_string()],
        ..a_cloud_init_user_data()
    };

    // When rendering user-data without a completion token
    let rendered = render_user_data_without_completion(
        &user_data,
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5 demo@tddy",
    );

    // Then the caller's directives survive, key substitution included
    assert!(rendered.starts_with("#cloud-config\n"), "got: {rendered}");
    assert!(rendered.contains("hostname: demo-vm"), "got: {rendered}");
    assert!(rendered.contains("name: tddy"), "got: {rendered}");
    assert!(rendered.contains("- curl"), "got: {rendered}");
    assert!(
        rendered.contains("systemctl enable tddy-daemon"),
        "got: {rendered}"
    );
    assert!(
        rendered.contains("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5 demo@tddy"),
        "got: {rendered}"
    );
}

#[test]
fn user_data_without_completion_still_configures_the_guest_network() {
    // Given a provisioning spec for a guest that must stay up
    let user_data = a_cloud_init_user_data();

    // When rendering user-data without a completion token
    let rendered = render_user_data_without_completion(&user_data, "ssh-ed25519 AAAA...");

    // Then it still gets networking — the only thing dropped relative to render_user_data
    // is the halt
    assert!(
        rendered.contains("/etc/netplan/50-tddy-cloud-init-dhcp.yaml"),
        "got: {rendered}"
    );
}

#[test]
fn user_data_without_completion_leaves_the_bootcmd_empty_when_the_caller_asked_for_none() {
    // Given a provisioning spec for a guest that must stay up
    let user_data = a_cloud_init_user_data();

    // When rendering user-data without a completion token
    let rendered = render_user_data_without_completion(&user_data, "ssh-ed25519 AAAA...");

    // Then nothing runs at boot ahead of the caller's own provisioning
    assert_eq!(rendered_bootcmd(&rendered), Vec::<String>::new());
}

// ── users: the distro default ────────────────────────────────────────────────

/// A `users:` list *replaces* the distro's default account rather than adding to it, but
/// `cc_ssh_authkey_fingerprints` still looks that account up by name — and dies on a guest
/// where it was never created: `KeyError: "getpwnam(): name not found: 'debian'"`, which
/// fails `cloud-final.service` with exit 1 even when every provisioning step succeeded.
/// cloud-init's own answer is the bare string `default` among the user maps.
#[test]
fn the_rendered_users_list_keeps_the_distro_default_account_ahead_of_the_ones_it_defines() {
    // Given a spec defining one account of its own
    let user_data = a_cloud_init_user_data();

    // When rendering user-data
    let rendered = render_user_data_without_completion(&user_data, "ssh-ed25519 AAAA...");

    // Then the distro's own account leads the list, and the spec's account follows it
    assert_eq!(rendered_user_names(&rendered), ["default", "tddy"]);
}

#[test]
fn a_spec_defining_no_accounts_of_its_own_renders_no_users_key_at_all() {
    // Given a spec that defines no accounts — every account it needs came from the image it
    // chains onto
    let user_data = CloudInitUserData {
        users: vec![],
        ..a_cloud_init_user_data()
    };

    // When rendering user-data
    let rendered = render_user_data_without_completion(&user_data, "ssh-ed25519 AAAA...");

    // Then the key is absent, which is what leaves cloud-init's own default in force —
    // rendering a lone `default` would say the same thing in more words
    assert!(!rendered.contains("users:"), "got: {rendered}");
}

// ── users: plain_text_passwd / lock_passwd ───────────────────────────────────

#[test]
fn a_user_with_an_unlocked_console_password_renders_both_password_keys() {
    // Given a user given a serial-console password, with the password lock lifted
    let user_data = CloudInitUserData {
        users: vec![CloudInitUser {
            name: "tddy".to_string(),
            shell: None,
            sudo: None,
            ssh_authorized_keys: vec![],
            plain_text_passwd: Some("console-pass".to_string()),
            lock_passwd: Some(false),
        }],
        ..a_cloud_init_user_data()
    };

    // When rendering user-data
    let rendered = render_user_data_without_completion(&user_data, "ssh-ed25519 AAAA...");

    // Then both keys reach the document — cloud-init locks every account's password by
    // default, so the password alone would not let anyone log in on the console
    assert!(
        rendered.contains("plain_text_passwd: console-pass"),
        "got: {rendered}"
    );
    assert!(rendered.contains("lock_passwd: false"), "got: {rendered}");
}

#[test]
fn a_user_with_no_console_password_omits_both_password_keys_entirely() {
    // Given a user with neither a password nor a lock setting
    let user_data = a_cloud_init_user_data();

    // When rendering user-data
    let rendered = render_user_data_without_completion(&user_data, "ssh-ed25519 AAAA...");

    // Then neither key appears — an emitted `plain_text_passwd: null` or
    // `lock_passwd: null` is not the same document to cloud-init
    assert!(!rendered.contains("plain_text_passwd"), "got: {rendered}");
    assert!(!rendered.contains("lock_passwd"), "got: {rendered}");
}

// ── render_meta_data ──────────────────────────────────────────────────────────

#[test]
fn rendered_meta_data_carries_a_deterministic_instance_id_and_hostname() {
    // Given an instance id and hostname
    // When rendering the NoCloud meta-data document
    let rendered = render_meta_data("cloud-init-demo", "demo-vm");

    // Then both fields are present verbatim
    assert_eq!(
        rendered,
        "instance-id: cloud-init-demo\nlocal-hostname: demo-vm\n"
    );
}

// ── seed_iso_argv / iso_tool_command ─────────────────────────────────────────

#[test]
fn seed_iso_argv_builds_a_cidata_volume_with_joliet_and_rock_ridge_extensions() {
    // Given the seed ISO output path and the NoCloud source directory
    let iso = PathBuf::from("/images/demo-seed.iso");
    let nocloud_dir = PathBuf::from("/images/seed/nocloud");

    // When building the mkisofs-family argv
    let args = seed_iso_argv(&iso, &nocloud_dir);

    // Then it matches the NoCloud `cidata` volume convention exactly
    assert_eq!(
        args,
        vec![
            "-output".to_string(),
            "/images/demo-seed.iso".to_string(),
            "-volid".to_string(),
            "cidata".to_string(),
            "-joliet".to_string(),
            "-rock".to_string(),
            "/images/seed/nocloud".to_string(),
        ]
    );
}

#[test]
fn iso_tool_resolves_to_xorriso_mkisofs_emulation_with_the_mkisofs_argument_prefix() {
    // Given the seed ISO output path and NoCloud directory
    let iso = PathBuf::from("/images/demo-seed.iso");
    let nocloud_dir = PathBuf::from("/images/seed/nocloud");

    // When resolving the xorriso command
    let (program, args) = iso_tool_command(IsoTool::Xorriso, &iso, &nocloud_dir);

    // Then it runs xorriso in mkisofs-emulation mode ahead of the shared mkisofs argv
    assert_eq!(program, "xorriso");
    assert_eq!(&args[..2], &["-as".to_string(), "mkisofs".to_string()]);
    assert_eq!(&args[2..], &seed_iso_argv(&iso, &nocloud_dir)[..]);
}

// ── completion_token ──────────────────────────────────────────────────────────

#[test]
fn completion_token_is_deterministic_for_identical_provisioning_input() {
    // Given the same name and token data twice
    let a = completion_token("demo", "{\"users\":[]}");
    let b = completion_token("demo", "{\"users\":[]}");

    // When compared
    // Then the derived tokens are identical
    assert_eq!(a, b);
}

#[test]
fn completion_token_changes_when_the_user_data_changes() {
    // Given two different provisioning inputs for the same name
    let a = completion_token("demo", "{\"users\":[]}");
    let b = completion_token("demo", "{\"users\":[{\"name\":\"tddy\"}]}");

    // When compared
    // Then the tokens differ
    assert!(
        a != b,
        "tokens for different provisioning input must differ"
    );
}

#[test]
fn completion_token_is_prefixed_with_the_target_name_and_a_twelve_character_hash() {
    // Given a provisioning input
    let token = completion_token("demo", "{\"users\":[]}");

    // When inspecting its shape
    // Then it matches CLOUDINIT_COMPLETE_<name>_<12-hex-char hash>
    let prefix = "CLOUDINIT_COMPLETE_demo_";
    assert!(token.starts_with(prefix), "got: {token}");
    let hash_part = &token[prefix.len()..];
    assert_eq!(
        hash_part.len(),
        12,
        "hash suffix must be 12 characters, got: {hash_part}"
    );
    assert!(
        hash_part.chars().all(|c| c.is_ascii_hexdigit()),
        "hash suffix must be hex, got: {hash_part}"
    );
}

// ── cloud_init_boot_argv ──────────────────────────────────────────────────────

#[test]
fn cloud_init_boot_argv_attaches_the_overlay_as_a_virtio_qcow2_drive() {
    // Given a boot config pointing at the provisioned overlay
    let cfg = a_boot_config();

    // When building the boot argv
    let args = cloud_init_boot_argv(&cfg);

    // Then the overlay is attached as a virtio qcow2 drive
    let idx = args.iter().position(|a| a == "-drive").unwrap();
    assert_eq!(
        args[idx + 1],
        "file=/images/demo.qcow2,if=virtio,format=qcow2"
    );
}

#[test]
fn cloud_init_boot_argv_attaches_the_seed_iso_as_a_cdrom() {
    // Given a boot config pointing at the seed ISO
    let cfg = a_boot_config();

    // When building the boot argv
    let args = cloud_init_boot_argv(&cfg);

    // Then the seed ISO is attached as a read-only virtio disk, which every supported
    // guest kernel can see — a CD-ROM is invisible to Debian's virtio-only `-cloud` kernel
    // on x86_64, where `-cdrom` lands on the IDE bus
    assert!(
        args.iter()
            .any(|a| a == "file=/images/demo-seed.iso,if=virtio,format=raw,readonly=on"),
        "seed must be attached as a read-only virtio drive; got: {args:?}"
    );
}

#[test]
fn cloud_init_boot_argv_routes_the_serial_console_to_stdio_for_token_watching() {
    // Given a boot config
    let cfg = a_boot_config();

    // When building the boot argv
    let args = cloud_init_boot_argv(&cfg);

    // Then serial goes to stdio (not a file), so the host can watch it live for the
    // completion token
    let idx = args.iter().position(|a| a == "-serial").unwrap();
    assert_eq!(args[idx + 1], "stdio");
}

#[test]
fn cloud_init_boot_argv_survives_a_guest_reboot_by_never_passing_no_reboot() {
    // Given a boot config
    let cfg = a_boot_config();

    // When building the boot argv
    let args = cloud_init_boot_argv(&cfg);

    // Then -no-reboot is absent. Provisioning may legitimately reboot mid-bake (the tddy
    // host recipe swaps the cloud kernel for a 9p-capable one), and under -no-reboot QEMU
    // exits on that reset — the serial reader hits EOF and the bake fails before doing any
    // work. The completion script's `shutdown -h now` is a power-off, which ends the
    // process without this flag.
    assert!(
        !args.contains(&"-no-reboot".to_string()),
        "the bake must tolerate a guest reboot, got: {args:?}"
    );
}

#[test]
fn cloud_init_boot_argv_attaches_the_uefi_firmware_pair_as_read_only_code_and_writable_vars() {
    // Given a boot config for a guest that boots through UEFI rather than its own BIOS
    let cfg = CloudInitBootConfig {
        firmware: Some(UefiFirmware {
            code_path: "/firmware/edk2-aarch64-code.fd".to_string(),
            vars_path: "/images/demo-vars.fd".to_string(),
        }),
        ..a_boot_config()
    };

    // When building the boot argv
    let args = cloud_init_boot_argv(&cfg);

    // Then the pflash pair follows the overlay drive: firmware code read-only on unit 0,
    // this guest's own variables store writable on unit 1
    let pflash_idx = args
        .iter()
        .position(|a| a.starts_with("if=pflash"))
        .unwrap();
    assert_eq!(
        args[pflash_idx - 2..pflash_idx + 3],
        [
            "file=/images/demo.qcow2,if=virtio,format=qcow2".to_string(),
            "-drive".to_string(),
            "if=pflash,format=raw,unit=0,readonly=on,file=/firmware/edk2-aarch64-code.fd"
                .to_string(),
            "-drive".to_string(),
            "if=pflash,format=raw,unit=1,file=/images/demo-vars.fd".to_string(),
        ]
    );
}

#[test]
fn cloud_init_boot_argv_exports_the_source_share_read_only_over_virtio_9p() {
    // Given a boot config carrying the operator's working copy as a read-only 9p share —
    // the entire mechanism by which the working copy reaches a tddy host bake
    let cfg = CloudInitBootConfig {
        nine_p_shares: vec![NinePShare {
            host_path: "/home/dev/tddy-coder".to_string(),
            mount_tag: "tddy-src".to_string(),
            writable: false,
        }],
        ..a_boot_config()
    };

    // When building the boot argv
    let args = cloud_init_boot_argv(&cfg);

    // Then the fsdev/device pair exports it under the tag the guest mounts, read-only
    let fsdev_idx = args.iter().position(|a| a == "-fsdev").unwrap();
    assert_eq!(
        args[fsdev_idx + 1],
        "local,id=fsdev0,path=/home/dev/tddy-coder,security_model=none,readonly=on"
    );
    assert_eq!(
        args[fsdev_idx + 3],
        "virtio-9p-pci,fsdev=fsdev0,mount_tag=tddy-src"
    );
}

#[test]
fn cloud_init_boot_argv_exports_no_9p_device_when_no_share_is_configured() {
    // Given a boot config with no 9p shares
    let cfg = a_boot_config();

    // When building the boot argv
    let args = cloud_init_boot_argv(&cfg);

    // Then no fsdev is emitted at all
    assert!(!args.contains(&"-fsdev".to_string()), "got: {args:?}");
}

#[test]
fn cloud_init_boot_argv_forwards_ssh_and_sets_memory_and_cpu_count() {
    // Given a boot config with an SSH host port, memory size, and cpu count
    let cfg = a_boot_config();

    // When building the boot argv
    let args = cloud_init_boot_argv(&cfg);

    // Then the hostfwd, memory, and smp flags carry the configured values
    let netdev_idx = args.iter().position(|a| a == "-netdev").unwrap();
    assert_eq!(args[netdev_idx + 1], "user,id=net0,hostfwd=tcp::2222-:22");

    let m_idx = args.iter().position(|a| a == "-m").unwrap();
    assert_eq!(args[m_idx + 1], "2048M");

    let smp_idx = args.iter().position(|a| a == "-smp").unwrap();
    assert_eq!(args[smp_idx + 1], "2");
}

// ── classify_serial_line ──────────────────────────────────────────────────────

#[test]
fn serial_watcher_reports_success_when_the_completion_token_appears_alone() {
    // Given a serial line that is exactly the completion token
    let token = "CLOUDINIT_COMPLETE_demo_abc123456789";

    // When classifying the line
    let outcome = classify_serial_line(token, token);

    // Then it reports success
    assert_eq!(outcome, CloudInitOutcome::Succeeded);
}

#[test]
fn serial_watcher_reports_failure_when_the_failed_token_variant_appears() {
    // Given a serial line carrying the `_FAILED` token variant
    let token = "CLOUDINIT_COMPLETE_demo_abc123456789";
    let line = format!("{token}_FAILED");

    // When classifying the line
    let outcome = classify_serial_line(&line, token);

    // Then it reports failure, not success (the failed variant must be checked before
    // the bare token, since it contains the bare token as a substring)
    assert_eq!(outcome, CloudInitOutcome::Failed);
}

#[test]
fn serial_watcher_stays_pending_for_unrelated_boot_output() {
    // Given ordinary kernel/cloud-init boot chatter unrelated to the token
    let token = "CLOUDINIT_COMPLETE_demo_abc123456789";
    let line = "[    2.345678] cloud-init[123]: Cloud-init v. running 'init-local'";

    // When classifying the line
    let outcome = classify_serial_line(line, token);

    // Then it reports pending — neither success nor failure has been observed yet
    assert_eq!(outcome, CloudInitOutcome::Pending);
}
