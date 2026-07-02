//! Unit tests for the cloud-init image-chaining pure argv/config builders in
//! `tddy_vm::cloud_init`.
//!
//! These tests fully specify the expected qemu-img/mkisofs-family argv and rendered
//! cloud-init documents so the implementation can be verified independently of actually
//! spawning `qemu-img`, an ISO tool, or QEMU itself.

use pretty_assertions::assert_eq;
use std::path::PathBuf;
use tddy_vm::cloud_init::{
    base_convert_argv, classify_serial_line, cloud_init_boot_argv, completion_token,
    iso_tool_command, overlay_create_argv, render_meta_data, render_user_data, seed_iso_argv,
    CloudInitBootConfig, CloudInitOutcome, CloudInitUser, CloudInitUserData, IsoTool,
};

fn a_cloud_init_user_data() -> CloudInitUserData {
    CloudInitUserData {
        hostname: Some("demo-vm".to_string()),
        users: vec![CloudInitUser {
            name: "tddy".to_string(),
            shell: Some("/bin/bash".to_string()),
            sudo: Some("ALL=(ALL) NOPASSWD:ALL".to_string()),
            ssh_authorized_keys: vec!["{{SSH_PUBLIC_KEY}}".to_string()],
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
    }
}

// ── base_convert_argv ────────────────────────────────────────────────────────

/// The immutable base is produced by a plain qcow2-to-qcow2 convert (flattening any
/// prior backing chain on the source), distinct from `build.rs::convert_to_qcow2`
/// (raw-to-qcow2, used by the Buildroot pipeline).
#[test]
fn base_convert_argv_builds_qemu_img_convert_into_an_immutable_qcow2() {
    // Given a downloaded/copied base image and the immutable base output path
    let base_input = PathBuf::from("/cache/debian-12-genericcloud-amd64.qcow2");
    let base_output = PathBuf::from("/images/demo-base.qcow2");

    // When building the qemu-img convert argv
    let args = base_convert_argv(&base_input, &base_output);

    // Then it matches `qemu-img convert -f qcow2 -O qcow2 <in> <out>` exactly
    assert_eq!(
        args,
        vec![
            "convert".to_string(),
            "-f".to_string(),
            "qcow2".to_string(),
            "-O".to_string(),
            "qcow2".to_string(),
            "/cache/debian-12-genericcloud-amd64.qcow2".to_string(),
            "/images/demo-base.qcow2".to_string(),
        ]
    );
}

// ── overlay_create_argv ──────────────────────────────────────────────────────

#[test]
fn overlay_create_argv_uses_the_base_filename_as_a_relative_backing_file() {
    // Given a bare base filename (not an absolute path) and an overlay destination
    let overlay = PathBuf::from("/images/demo.qcow2");

    // When building the overlay-create argv
    let args = overlay_create_argv("demo-base.qcow2", &overlay, "20G");

    // Then the -b value is the relative basename, not an absolute path, so the overlay
    // and base can be relocated together without breaking the backing reference
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
fn rendered_user_data_embeds_a_per_boot_completion_script_that_prints_the_token_then_shuts_down() {
    // Given a completion token identifying this build
    let user_data = a_cloud_init_user_data();
    let token = "CLOUDINIT_COMPLETE_demo_abc123456789";

    // When rendering user-data
    let rendered = render_user_data(&user_data, "ssh-ed25519 AAAA...", token);

    // Then the per-boot script prints the token and shuts the guest down
    assert!(
        rendered.contains(token),
        "rendered user-data must embed the completion token, got: {rendered}"
    );
    assert!(
        rendered.contains("shutdown -h now"),
        "rendered user-data must shut the guest down on completion, got: {rendered}"
    );
}

#[test]
fn rendered_user_data_injects_a_cloud_init_clean_bootcmd_so_the_copied_base_re_runs_provisioning() {
    // Given a minimal provisioning spec (the base image already ran cloud-init once)
    let user_data = a_cloud_init_user_data();

    // When rendering user-data
    let rendered = render_user_data(
        &user_data,
        "ssh-ed25519 AAAA...",
        "CLOUDINIT_COMPLETE_demo_abc123456789",
    );

    // Then a `cloud-init clean --logs --seed` bootcmd forces re-provisioning on the copy
    assert!(
        rendered.contains("cloud-init clean --logs --seed"),
        "rendered user-data must force cloud-init to re-run against the fresh seed, got: {rendered}"
    );
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

    // Then the seed ISO is attached as a cdrom so NoCloud can find the cidata volume
    let idx = args.iter().position(|a| a == "-cdrom").unwrap();
    assert_eq!(args[idx + 1], "/images/demo-seed.iso");
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
fn cloud_init_boot_argv_disables_reboot_so_the_guest_shutdown_ends_the_process() {
    // Given a boot config
    let cfg = a_boot_config();

    // When building the boot argv
    let args = cloud_init_boot_argv(&cfg);

    // Then -no-reboot is present, so the guest's `shutdown -h now` ends the QEMU
    // process instead of rebooting it
    assert!(args.contains(&"-no-reboot".to_string()));
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
