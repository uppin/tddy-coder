//! Unit tests for the per-VM SSH keypair the library must generate.
//!
//! `VmLibrary::create_vm`'s documentation already promises "writes the manifest + SSH keys",
//! but no key is generated today and `LoginPolicy::ssh_private_key` is never populated —
//! which is why `QemuVm` falls back to `root@` with the ambient agent key.

use std::os::unix::fs::PermissionsExt;

use pretty_assertions::assert_eq;
use tddy_vm::library::generate_vm_ssh_keypair;
use tempfile::tempdir;

#[test]
fn writes_the_keypair_under_the_conventional_per_vm_filenames() {
    // Given a per-VM directory
    let dir = tempdir().unwrap();

    // When a keypair is generated for the VM
    let keys = generate_vm_ssh_keypair(dir.path(), "web").expect("keypair must be generatable");

    // Then it follows the library's documented `id_<name>` / `id_<name>.pub` layout
    assert_eq!(keys.private_key_path, dir.path().join("id_web"));
    assert_eq!(keys.public_key_path, dir.path().join("id_web.pub"));
}

#[test]
fn locks_the_private_key_to_owner_read_write_only() {
    // Given a generated keypair
    let dir = tempdir().unwrap();
    let keys = generate_vm_ssh_keypair(dir.path(), "web").expect("keypair must be generatable");

    // When the private key's mode is inspected
    let mode = std::fs::metadata(&keys.private_key_path)
        .unwrap()
        .permissions()
        .mode()
        & 0o777;

    // Then it is 0600 — OpenSSH refuses to use a more permissive key
    assert_eq!(mode, 0o600);
}

#[test]
fn generates_a_public_key_openssh_recognises() {
    // Given a generated keypair
    let dir = tempdir().unwrap();
    let keys = generate_vm_ssh_keypair(dir.path(), "web").expect("keypair must be generatable");

    // When the public key is read
    let public = std::fs::read_to_string(&keys.public_key_path).unwrap();

    // Then it is an ed25519 key in authorized_keys form
    assert!(
        public.starts_with("ssh-ed25519 "),
        "public key must be ed25519 in authorized_keys form: {public}"
    );
}

#[test]
fn generates_a_distinct_keypair_for_each_vm() {
    // Given two VMs
    let dir = tempdir().unwrap();
    let first_dir = dir.path().join("one");
    let second_dir = dir.path().join("two");
    std::fs::create_dir_all(&first_dir).unwrap();
    std::fs::create_dir_all(&second_dir).unwrap();

    // When each gets a keypair
    let first = generate_vm_ssh_keypair(&first_dir, "one").expect("keypair must be generatable");
    let second = generate_vm_ssh_keypair(&second_dir, "two").expect("keypair must be generatable");

    // Then the keys differ — one VM's key must not open another
    let first_public = std::fs::read_to_string(&first.public_key_path).unwrap();
    let second_public = std::fs::read_to_string(&second.public_key_path).unwrap();
    assert_ne!(first_public, second_public);
}
