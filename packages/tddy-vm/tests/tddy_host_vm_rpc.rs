//! RPC-surface tests for the two methods the daemon-spawned tddy host VM feature adds to
//! `vm.VmService`.
//!
//! These drive the methods through `RpcBridge` exactly as the existing
//! `vm_service_acceptance.rs` does, so they exercise the generated dispatch table
//! (method-name routing, status mapping) without a network or a VM. They pin the wire
//! contract and the auth boundary; the feature itself is proven by the real-boot tests in
//! `packages/tddy-vm/tests/tddy_host_vm_acceptance.rs`.

use prost::Message;
use std::sync::Arc;
use tddy_rpc::{Code, RequestMetadata, ResponseBody, RpcBridge, RpcMessage, Status};
use tddy_service::proto::vm::{
    build_tddy_host_image_progress, BuildTddyHostImageProgress, BuildTddyHostImageRequest,
    CreateVmFromPreparedBaseRequest, CreateVmFromPreparedBaseResponse, TddyHostLiveKit,
    VmRunPolicyProto, VmServiceServer,
};
use tddy_task::TaskRegistry;
use tddy_vm::library::VmLibrary;
use tddy_vm::service::{SessionUserResolver, VmServiceImpl};
use tddy_vm::{MockVm, VmManager};
use tempfile::{tempdir, TempDir};

const GOOD_TOKEN: &str = "valid-token";
const BAD_TOKEN: &str = "bogus-token";

fn test_resolver() -> SessionUserResolver {
    Arc::new(|token: &str| {
        if token == GOOD_TOKEN {
            Some("testuser".to_string())
        } else {
            None
        }
    })
}

/// A library-backed VM service over a temp root, with a recording mock QEMU backend.
fn a_vm_service() -> (TempDir, RpcBridge<VmServiceServer<VmServiceImpl>>) {
    let dir = tempdir().unwrap();
    let library = VmLibrary::new(dir.path());
    library.init().expect("library must initialise");
    let manager = Arc::new(VmManager::from_library(library, Box::new(MockVm::new())));
    let svc = VmServiceImpl::new(manager, test_resolver(), TaskRegistry::new());
    (dir, RpcBridge::new(VmServiceServer::new(svc)))
}

fn a_run_policy() -> VmRunPolicyProto {
    VmRunPolicyProto {
        memory: "2048M".to_string(),
        cpus: 2,
        disk_size: "40G".to_string(),
        ssh_host_port: 2222,
        port_forwards: vec![],
        arch: "aarch64".to_string(),
        accel: "hvf".to_string(),
    }
}

/// A build request whose `base_image_path` names a file inside this test's own temp dir
/// that was never created — so "the base image does not exist" is guaranteed by the test,
/// not by whatever the host happens to have under `/images`.
fn a_build_request(dir: &TempDir) -> BuildTddyHostImageRequest {
    let absent_base_image = dir.path().join("debian-12-genericcloud.qcow2");
    assert!(
        !absent_base_image.exists(),
        "the base image must not exist for this request to fail fast"
    );
    BuildTddyHostImageRequest {
        session_token: GOOD_TOKEN.to_string(),
        name: "debian-12-tddy".to_string(),
        base_image_name: "debian-12".to_string(),
        base_image_path: absent_base_image.display().to_string(),
        source_dir: dir.path().display().to_string(),
        livekit: Some(TddyHostLiveKit {
            url: "wss://livekit.example.com".to_string(),
            api_key: "devkey".to_string(),
            api_secret: "devsecret".to_string(),
            common_room: "tddy-common".to_string(),
        }),
        run: Some(a_run_policy()),
    }
}

fn a_create_request() -> CreateVmFromPreparedBaseRequest {
    CreateVmFromPreparedBaseRequest {
        session_token: GOOD_TOKEN.to_string(),
        name: "tddy-host-1".to_string(),
        prepared_base: "debian-12-tddy".to_string(),
        run: Some(a_run_policy()),
        ssh_username: "tddy".to_string(),
    }
}

/// Stand in for a completed bake: a real, empty qcow2 in the library's prepared-base
/// directory, so the per-VM overlay `qemu-img` chains onto it has a valid backing file.
fn a_prepared_base(dir: &TempDir, name: &str) {
    let path = dir
        .path()
        .join("images/02-prepared-base")
        .join(format!("{name}.qcow2"));
    let output = std::process::Command::new("qemu-img")
        .args(["create", "-f", "qcow2"])
        .arg(&path)
        .arg("64M")
        .output()
        .expect("qemu-img must be runnable to place a prepared base");
    assert!(
        output.status.success(),
        "qemu-img create failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

async fn call<Req: Message, Resp: Message + Default>(
    bridge: &RpcBridge<VmServiceServer<VmServiceImpl>>,
    method: &str,
    req: Req,
) -> Resp {
    let msg = RpcMessage {
        payload: req.encode_to_vec(),
        metadata: RequestMetadata::default(),
    };
    let result = bridge
        .handle_messages("vm.VmService", method, &[msg])
        .await
        .expect("bridge dispatch must not fail at transport level");
    let chunks = match result {
        ResponseBody::Complete(c) => c,
        _ => panic!("expected Complete for unary method {method}"),
    };
    assert_eq!(chunks.len(), 1, "unary method {method} must return 1 chunk");
    Resp::decode(&chunks[0][..]).expect("decode response")
}

async fn call_stream<Req: Message, Resp: Message + Default>(
    bridge: &RpcBridge<VmServiceServer<VmServiceImpl>>,
    method: &str,
    req: Req,
) -> Vec<Resp> {
    let msg = RpcMessage {
        payload: req.encode_to_vec(),
        metadata: RequestMetadata::default(),
    };
    let result = bridge
        .handle_messages("vm.VmService", method, &[msg])
        .await
        .expect("bridge dispatch must not fail at transport level");
    let mut rx = match result {
        ResponseBody::Streaming(rx) => rx,
        _ => panic!("expected Streaming for server-streaming method {method}"),
    };
    let mut messages = Vec::new();
    while let Some(chunk) = rx.recv().await {
        let bytes = chunk.expect("stream chunk must not be an error");
        messages.push(Resp::decode(&bytes[..]).expect("decode stream message"));
    }
    messages
}

/// The type and comment of an OpenSSH public key line — the two parts of it that are fixed,
/// with the per-VM key material between them left out.
fn key_type_and_comment(public_key: &str) -> (&str, &str) {
    let words: Vec<&str> = public_key.split_whitespace().collect();
    assert_eq!(
        words.len(),
        3,
        "expected a 'type material comment' OpenSSH public key line, got: {public_key}"
    );
    (words[0], words[2])
}

/// The `Status` the bridge refused this request with, for tests about rejected arguments.
async fn rejection_of<Req: Message>(
    bridge: &RpcBridge<VmServiceServer<VmServiceImpl>>,
    method: &str,
    req: Req,
) -> Status {
    let msg = RpcMessage {
        payload: req.encode_to_vec(),
        metadata: RequestMetadata::default(),
    };
    bridge
        .handle_messages("vm.VmService", method, &[msg])
        .await
        .err()
        .unwrap_or_else(|| panic!("expected {method} to refuse the request"))
}

async fn assert_unauthenticated(
    bridge: &RpcBridge<VmServiceServer<VmServiceImpl>>,
    method: &str,
    payload: Vec<u8>,
) {
    let msg = RpcMessage {
        payload,
        metadata: RequestMetadata::default(),
    };
    let result = bridge.handle_messages("vm.VmService", method, &[msg]).await;
    match result {
        Err(status) => assert_eq!(
            status.code,
            Code::Unauthenticated,
            "expected Unauthenticated for method {method}, got {:?}",
            status.code
        ),
        Ok(_) => panic!("expected Unauthenticated error for method {method} with bad token"),
    }
}

#[tokio::test]
async fn build_tddy_host_image_with_invalid_token_returns_unauthenticated() {
    // Given — a VM service and a build request carrying a bogus token
    let (dir, bridge) = a_vm_service();
    let req = BuildTddyHostImageRequest {
        session_token: BAD_TOKEN.to_string(),
        ..a_build_request(&dir)
    };

    // When / Then — the bake is refused before any work starts
    assert_unauthenticated(&bridge, "BuildTddyHostImage", req.encode_to_vec()).await;
}

#[tokio::test]
async fn create_vm_from_prepared_base_with_invalid_token_returns_unauthenticated() {
    // Given — a VM service and a create request carrying a bogus token
    let (_dir, bridge) = a_vm_service();
    let req = CreateVmFromPreparedBaseRequest {
        session_token: BAD_TOKEN.to_string(),
        name: "tddy-host-1".to_string(),
        prepared_base: "debian-12-tddy".to_string(),
        run: Some(a_run_policy()),
        ssh_username: "tddy".to_string(),
    };

    // When / Then
    assert_unauthenticated(&bridge, "CreateVmFromPreparedBase", req.encode_to_vec()).await;
}

#[tokio::test]
async fn build_tddy_host_image_streams_progress_and_ends_in_a_terminal_stage() {
    // Given — a VM service and a build request naming a base image that does not exist, so
    // the bake fails fast instead of booting a real guest
    let (dir, bridge) = a_vm_service();

    // When — the streaming method is dispatched
    let messages: Vec<BuildTddyHostImageProgress> =
        call_stream(&bridge, "BuildTddyHostImage", a_build_request(&dir)).await;

    // Then — it is a server stream whose final message reports a terminal stage
    let last = messages
        .last()
        .expect("stream must emit at least one message");
    assert_eq!(
        last.stage,
        build_tddy_host_image_progress::Stage::Error as i32,
        "a missing base image must end the stream in STAGE_ERROR, not silently succeed"
    );
}

#[tokio::test]
async fn create_vm_from_prepared_base_returns_the_overlay_it_made_and_the_key_it_generated() {
    // Given — a VM service whose library holds the "debian-12-tddy" prepared base
    let (dir, bridge) = a_vm_service();
    a_prepared_base(&dir, "debian-12-tddy");

    // When — a VM is created from it
    let resp: CreateVmFromPreparedBaseResponse =
        call(&bridge, "CreateVmFromPreparedBase", a_create_request()).await;

    // Then — the caller gets the VM's own overlay and the public key it must authorize
    assert!(resp.ok, "creation must succeed, got: {}", resp.message);
    assert_eq!(
        resp.overlay_path,
        dir.path()
            .join("vm/tddy-host-1/tddy-host-1.qcow2")
            .display()
            .to_string()
    );
    // The key material is minted per VM, so only its type and comment are fixed.
    assert_eq!(
        key_type_and_comment(&resp.ssh_public_key),
        ("ssh-ed25519", "tddy-vm-tddy-host-1")
    );
}

#[tokio::test]
async fn create_vm_from_prepared_base_rejects_an_unknown_prepared_base() {
    // Given — a VM service whose library holds no prepared bases
    let (_dir, bridge) = a_vm_service();

    // When — a VM is requested from a base that was never baked
    let resp: CreateVmFromPreparedBaseResponse = call(
        &bridge,
        "CreateVmFromPreparedBase",
        CreateVmFromPreparedBaseRequest {
            prepared_base: "never-baked".to_string(),
            ..a_create_request()
        },
    )
    .await;

    // Then — it is refused with an explanatory message rather than creating a broken overlay
    assert!(!resp.ok, "unknown prepared base must not report success");
    assert!(
        resp.message.contains("never-baked"),
        "message must name the missing prepared base, got: {}",
        resp.message
    );
}

#[tokio::test]
async fn create_vm_from_prepared_base_requires_the_account_baked_into_the_base() {
    // Given — a create request naming no account to log in as
    let (dir, bridge) = a_vm_service();
    a_prepared_base(&dir, "debian-12-tddy");
    let req = CreateVmFromPreparedBaseRequest {
        ssh_username: String::new(),
        ..a_create_request()
    };

    // When
    let rejection = rejection_of(&bridge, "CreateVmFromPreparedBase", req).await;

    // Then
    assert_eq!(rejection.code, Code::InvalidArgument);
    assert_eq!(
        rejection.message,
        "ssh_username is required — it must name the account baked into the prepared base"
    );
}

#[tokio::test]
async fn create_vm_from_prepared_base_requires_a_run_policy() {
    // Given — a create request carrying no resources, arch or accelerator
    let (dir, bridge) = a_vm_service();
    a_prepared_base(&dir, "debian-12-tddy");
    let req = CreateVmFromPreparedBaseRequest {
        run: None,
        ..a_create_request()
    };

    // When
    let rejection = rejection_of(&bridge, "CreateVmFromPreparedBase", req).await;

    // Then
    assert_eq!(rejection.code, Code::InvalidArgument);
    assert_eq!(rejection.message, "run is required");
}

#[tokio::test]
async fn create_vm_from_prepared_base_rejects_a_vm_name_that_would_escape_the_library_root() {
    // Given — a create request whose name climbs out of `<root>/vm/`
    let (dir, bridge) = a_vm_service();
    a_prepared_base(&dir, "debian-12-tddy");
    let req = CreateVmFromPreparedBaseRequest {
        name: "../../escaped".to_string(),
        ..a_create_request()
    };

    // When
    let rejection = rejection_of(&bridge, "CreateVmFromPreparedBase", req).await;

    // Then — the request is refused before any directory is created, naming the bad field
    assert_eq!(rejection.code, Code::InvalidArgument);
    assert_eq!(
        rejection.message,
        "name '../../escaped' must be a plain name: '/', '\\' and '..' would place it \
         outside the VM & Image Library"
    );
}

#[tokio::test]
async fn build_tddy_host_image_rejects_a_base_image_name_that_would_escape_the_library_root() {
    // Given — a build request whose imported-base name climbs out of `<root>/images/01-base/`
    let (dir, bridge) = a_vm_service();
    let req = BuildTddyHostImageRequest {
        base_image_name: "../../../etc/passwd".to_string(),
        ..a_build_request(&dir)
    };

    // When
    let rejection = rejection_of(&bridge, "BuildTddyHostImage", req).await;

    // Then — the hours-long bake never starts
    assert_eq!(rejection.code, Code::InvalidArgument);
    assert_eq!(
        rejection.message,
        "base_image_name '../../../etc/passwd' must be a plain name: '/', '\\' and '..' would \
         place it outside the VM & Image Library"
    );
}
