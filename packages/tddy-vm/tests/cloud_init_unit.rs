//! Unit tests for the cloud-init image-chaining pure argv/config builders in
//! `tddy_vm::cloud_init`.
//!
//! These tests fully specify the expected qemu-img/mkisofs-family argv and rendered
//! cloud-init documents so the implementation can be verified independently of actually
//! spawning `qemu-img`, an ISO tool, or QEMU itself.

use pretty_assertions::assert_eq;
use std::path::{Path, PathBuf};
use tddy_vm::cloud_init::{
    base_convert_argv, classify_serial_line, cloud_init_boot_argv, cloud_init_library_paths,
    completion_token, iso_tool_command, overlay_create_argv, promote_prepared_base_pair,
    render_meta_data, render_user_data, render_user_data_without_completion, seed_iso_argv,
    CloudInitBootConfig, CloudInitOutcome, CloudInitUser, CloudInitUserData, IsoTool, NinePShare,
};
use tddy_vm::library::VmLibrary;
use tddy_vm::{UefiFirmware, VmAccel, VmArch};

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

// ── render_user_data_without_completion ──────────────────────────────────────

/// A guest seeded for real work — over SSH or the serial console — must not power itself
/// off the moment cloud-init finishes. Re-introducing the completion script here would kill
/// every real-boot test the moment provisioning completed.
#[test]
fn user_data_without_completion_omits_the_halt_on_completion_script() {
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
        !rendered.contains("99-tddy-cloud-init-complete.sh"),
        "the completion script must not be written at all, got: {rendered}"
    );
    assert!(
        !rendered.contains("CLOUDINIT_COMPLETE"),
        "no completion token belongs in a long-lived guest's seed, got: {rendered}"
    );
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

    // Then the halt-on-completion script is present — the two renderers differ in exactly
    // this, and nothing else
    assert!(
        rendered.contains("shutdown -h now"),
        "a bake must halt so the host can seal the overlay, got: {rendered}"
    );
    assert!(
        rendered.contains("99-tddy-cloud-init-complete.sh"),
        "the completion script must be written for a bake, got: {rendered}"
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
fn user_data_without_completion_keeps_the_netplan_and_cloud_init_clean_bootcmd() {
    // Given a provisioning spec for a guest that must stay up
    let user_data = a_cloud_init_user_data();

    // When rendering user-data without a completion token
    let rendered = render_user_data_without_completion(&user_data, "ssh-ed25519 AAAA...");

    // Then it still gets networking and still re-provisions on a copied base image — the
    // only thing dropped relative to render_user_data is the halt
    assert!(
        rendered.contains("/etc/netplan/50-tddy-cloud-init-dhcp.yaml"),
        "got: {rendered}"
    );
    assert!(
        rendered.contains("cloud-init clean --logs --seed"),
        "got: {rendered}"
    );
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

// ── promote_prepared_base_pair ────────────────────────────────────────────────

/// Two dummy files standing in for a finished bake's chained pair, in the scratch directory
/// `build_cloud_init_image` writes every artifact to.
fn a_baked_pair_in(scratch_dir: &Path, name: &str) {
    std::fs::create_dir_all(scratch_dir).unwrap();
    std::fs::write(
        scratch_dir.join(format!("{name}-base.qcow2")),
        b"immutable base",
    )
    .unwrap();
    std::fs::write(scratch_dir.join(format!("{name}.qcow2")), b"delta overlay").unwrap();
    std::fs::write(scratch_dir.join(format!("{name}-seed.iso")), b"seed").unwrap();
}

#[tokio::test]
async fn promoting_moves_both_halves_of_the_pair_out_of_the_scratch_directory() {
    // Given a finished bake's scratch directory holding the pair plus its seed ISO
    let dir = tempfile::tempdir().unwrap();
    let library = VmLibrary::new(dir.path());
    library.init().unwrap();
    let scratch_dir = library.prepared_base_dir().join("demo");
    a_baked_pair_in(&scratch_dir, "demo");
    let paths = cloud_init_library_paths(&library, "debian-12", "demo");

    // When the pair is promoted
    promote_prepared_base_pair(&scratch_dir, "demo", &paths)
        .await
        .unwrap();

    // Then both halves land flat in images/02-prepared-base/, co-located so the overlay's
    // relative backing reference still resolves, and only they moved
    assert_eq!(
        std::fs::read(&paths.prepared_base_output).unwrap(),
        b"immutable base"
    );
    assert_eq!(
        std::fs::read(&paths.prepared_overlay_output).unwrap(),
        b"delta overlay"
    );
    assert!(
        scratch_dir.join("demo-seed.iso").exists(),
        "promotion moves the images only — the seed ISO stays for its owner to clean up"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn promoting_locks_both_halves_of_the_pair_read_only() {
    use std::os::unix::fs::PermissionsExt;

    // Given a finished bake's scratch directory
    let dir = tempfile::tempdir().unwrap();
    let library = VmLibrary::new(dir.path());
    library.init().unwrap();
    let scratch_dir = library.prepared_base_dir().join("demo");
    a_baked_pair_in(&scratch_dir, "demo");
    let paths = cloud_init_library_paths(&library, "debian-12", "demo");

    // When the pair is promoted
    promote_prepared_base_pair(&scratch_dir, "demo", &paths)
        .await
        .unwrap();

    // Then both are sealed 0444 — a prepared base is shared by every VM cloned from it and
    // must not be mutated in place
    let base_mode = std::fs::metadata(&paths.prepared_base_output)
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    let overlay_mode = std::fs::metadata(&paths.prepared_overlay_output)
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(base_mode, 0o444);
    assert_eq!(overlay_mode, 0o444);
}

#[tokio::test]
async fn promoting_fails_when_the_bake_left_no_pair_behind() {
    // Given an empty scratch directory — a bake that produced nothing
    let dir = tempfile::tempdir().unwrap();
    let library = VmLibrary::new(dir.path());
    library.init().unwrap();
    let scratch_dir = library.prepared_base_dir().join("demo");
    std::fs::create_dir_all(&scratch_dir).unwrap();
    let paths = cloud_init_library_paths(&library, "debian-12", "demo");

    // When the pair is promoted
    let result = promote_prepared_base_pair(&scratch_dir, "demo", &paths).await;

    // Then it reports the missing file rather than silently publishing nothing
    let message = result.unwrap_err().to_string();
    assert!(
        message.contains("demo-base.qcow2"),
        "error must name the missing image, got: {message}"
    );
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
