//! Unit tests for the tddy-host cloud-init recipe (`tddy_vm::tddy_host`).
//!
//! These pin *what the guest is told to do* — the provisioning contract — without booting
//! anything. Whether the recipe actually succeeds in a guest is proven by
//! `tddy_host_vm_acceptance.rs`.

use pretty_assertions::assert_eq;
use tddy_vm::tddy_host::{
    daemon_config_yaml, ninep_capable_kernel_command, tddy_host_user_data, LiveKitCommonRoom,
    TddyHostSpec, GUEST_CHECKOUT_DIR, GUEST_SOURCE_MOUNT, TDDY_SOURCE_MOUNT_TAG,
};

fn a_tddy_host_spec() -> TddyHostSpec {
    TddyHostSpec {
        hostname: "tddy-host".to_string(),
        username: "tddy".to_string(),
        livekit: Some(LiveKitCommonRoom {
            url: "wss://livekit.example.com".to_string(),
            api_key: "devkey".to_string(),
            api_secret: "devsecret".to_string(),
            common_room: "tddy-common".to_string(),
        }),
    }
}

/// The single `runcmd` entry containing `needle`, for asserting on one provisioning step.
fn runcmd_containing(spec: &TddyHostSpec, needle: &str) -> String {
    let user_data = tddy_host_user_data(spec);
    let matches: Vec<String> = user_data
        .runcmd
        .iter()
        .filter(|c| c.contains(needle))
        .cloned()
        .collect();
    assert_eq!(
        matches.len(),
        1,
        "expected exactly one runcmd containing {needle:?}, found {matches:?}"
    );
    matches.into_iter().next().unwrap()
}

/// Index of the single `runcmd` entry containing `needle`.
fn runcmd_index(spec: &TddyHostSpec, needle: &str) -> usize {
    let user_data = tddy_host_user_data(spec);
    user_data
        .runcmd
        .iter()
        .position(|c| c.contains(needle))
        .unwrap_or_else(|| panic!("no runcmd contains {needle:?}: {:?}", user_data.runcmd))
}

#[test]
fn mounts_the_source_share_read_only_at_the_reserved_guest_path() {
    // Given a tddy host spec
    let spec = a_tddy_host_spec();

    // When the mount step is rendered
    let mount = runcmd_containing(&spec, "mount -t 9p");

    // Then the reserved tag is mounted read-only where the copy step expects it
    assert!(
        mount.contains(TDDY_SOURCE_MOUNT_TAG),
        "mount must use the reserved 9p tag: {mount}"
    );
    assert!(
        mount.contains(GUEST_SOURCE_MOUNT),
        "mount must target the reserved guest path: {mount}"
    );
    assert!(
        mount.contains("ro"),
        "the operator's working copy must be mounted read-only: {mount}"
    );
}

#[test]
fn copies_the_shared_working_copy_into_the_guest_before_building() {
    // Given a tddy host spec
    let spec = a_tddy_host_spec();

    // When the copy step is rendered
    let copy = runcmd_containing(&spec, GUEST_CHECKOUT_DIR);

    // Then it copies out of the read-only share into a writable checkout
    assert!(
        copy.contains(GUEST_SOURCE_MOUNT),
        "the copy must read from the 9p mount: {copy}"
    );
}

#[test]
fn installs_nix_before_running_the_repository_build_scripts() {
    // Given a tddy host spec
    let spec = a_tddy_host_spec();

    // When the ordering of the Nix install and the release build is compared
    let nix = runcmd_index(&spec, "nixos.org/nix/install");
    let release = runcmd_index(&spec, "./release");

    // Then Nix lands first — ./release and ./install are defined in terms of the dev shell
    assert!(
        nix < release,
        "Nix must be installed before ./release runs (nix at {nix}, release at {release})"
    );
}

#[test]
fn gives_the_guest_a_nine_p_capable_kernel_before_mounting_the_share() {
    // Given a tddy host spec
    let spec = a_tddy_host_spec();

    // When the ordering of the kernel step and the mount is compared
    let kernel = runcmd_index(&spec, "linux-image-");
    let mount = runcmd_index(&spec, "mount -t 9p");

    // Then the kernel is sorted out first — Debian's cloud kernel flavour has no 9p modules
    // at all, so mounting the share before this step fails with `unknown filesystem type`
    assert!(
        kernel < mount,
        "the 9p-capable kernel must be in place before the share is mounted \
         (kernel at {kernel}, mount at {mount})"
    );
}

#[test]
fn purges_the_cloud_kernel_so_grub_boots_the_nine_p_capable_one() {
    // Given the kernel-preparation step
    let command = ninep_capable_kernel_command();

    // Then it both installs the generic flavour and removes the cloud one — installing
    // alone leaves GRUB still booting the cloud kernel, which has no 9p support
    assert!(
        command.contains("apt-get install -y -qq linux-image-"),
        "must install the generic kernel: {command}"
    );
    assert!(
        command.contains("purge") && command.contains("-cloud"),
        "must purge the cloud kernel flavour: {command}"
    );
    assert!(
        command.contains("update-grub"),
        "must refresh the boot menu after changing kernels: {command}"
    );
}

#[test]
fn skips_the_kernel_step_when_a_non_cloud_kernel_is_already_running() {
    // Given the kernel-preparation step
    let command = ninep_capable_kernel_command();

    // Then it is guarded on the running kernel, so the reboot happens exactly once rather
    // than on every cloud-init pass
    assert!(
        command.contains("uname -r | grep -q -- '-cloud'"),
        "must be guarded on the running kernel flavour: {command}"
    );
}

/// A failed `apt-get` must abort the whole provisioning script. cloud-init joins `runcmd`
/// into one shell script, so an unconditional `exit 0` here would skip every later step —
/// mount, copy, Nix, `./release`, `./install` — while cloud-init recorded no error at all.
/// The host would then seal and promote a prepared base with no tddy in it and report a
/// successful bake.
#[test]
fn fails_the_provisioning_script_when_the_kernel_install_fails() {
    // Given the kernel-preparation step
    let command = ninep_capable_kernel_command();

    // Then it never exits 0 unconditionally, and carries the install chain's status out
    assert!(
        !command.contains("exit 0"),
        "a hardcoded exit 0 discards the install chain's status: {command}"
    );
    assert!(
        command.contains("kernel_status=$?"),
        "the install chain's status must be captured: {command}"
    );
    assert!(
        command.contains("if [ \"$kernel_status\" -ne 0 ]; then exit \"$kernel_status\"; fi"),
        "a failed install must abort the script with its own status: {command}"
    );
}

/// cloud-init runs `runcmd` as one shell script and adds no error handling of its own, so
/// without this a failed step only skips the rest of its own `&&` chain. A failed `rsync`
/// would skip just the `cd` into the checkout and leave every later build step running in
/// `/`, where `./release` does not exist — and the script would still exit 0.
#[test]
fn aborts_the_provisioning_script_at_the_first_failing_step() {
    // Given a tddy host spec
    let spec = a_tddy_host_spec();

    // When the user-data is rendered
    let user_data = tddy_host_user_data(&spec);

    // Then the script begins by turning on errexit
    assert_eq!(user_data.runcmd.first().map(String::as_str), Some("set -e"));
}

#[test]
fn mounts_the_source_share_before_copying_it() {
    // Given a tddy host spec
    let spec = a_tddy_host_spec();

    // When the ordering of mount and copy is compared
    let mount = runcmd_index(&spec, "mount -t 9p");
    let copy = runcmd_index(&spec, GUEST_CHECKOUT_DIR);

    // Then the share is mounted first
    assert!(
        mount < copy,
        "the 9p share must be mounted before it is copied (mount at {mount}, copy at {copy})"
    );
}

#[test]
fn builds_the_web_bundle_before_installing_because_install_copies_it() {
    // Given a tddy host spec
    let spec = a_tddy_host_spec();

    // When the ordering of the web build and the install is compared
    let web = runcmd_index(&spec, "bun run build");
    let install = runcmd_index(&spec, "./install --systemd");

    // Then the bundle exists before ./install copies packages/tddy-web/dist
    assert!(
        web < install,
        "the web bundle must be built before ./install copies it (web at {web}, install at {install})"
    );
}

#[test]
fn installs_the_daemon_as_a_systemd_service() {
    // Given a tddy host spec
    let spec = a_tddy_host_spec();

    // When the install step is rendered
    let install = runcmd_containing(&spec, "./install");

    // Then it requests the systemd installation the repo's script implements
    assert!(
        install.contains("--systemd"),
        "install must run in systemd mode: {install}"
    );
}

#[test]
fn writes_the_daemon_config_as_a_file_rather_than_a_command() {
    // Given a tddy host spec
    let spec = a_tddy_host_spec();

    // When the user-data is rendered
    let user_data = tddy_host_user_data(&spec);

    // Then the config is a write_files entry — cloud-init runs those before runcmd, and
    // ./install keeps an existing config rather than overwriting it
    let configs: Vec<&str> = user_data
        .write_files
        .iter()
        .map(|w| w.path.as_str())
        .filter(|p| p.ends_with("daemon.yaml"))
        .collect();
    assert_eq!(configs, vec!["/etc/tddy/daemon.yaml"]);
}

#[test]
fn carries_the_livekit_common_room_settings_into_the_guest_daemon_config() {
    // Given a spec naming a LiveKit common room
    let spec = a_tddy_host_spec();

    // When the daemon config is rendered
    let yaml = daemon_config_yaml(&spec);

    // Then the guest daemon can announce itself on that room
    assert!(yaml.contains("wss://livekit.example.com"), "config: {yaml}");
    assert!(yaml.contains("tddy-common"), "config: {yaml}");
    assert!(yaml.contains("devkey"), "config: {yaml}");
    assert!(yaml.contains("devsecret"), "config: {yaml}");
}

/// Every key here has to match a field of `tddy_daemon::config::DaemonConfig`, which is
/// `deny_unknown_fields`: a renamed or misspelled key makes the guest daemon refuse to start,
/// and that only surfaces after an hours-long bake. Asserting the whole document — rather
/// than a handful of `contains` — is what makes a rename fail here instead of there.
#[test]
fn renders_the_exact_daemon_config_document_the_guest_daemon_deserializes() {
    // Given a tddy host spec
    let spec = a_tddy_host_spec();

    // When the daemon config is rendered
    let yaml = daemon_config_yaml(&spec);

    // Then it is exactly this document
    assert_eq!(
        yaml,
        "# tddy-daemon config baked into this VM by tddy-vm.\n\
         listen:\n\
         \x20 web_port: 8080\n\
         \x20 web_host: '0.0.0.0'\n\
         web_bundle_path: /usr/local/share/tddy/web\n\
         daemon_instance_id: tddy-host\n\
         livekit:\n\
         \x20 url: wss://livekit.example.com\n\
         \x20 api_key: devkey\n\
         \x20 api_secret: devsecret\n\
         \x20 common_room: tddy-common\n\
         allowed_tools:\n\
         - path: /usr/local/bin/tddy-coder\n\
         \x20 label: tddy-coder\n\
         - path: /usr/local/bin/tddy-tools\n\
         \x20 label: tddy-tools\n"
    );
}

#[test]
fn keeps_the_baked_livekit_secret_off_a_world_readable_path_in_the_guest() {
    // Given a tddy host spec whose daemon config carries a live LiveKit api_secret
    let spec = a_tddy_host_spec();

    // When the user-data is rendered
    let user_data = tddy_host_user_data(&spec);

    // Then the config is written 0640 owned by root and the daemon's own group, so no other
    // account in the guest can read the secret. `defer` is what makes that owner possible:
    // cloud-init applies write_files before it creates users, and chowning to a group that
    // does not exist yet fails the module.
    let config = user_data
        .write_files
        .iter()
        .find(|w| w.path == "/etc/tddy/daemon.yaml")
        .expect("the daemon config must be a write_files entry");
    assert_eq!(config.permissions.as_deref(), Some("0640"));
    assert_eq!(config.owner.as_deref(), Some("root:tddy"));
    assert_eq!(config.defer, Some(true));
}

#[test]
fn omits_the_livekit_section_when_no_common_room_is_configured() {
    // Given a spec with no LiveKit configuration
    let spec = TddyHostSpec {
        livekit: None,
        ..a_tddy_host_spec()
    };

    // When the daemon config is rendered
    let yaml = daemon_config_yaml(&spec);

    // Then no empty or placeholder LiveKit block is emitted
    assert!(
        !yaml.contains("livekit"),
        "a daemon with no common room must not carry a livekit section: {yaml}"
    );
}

#[test]
fn authorizes_the_generated_key_for_the_policy_user() {
    // Given a tddy host spec
    let spec = a_tddy_host_spec();

    // When the user-data is rendered
    let user_data = tddy_host_user_data(&spec);

    // Then the single provisioned user carries the substitution placeholder that
    // render_user_data replaces with the per-VM public key
    assert_eq!(user_data.users.len(), 1);
    assert_eq!(user_data.users[0].name, "tddy");
    assert_eq!(
        user_data.users[0].ssh_authorized_keys,
        vec!["{{SSH_PUBLIC_KEY}}".to_string()]
    );
}

#[test]
fn installs_the_host_packages_the_build_needs_before_nix_is_available() {
    // Given a tddy host spec
    let spec = a_tddy_host_spec();

    // When the user-data is rendered
    let user_data = tddy_host_user_data(&spec);

    // Then curl and git are present — the Nix installer and the build both need them, and
    // neither can come from Nix itself
    assert!(
        user_data.packages.contains(&"curl".to_string()),
        "packages: {:?}",
        user_data.packages
    );
    assert!(
        user_data.packages.contains(&"git".to_string()),
        "packages: {:?}",
        user_data.packages
    );
}
