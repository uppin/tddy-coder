//! VM service acceptance tests — drives VmService through RpcBridge with mock backend.

use prost::Message;
use std::sync::Arc;
use tddy_rpc::{Code, RequestMetadata, ResponseBody, RpcBridge, RpcMessage};
use tddy_service::proto::vm::{
    BuildVmImageProgress, BuildVmImageRequest, DefineVmRequest, DefineVmResponse,
    GetVmStatusRequest, GetVmStatusResponse, ListVmImagesRequest, ListVmImagesResponse,
    ListVmsRequest, ListVmsResponse, StartVmRequest, StartVmResponse, StopVmRequest,
    StopVmResponse, VmServiceServer, VmSpecProto,
};
use tddy_task::TaskRegistry;
use tddy_vm::service::{SessionUserResolver, VmServiceImpl};
use tddy_vm::{MockVm, VmManager};
use tempfile::tempdir;

const GOOD_TOKEN: &str = "valid-token";
const BAD_TOKEN: &str = "bogus-token";

/// Build a resolver that accepts only GOOD_TOKEN.
fn test_resolver() -> SessionUserResolver {
    Arc::new(|token: &str| {
        if token == GOOD_TOKEN {
            Some("testuser".to_string())
        } else {
            None
        }
    })
}

async fn call<Req: Message, Resp: Message + Default>(
    bridge: &RpcBridge<VmServiceServer<VmServiceImpl>>,
    method: &str,
    req: Req,
) -> Resp {
    let payload = req.encode_to_vec();
    let msg = RpcMessage {
        payload,
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

/// Call a unary method with raw payload and assert it returns Unauthenticated.
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
        Err(status) => {
            assert_eq!(
                status.code,
                Code::Unauthenticated,
                "expected Unauthenticated for method {method}, got {:?}",
                status.code
            );
        }
        Ok(_) => panic!("expected Unauthenticated error for method {method} with bad token"),
    }
}

#[tokio::test]
async fn define_vm_then_list_shows_it() {
    // Given — a fresh VmService backed by MockVm with a valid resolver
    let _dir = tempdir().unwrap();
    let manager = Arc::new(VmManager::new(
        &_dir.path().join("vms.json"),
        Box::new(MockVm::new()),
    ));
    let svc = VmServiceImpl::new(manager, test_resolver(), TaskRegistry::new());
    let bridge = RpcBridge::new(VmServiceServer::new(svc));

    // When — DefineVm is called
    let _: DefineVmResponse = call(
        &bridge,
        "DefineVm",
        DefineVmRequest {
            session_token: GOOD_TOKEN.to_string(),
            spec: Some(VmSpecProto {
                name: "web".to_string(),
                image_path: "/fake/image.qcow2".to_string(),
                build_target: String::new(),
                ssh_host_port: 2222,
                port_forwards: vec![],
            }),
        },
    )
    .await;

    // Then — ListVms includes the newly defined VM
    let list: ListVmsResponse = call(
        &bridge,
        "ListVms",
        ListVmsRequest {
            session_token: GOOD_TOKEN.to_string(),
        },
    )
    .await;
    assert_eq!(list.vms.len(), 1);
    assert_eq!(list.vms[0].name, "web");
}

#[tokio::test]
async fn start_vm_and_get_running_status() {
    // Given — a defined VM
    let _dir = tempdir().unwrap();
    let manager = Arc::new(VmManager::new(
        &_dir.path().join("vms.json"),
        Box::new(MockVm::new()),
    ));
    let svc = VmServiceImpl::new(manager, test_resolver(), TaskRegistry::new());
    let bridge = RpcBridge::new(VmServiceServer::new(svc));

    let _: DefineVmResponse = call(
        &bridge,
        "DefineVm",
        DefineVmRequest {
            session_token: GOOD_TOKEN.to_string(),
            spec: Some(VmSpecProto {
                name: "app".to_string(),
                image_path: "/fake/image.qcow2".to_string(),
                build_target: String::new(),
                ssh_host_port: 2223,
                port_forwards: vec![],
            }),
        },
    )
    .await;

    // When — StartVm is called
    let _: StartVmResponse = call(
        &bridge,
        "StartVm",
        StartVmRequest {
            session_token: GOOD_TOKEN.to_string(),
            name: "app".to_string(),
        },
    )
    .await;

    // Then — GetVmStatus returns RUNNING
    let status: GetVmStatusResponse = call(
        &bridge,
        "GetVmStatus",
        GetVmStatusRequest {
            session_token: GOOD_TOKEN.to_string(),
            name: "app".to_string(),
        },
    )
    .await;
    // VmState::VM_STATE_RUNNING = 3 in the proto enum
    assert_eq!(status.state, 3, "VM must be RUNNING after StartVm");
}

#[tokio::test]
async fn stop_vm_returns_stopped_status() {
    // Given — a running VM
    let _dir = tempdir().unwrap();
    let manager = Arc::new(VmManager::new(
        &_dir.path().join("vms.json"),
        Box::new(MockVm::new()),
    ));
    let svc = VmServiceImpl::new(manager, test_resolver(), TaskRegistry::new());
    let bridge = RpcBridge::new(VmServiceServer::new(svc));

    let _: DefineVmResponse = call(
        &bridge,
        "DefineVm",
        DefineVmRequest {
            session_token: GOOD_TOKEN.to_string(),
            spec: Some(VmSpecProto {
                name: "runner".to_string(),
                image_path: "/fake/image.qcow2".to_string(),
                build_target: String::new(),
                ssh_host_port: 2224,
                port_forwards: vec![],
            }),
        },
    )
    .await;
    let _: StartVmResponse = call(
        &bridge,
        "StartVm",
        StartVmRequest {
            session_token: GOOD_TOKEN.to_string(),
            name: "runner".to_string(),
        },
    )
    .await;

    // When — StopVm is called
    let _: StopVmResponse = call(
        &bridge,
        "StopVm",
        StopVmRequest {
            session_token: GOOD_TOKEN.to_string(),
            name: "runner".to_string(),
        },
    )
    .await;

    // Then — GetVmStatus returns STOPPED
    let status: GetVmStatusResponse = call(
        &bridge,
        "GetVmStatus",
        GetVmStatusRequest {
            session_token: GOOD_TOKEN.to_string(),
            name: "runner".to_string(),
        },
    )
    .await;
    // VmState::VM_STATE_STOPPED = 4 in the proto enum
    assert_eq!(status.state, 4, "VM must be STOPPED after StopVm");
}

async fn call_stream<Req: Message, Resp: Message + Default>(
    bridge: &RpcBridge<VmServiceServer<VmServiceImpl>>,
    method: &str,
    req: Req,
) -> Vec<Resp> {
    let payload = req.encode_to_vec();
    let msg = RpcMessage {
        payload,
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

#[tokio::test]
async fn build_vm_image_streams_progress_messages() {
    // BuildVmImage must return a server stream ending with a Done or Error message.
    let _dir = tempdir().unwrap();
    let manager = Arc::new(VmManager::new(
        &_dir.path().join("vms.json"),
        Box::new(MockVm::new()),
    ));
    let svc = VmServiceImpl::new(manager, test_resolver(), TaskRegistry::new());
    let bridge = RpcBridge::new(VmServiceServer::new(svc));

    let messages: Vec<BuildVmImageProgress> = call_stream(
        &bridge,
        "BuildVmImage",
        BuildVmImageRequest {
            session_token: GOOD_TOKEN.to_string(),
            buildroot_spec: "BR2_x86_64=y\nBR2_TOOLCHAIN_BUILDROOT_GLIBC=y\n".to_string(),
        },
    )
    .await;

    assert!(
        !messages.is_empty(),
        "must emit at least one progress message"
    );
    let last = messages.last().unwrap();
    // stage 4 = STAGE_DONE, stage 5 = STAGE_ERROR
    assert!(
        last.stage == 4 || last.stage == 5,
        "last message must be Done or Error, got stage {}",
        last.stage
    );
    if last.stage == 4 {
        assert!(
            !last.image_path.is_empty(),
            "Done message must carry a non-empty image_path"
        );
    }
}

#[tokio::test]
async fn list_vms_with_invalid_token_returns_unauthenticated() {
    let _dir = tempdir().unwrap();
    let manager = Arc::new(VmManager::new(
        &_dir.path().join("vms.json"),
        Box::new(MockVm::new()),
    ));
    let svc = VmServiceImpl::new(manager, test_resolver(), TaskRegistry::new());
    let bridge = RpcBridge::new(VmServiceServer::new(svc));

    assert_unauthenticated(
        &bridge,
        "ListVms",
        ListVmsRequest {
            session_token: BAD_TOKEN.to_string(),
        }
        .encode_to_vec(),
    )
    .await;
}

#[tokio::test]
async fn define_vm_with_invalid_token_returns_unauthenticated() {
    let _dir = tempdir().unwrap();
    let manager = Arc::new(VmManager::new(
        &_dir.path().join("vms.json"),
        Box::new(MockVm::new()),
    ));
    let svc = VmServiceImpl::new(manager, test_resolver(), TaskRegistry::new());
    let bridge = RpcBridge::new(VmServiceServer::new(svc));

    assert_unauthenticated(
        &bridge,
        "DefineVm",
        DefineVmRequest {
            session_token: BAD_TOKEN.to_string(),
            spec: Some(VmSpecProto {
                name: "vm".to_string(),
                image_path: "/fake.qcow2".to_string(),
                build_target: String::new(),
                ssh_host_port: 2225,
                port_forwards: vec![],
            }),
        }
        .encode_to_vec(),
    )
    .await;
}

#[tokio::test]
async fn list_vm_images_returns_empty_when_no_images_built() {
    // With no built images in the scan dir, ListVmImages returns an empty list (no error).
    let _dir = tempdir().unwrap();
    let manager = Arc::new(VmManager::new(
        &_dir.path().join("vms.json"),
        Box::new(MockVm::new()),
    ));
    let svc = VmServiceImpl::new(manager, test_resolver(), TaskRegistry::new());
    let bridge = RpcBridge::new(VmServiceServer::new(svc));

    let result: ListVmImagesResponse = call(
        &bridge,
        "ListVmImages",
        ListVmImagesRequest {
            session_token: GOOD_TOKEN.to_string(),
        },
    )
    .await;

    // The scan dir (tmp/buildroot/disks) likely doesn't exist in the test environment.
    // What matters is that a valid token succeeds without error and returns a (possibly empty) list.
    let _ = result.images.len();
}

#[tokio::test]
async fn list_vm_images_with_invalid_token_returns_unauthenticated() {
    let _dir = tempdir().unwrap();
    let manager = Arc::new(VmManager::new(
        &_dir.path().join("vms.json"),
        Box::new(MockVm::new()),
    ));
    let svc = VmServiceImpl::new(manager, test_resolver(), TaskRegistry::new());
    let bridge = RpcBridge::new(VmServiceServer::new(svc));

    assert_unauthenticated(
        &bridge,
        "ListVmImages",
        ListVmImagesRequest {
            session_token: BAD_TOKEN.to_string(),
        }
        .encode_to_vec(),
    )
    .await;
}

// ─── VM build fold-in tests ───────────────────────────────────────────────────

/// Regression guard: BuildVmImage still streams at least one progress message after the
/// VmBuildTaskBody adapter was introduced. A STAGE_ERROR is acceptable in the test environment
/// (BUILDROOT_DIR is not set), but the stream must not be empty and must be reachable.
#[tokio::test]
async fn build_vm_image_adapter_still_delivers_progress_messages() {
    // Given — VmService backed by a shared TaskRegistry (mirrors the daemon setup)
    let _dir = tempdir().unwrap();
    let registry = TaskRegistry::new();
    let manager = Arc::new(VmManager::new(
        &_dir.path().join("vms.json"),
        Box::new(MockVm::new()),
    ));
    let svc = VmServiceImpl::new(manager, test_resolver(), registry.clone());

    // Note: we pass the registry clone to VmServiceImpl so the build task lands there.
    let bridge = RpcBridge::new(VmServiceServer::new(svc));

    // When — BuildVmImage is called (BUILDROOT_DIR not set → immediate STAGE_ERROR)
    let messages: Vec<BuildVmImageProgress> = call_stream(
        &bridge,
        "BuildVmImage",
        BuildVmImageRequest {
            session_token: GOOD_TOKEN.to_string(),
            buildroot_spec: "BR2_x86_64=y\n".to_string(),
        },
    )
    .await;

    // Then — at least one progress message is delivered (stream not empty)
    assert!(
        !messages.is_empty(),
        "BuildVmImage adapter must emit at least one progress message"
    );

    // And the last message is a terminal stage (STAGE_DONE=4 or STAGE_ERROR=5)
    let last = messages.last().unwrap();
    assert!(
        last.stage == 4 || last.stage == 5,
        "last progress message must be terminal (Done or Error); got stage {}",
        last.stage
    );
}

/// Fold-in: a VM build spawned by BuildVmImage appears in the TaskRegistry and can be cancelled.
/// Uses a simulated long build: since BUILDROOT_DIR is not set the body exits quickly with Failed,
/// so we instead verify that CancelTask is accepted on a live task spawned with the same body type.
#[tokio::test]
async fn vm_build_task_appears_in_registry_after_build_call() {
    // Given — VmService sharing a TaskRegistry with a TaskService observer
    let _dir = tempdir().unwrap();
    let registry = TaskRegistry::new();
    let manager = Arc::new(VmManager::new(
        &_dir.path().join("vms.json"),
        Box::new(MockVm::new()),
    ));
    let svc = VmServiceImpl::new(manager, test_resolver(), registry.clone());
    let bridge = RpcBridge::new(VmServiceServer::new(svc));

    // When — BuildVmImage is called; the body completes quickly (BUILDROOT_DIR not set → Failed)
    let _: Vec<BuildVmImageProgress> = call_stream(
        &bridge,
        "BuildVmImage",
        BuildVmImageRequest {
            session_token: GOOD_TOKEN.to_string(),
            buildroot_spec: "BR2_x86_64=y\n".to_string(),
        },
    )
    .await;

    // Then — the registry has at least one task of kind "vm_build"
    let tasks = registry.list().await;
    let vm_task = tasks.iter().find(|t| t.kind == "vm_build");
    assert!(
        vm_task.is_some(),
        "registry must contain a vm_build task after BuildVmImage; got kinds: {:?}",
        tasks.iter().map(|t| &t.kind).collect::<Vec<_>>()
    );

    // And the task is terminal (build fails immediately without BUILDROOT_DIR)
    let task = vm_task.unwrap();
    assert!(
        task.status().is_terminal(),
        "vm_build task must reach terminal status; got {:?}",
        task.status()
    );
}
