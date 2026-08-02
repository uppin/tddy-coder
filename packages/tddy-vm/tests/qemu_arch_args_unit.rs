//! Unit tests for the architecture, acceleration, resource, UEFI, 9p, and SSH-login
//! additions to the QEMU launcher.
//!
//! The existing `qemu_args_unit.rs` covers the pre-existing shape (drive, netdev, monitor,
//! serial). These pin what the launcher must emit so it can boot a real cloud image and log
//! into it.

use std::path::Path;

use pretty_assertions::assert_eq;
use tddy_vm::cloud_init::NinePShare;
use tddy_vm::qemu::{ensure_uefi_vars_file, qemu_binary, ssh_destination, ssh_opts, QemuVmArgs};
use tddy_vm::vm::{PortForward, RunningVm, UefiFirmware, VmAccel, VmArch, VmConfig, VmLogin};

fn an_aarch64_config() -> VmConfig {
    VmConfig {
        qcow2_path: "/images/guest.qcow2".to_string(),
        extra_hostfwd: vec![],
        ssh_host_port: 2222,
        arch: VmArch::Aarch64,
        accel: VmAccel::Hvf,
        memory: "2048M".to_string(),
        cpus: 4,
        firmware: Some(UefiFirmware {
            code_path: "/qemu/share/edk2-aarch64-code.fd".to_string(),
            vars_path: "/vms/guest-vars.fd".to_string(),
        }),
        login: VmLogin {
            username: "tddy".to_string(),
            private_key_path: Some("/vms/id_guest".to_string()),
        },
        seed_iso: None,
        nine_p_shares: vec![],
    }
}

/// A running guest reached over SSH as the `tddy` user with its own per-VM key.
fn a_running_vm() -> RunningVm {
    RunningVm {
        ssh_host_port: 2222,
        monitor_socket: "/tmp/tddy-vm-monitor-2222.sock".to_string(),
        pid: 4242,
        login: VmLogin {
            username: "tddy".to_string(),
            private_key_path: Some("/vms/guest/id_guest".to_string()),
        },
    }
}

/// Find the value that follows `flag` in an argv vector.
fn value_after(argv: &[String], flag: &str) -> String {
    let index = argv
        .iter()
        .position(|a| a == flag)
        .unwrap_or_else(|| panic!("argv must contain {flag}: {argv:?}"));
    argv[index + 1].clone()
}

/// Collect every value that follows each occurrence of `flag`.
fn values_after(argv: &[String], flag: &str) -> Vec<String> {
    argv.iter()
        .enumerate()
        .filter(|(_, a)| a.as_str() == flag)
        .map(|(i, _)| argv[i + 1].clone())
        .collect()
}

#[test]
fn selects_the_aarch64_system_emulator_for_an_aarch64_guest() {
    // Given an aarch64 guest
    let arch = VmArch::Aarch64;

    // When the emulator binary is resolved
    let binary = qemu_binary(arch);

    // Then the aarch64 emulator is chosen, not the x86_64 one
    assert_eq!(binary, "qemu-system-aarch64");
}

#[test]
fn selects_the_x86_64_system_emulator_for_an_x86_64_guest() {
    // Given an x86_64 guest
    let arch = VmArch::X86_64;

    // When the emulator binary is resolved
    let binary = qemu_binary(arch);

    // Then the x86_64 emulator is chosen
    assert_eq!(binary, "qemu-system-x86_64");
}

#[test]
fn boots_an_aarch64_guest_on_the_virt_machine_with_hardware_acceleration() {
    // Given an HVF-accelerated aarch64 guest
    let config = an_aarch64_config();

    // When the argv is built
    let argv = QemuVmArgs::build(&config);

    // Then the machine type carries the accelerator — aarch64 has no default machine
    assert_eq!(value_after(&argv, "-machine"), "virt,accel=hvf");
}

#[test]
fn passes_through_the_host_cpu_when_hardware_accelerated() {
    // Given an HVF-accelerated guest
    let config = an_aarch64_config();

    // When the argv is built
    let argv = QemuVmArgs::build(&config);

    // Then the host CPU is passed through
    assert_eq!(value_after(&argv, "-cpu"), "host");
}

#[test]
fn selects_an_emulated_cpu_when_running_under_tcg() {
    // Given an aarch64 guest with no hardware acceleration available
    let config = VmConfig {
        accel: VmAccel::Tcg,
        ..an_aarch64_config()
    };

    // When the argv is built
    let argv = QemuVmArgs::build(&config);

    // Then a concrete emulated CPU is named — `-cpu host` is invalid under TCG
    assert_eq!(value_after(&argv, "-machine"), "virt,accel=tcg");
    assert_eq!(value_after(&argv, "-cpu"), "cortex-a72");
}

#[test]
fn boots_an_x86_64_guest_on_the_q35_machine_with_kvm() {
    // Given an x86_64 guest on a Linux host with KVM available
    let config = VmConfig {
        arch: VmArch::X86_64,
        accel: VmAccel::Kvm,
        firmware: None,
        ..an_aarch64_config()
    };

    // When the argv is built
    let argv = QemuVmArgs::build(&config);

    // Then q35 carries KVM and the host CPU is passed straight through
    assert_eq!(value_after(&argv, "-machine"), "q35,accel=kvm");
    assert_eq!(value_after(&argv, "-cpu"), "host");
}

#[test]
fn selects_the_widest_emulated_x86_64_cpu_when_running_under_tcg() {
    // Given an x86_64 guest with no accelerator, as on a CI runner without /dev/kvm
    let config = VmConfig {
        arch: VmArch::X86_64,
        accel: VmAccel::Tcg,
        firmware: None,
        ..an_aarch64_config()
    };

    // When the argv is built
    let argv = QemuVmArgs::build(&config);

    // Then a concrete emulated CPU is named — `-cpu host` is invalid under TCG
    assert_eq!(value_after(&argv, "-machine"), "q35,accel=tcg");
    assert_eq!(value_after(&argv, "-cpu"), "max");
}

#[test]
fn honours_the_memory_and_cpu_count_from_the_run_policy() {
    // Given a guest asking for 2048M and 4 vCPUs
    let config = an_aarch64_config();

    // When the argv is built
    let argv = QemuVmArgs::build(&config);

    // Then both reach QEMU, instead of the hard-coded 512M and absent -smp
    assert_eq!(value_after(&argv, "-m"), "2048M");
    assert_eq!(value_after(&argv, "-smp"), "4");
}

#[test]
fn attaches_uefi_firmware_as_a_read_only_code_and_writable_vars_pflash_pair() {
    // Given a guest with UEFI firmware configured
    let config = an_aarch64_config();

    // When the argv is built
    let argv = QemuVmArgs::build(&config);

    // Then the code half is read-only and the vars half is writable — aarch64 virt has no
    // legacy BIOS, so without this pair the guest cannot boot at all
    let drives = values_after(&argv, "-drive");
    assert!(
        drives.contains(
            &"if=pflash,format=raw,unit=0,readonly=on,file=/qemu/share/edk2-aarch64-code.fd"
                .to_string()
        ),
        "argv must attach read-only firmware code: {drives:?}"
    );
    assert!(
        drives.contains(&"if=pflash,format=raw,unit=1,file=/vms/guest-vars.fd".to_string()),
        "argv must attach writable firmware vars: {drives:?}"
    );
}

#[test]
fn omits_pflash_drives_when_no_firmware_is_configured() {
    // Given an x86_64 guest booting via its own BIOS
    let config = VmConfig {
        arch: VmArch::X86_64,
        firmware: None,
        ..an_aarch64_config()
    };

    // When the argv is built
    let argv = QemuVmArgs::build(&config);

    // Then no firmware drives are attached
    let drives = values_after(&argv, "-drive");
    assert!(
        !drives.iter().any(|d| d.contains("pflash")),
        "argv must not attach pflash when no firmware is configured: {drives:?}"
    );
}

#[test]
fn attaches_the_cloud_init_seed_iso_when_one_is_configured() {
    // Given a guest with a NoCloud seed to consume on first boot
    let config = VmConfig {
        seed_iso: Some("/vms/guest-seed.iso".to_string()),
        ..an_aarch64_config()
    };

    // When the argv is built
    let argv = QemuVmArgs::build(&config);

    // Then the seed is attached as a CD-ROM
    assert_eq!(value_after(&argv, "-cdrom"), "/vms/guest-seed.iso");
}

#[test]
fn shares_a_read_only_host_directory_into_the_guest_over_virtio_nine_p() {
    // Given a guest with the operator's working copy shared read-only
    let config = VmConfig {
        nine_p_shares: vec![NinePShare {
            host_path: "/home/dev/tddy-coder".to_string(),
            mount_tag: "tddy-src".to_string(),
            writable: false,
        }],
        ..an_aarch64_config()
    };

    // When the argv is built
    let argv = QemuVmArgs::build(&config);

    // Then the share is exported read-only under its mount tag
    assert_eq!(
        value_after(&argv, "-fsdev"),
        "local,id=fsdev0,path=/home/dev/tddy-coder,security_model=none,readonly=on"
    );
    assert_eq!(
        value_after(&argv, "-device"),
        "virtio-9p-pci,fsdev=fsdev0,mount_tag=tddy-src"
    );
}

#[test]
fn exports_a_writable_nine_p_share_without_the_read_only_flag() {
    // Given a writable share
    let config = VmConfig {
        nine_p_shares: vec![NinePShare {
            host_path: "/home/dev/out".to_string(),
            mount_tag: "tddy-out".to_string(),
            writable: true,
        }],
        ..an_aarch64_config()
    };

    // When the argv is built
    let argv = QemuVmArgs::build(&config);

    // Then the fsdev carries no readonly flag
    assert_eq!(
        value_after(&argv, "-fsdev"),
        "local,id=fsdev0,path=/home/dev/out,security_model=none"
    );
}

#[test]
fn gives_each_of_two_shares_its_own_fsdev_so_the_second_does_not_replace_the_first() {
    // Given the operator's working copy shared read-only alongside a writable output dir
    let shares = vec![
        NinePShare {
            host_path: "/home/dev/tddy-coder".to_string(),
            mount_tag: "tddy-src".to_string(),
            writable: false,
        },
        NinePShare {
            host_path: "/home/dev/out".to_string(),
            mount_tag: "tddy-out".to_string(),
            writable: true,
        },
    ];

    // When the 9p args are built
    let args = QemuVmArgs::nine_p_args(&shares);

    // Then each share carries a distinct fsdev id, matched by the device that mounts it
    assert_eq!(
        args,
        vec![
            "-fsdev",
            "local,id=fsdev0,path=/home/dev/tddy-coder,security_model=none,readonly=on",
            "-device",
            "virtio-9p-pci,fsdev=fsdev0,mount_tag=tddy-src",
            "-fsdev",
            "local,id=fsdev1,path=/home/dev/out,security_model=none",
            "-device",
            "virtio-9p-pci,fsdev=fsdev1,mount_tag=tddy-out",
        ]
    );
}

#[test]
fn always_forwards_the_ssh_port_ahead_of_the_extra_port_forwards() {
    // Given a guest that also forwards an application port
    let config = VmConfig {
        extra_hostfwd: vec![PortForward {
            host_port: 8080,
            guest_port: 80,
        }],
        ..an_aarch64_config()
    };

    // When the argv is built
    let argv = QemuVmArgs::build(&config);

    // Then SSH comes first, so the caller can always reach the guest
    assert_eq!(
        value_after(&argv, "-netdev"),
        "user,id=net0,hostfwd=tcp::2222-:22,hostfwd=tcp::8080-:80"
    );
}

#[test]
fn keeps_the_console_stderr_log_beside_the_guests_own_disk_image() {
    // Given a guest whose image lives in the VM's own directory
    let config = an_aarch64_config();

    // When the emulator's stderr log is placed
    let log = QemuVmArgs::console_stderr_log_path(&config.qcow2_path);

    // Then it lands beside the image, not in a world-writable directory where a
    // pre-created symlink would choose the file the daemon truncates — and reads back
    assert_eq!(log, Path::new("/images/guest.console-stderr.log"));
}

#[test]
fn connects_as_the_user_the_login_policy_names_rather_than_root() {
    // Given a guest whose login policy names the `tddy` user
    let vm = a_running_vm();

    // When the SSH destination is built
    let destination = ssh_destination(&vm);

    // Then that user is who SSH logs in as — a cloud image has no root login at all
    assert_eq!(destination, "tddy@127.0.0.1");
}

#[test]
fn offers_only_the_vms_own_key_when_the_login_policy_carries_one() {
    // Given a guest with a per-VM private key
    let vm = a_running_vm();

    // When the SSH options are built
    let opts = ssh_opts(&vm);

    // Then the key is offered with `IdentitiesOnly`, so the ambient agent's keys cannot be
    // tried first and exhaust the guest's MaxAuthTries before this one is reached
    assert_eq!(
        opts.join(" "),
        "-p 2222 \
         -o StrictHostKeyChecking=no \
         -o UserKnownHostsFile=/dev/null \
         -o BatchMode=yes \
         -o ConnectTimeout=10 \
         -o LogLevel=ERROR \
         -i /vms/guest/id_guest \
         -o IdentitiesOnly=yes"
    );
}

#[test]
fn offers_no_identity_when_the_login_policy_carries_no_key() {
    // Given a guest reached with whatever the ambient agent offers
    let vm = RunningVm {
        login: VmLogin {
            username: "tddy".to_string(),
            private_key_path: None,
        },
        ..a_running_vm()
    };

    // When the SSH options are built
    let opts = ssh_opts(&vm);

    // Then neither `-i` nor `IdentitiesOnly` appears, leaving the agent free to answer
    assert_eq!(
        opts.join(" "),
        "-p 2222 \
         -o StrictHostKeyChecking=no \
         -o UserKnownHostsFile=/dev/null \
         -o BatchMode=yes \
         -o ConnectTimeout=10 \
         -o LogLevel=ERROR"
    );
}

#[test]
fn creates_the_uefi_variables_store_at_the_exact_size_qemu_pflash_requires() {
    // Given a VM directory with no variables store in it yet
    let vm_dir = tempfile::tempdir().expect("a scratch VM directory");
    let vars = vm_dir.path().join("guest-vars.fd");

    // When the store is ensured
    ensure_uefi_vars_file(&vars).expect("the UEFI variables store must be creatable");

    // Then it is exactly 64 MiB — QEMU refuses a pflash unit of any other size
    let size = std::fs::metadata(&vars)
        .expect("the UEFI variables store must exist")
        .len();
    assert_eq!(size, 67_108_864);
}

#[test]
fn leaves_an_existing_uefi_variables_store_untouched() {
    // Given a store an earlier boot already wrote its boot entries into
    let vm_dir = tempfile::tempdir().expect("a scratch VM directory");
    let vars = vm_dir.path().join("guest-vars.fd");
    std::fs::write(&vars, "Boot0001: debian").expect("the UEFI variables store must be writable");

    // When the same VM is started again
    ensure_uefi_vars_file(&vars).expect("the UEFI variables store must be ensurable");

    // Then the entries survive the restart, rather than being zeroed back to 64 MiB
    let contents =
        std::fs::read_to_string(&vars).expect("the UEFI variables store must be readable");
    assert_eq!(contents, "Boot0001: debian");
}
