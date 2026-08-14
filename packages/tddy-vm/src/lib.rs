pub mod build;
pub mod cloud_init;
pub mod image_import;
pub mod library;
pub mod mock;
pub mod qemu;
pub mod registry;
pub mod serial_shell;
pub mod service;
pub mod tddy_host;
pub mod vm;
pub mod vm_manifest;

pub use build::{build_image, ImageFormat, VmImageRecord};
pub use cloud_init::{
    build_cloud_init_image, classify_serial_line, cloud_init_boot_argv, cloud_init_library_paths,
    completion_token, iso_tool_command, overlay_create_argv, relative_backing_path,
    render_meta_data, render_user_data, seed_iso_argv, CloudInitBootConfig, CloudInitBuildOptions,
    CloudInitLibraryPaths, CloudInitOutcome, CloudInitUser, CloudInitUserData, CloudInitWriteFile,
    IsoTool, NinePShare,
};
pub use image_import::{
    normalise_to_qcow2, refuse_chain_flattening, supplied_image_format, SuppliedImageFormat,
};
pub use library::{
    generate_vm_ssh_keypair, set_readonly_file, vm_overlay_create_argv, VmLibrary, VmSshKeypair,
};
pub use mock::MockVm;
pub use qemu::{
    ensure_uefi_vars_file, qemu_binary, resolve_uefi_code_path, scp_to_guest, scp_to_guest_argv,
    send_monitor_command, ssh_destination, ssh_opts, wait_for_ssh_port, BootedWithConsole, QemuVm,
    QemuVmArgs,
};
pub use registry::{VmManager, VmSpec, VmState};
pub use service::{SessionUserResolver, VmServiceImpl};
pub use vm::{
    ForwardHandle, PortForward, RunningVm, UefiFirmware, VerifyResult, Vm, VmAccel, VmArch,
    VmConfig, VmError, VmLogin,
};
pub use vm_manifest::{LoginPolicy, RunPolicy, VmManifest};
