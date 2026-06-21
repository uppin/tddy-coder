//! VM service acceptance tests — drives VmService through RpcBridge with mock backend.
//! Fails until VmServiceImpl methods are implemented.

use prost::Message;
use std::sync::Arc;
use tddy_rpc::{RequestMetadata, ResponseBody, RpcBridge, RpcMessage};
use tddy_service::proto::vm::{
    BuildVmImageProgress, BuildVmImageRequest, DefineVmRequest, DefineVmResponse,
    GetVmStatusRequest, GetVmStatusResponse, ListVmsRequest, ListVmsResponse, StartVmRequest,
    StartVmResponse, StopVmRequest, StopVmResponse, VmServiceServer, VmSpecProto,
};
use tddy_vm::service::VmServiceImpl;
use tddy_vm::{MockVm, VmManager};
use tempfile::tempdir;

const TOKEN: &str = "valid-token";

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

#[tokio::test]
async fn define_vm_then_list_shows_it() {
    // Given — a fresh VmService backed by MockVm
    let _dir = tempdir().unwrap();
    let manager = Arc::new(VmManager::new(
        &_dir.path().join("vms.json"),
        Box::new(MockVm::new()),
    ));
    let svc = VmServiceImpl::new(manager);
    let bridge = RpcBridge::new(VmServiceServer::new(svc));

    // When — DefineVm is called
    let _: DefineVmResponse = call(
        &bridge,
        "DefineVm",
        DefineVmRequest {
            session_token: TOKEN.to_string(),
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
            session_token: TOKEN.to_string(),
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
    let svc = VmServiceImpl::new(manager);
    let bridge = RpcBridge::new(VmServiceServer::new(svc));

    let _: DefineVmResponse = call(
        &bridge,
        "DefineVm",
        DefineVmRequest {
            session_token: TOKEN.to_string(),
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
            session_token: TOKEN.to_string(),
            name: "app".to_string(),
        },
    )
    .await;

    // Then — GetVmStatus returns RUNNING
    let status: GetVmStatusResponse = call(
        &bridge,
        "GetVmStatus",
        GetVmStatusRequest {
            session_token: TOKEN.to_string(),
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
    let svc = VmServiceImpl::new(manager);
    let bridge = RpcBridge::new(VmServiceServer::new(svc));

    let _: DefineVmResponse = call(
        &bridge,
        "DefineVm",
        DefineVmRequest {
            session_token: TOKEN.to_string(),
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
            session_token: TOKEN.to_string(),
            name: "runner".to_string(),
        },
    )
    .await;

    // When — StopVm is called
    let _: StopVmResponse = call(
        &bridge,
        "StopVm",
        StopVmRequest {
            session_token: TOKEN.to_string(),
            name: "runner".to_string(),
        },
    )
    .await;

    // Then — GetVmStatus returns STOPPED
    let status: GetVmStatusResponse = call(
        &bridge,
        "GetVmStatus",
        GetVmStatusRequest {
            session_token: TOKEN.to_string(),
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
    let svc = VmServiceImpl::new(manager);
    let bridge = RpcBridge::new(VmServiceServer::new(svc));

    let messages: Vec<BuildVmImageProgress> = call_stream(
        &bridge,
        "BuildVmImage",
        BuildVmImageRequest {
            session_token: TOKEN.to_string(),
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
