//! Red-phase: invoke stub `RemoteSandboxService` methods in-process so production `eprintln!` markers
//! appear in test output; each test fails until handlers return success.

use tddy_rpc::Request;
use tddy_service::proto::remote_sandbox_v1::{
    ExecChecksumRequest, ExecNonInteractiveRequest, PutObjectRequest, RemoteSandboxService,
    StatObjectRequest,
};
use tddy_service::RemoteSandboxServiceImpl;

#[tokio::test]
async fn red_exec_non_interactive_succeeds() {
    let s = RemoteSandboxServiceImpl::default();
    let _ = RemoteSandboxService::exec_non_interactive(
        &s,
        Request::new(ExecNonInteractiveRequest {
            argv_json: String::new(),
            session: String::new(),
        }),
    )
    .await
    .expect("ExecNonInteractive must return OK once remote sandbox is implemented");
}

#[tokio::test]
async fn red_put_object_succeeds() {
    let s = RemoteSandboxServiceImpl::default();
    let _ = RemoteSandboxService::put_object(
        &s,
        Request::new(PutObjectRequest {
            session: "a".into(),
            path: "/f".into(),
            content: vec![],
        }),
    )
    .await
    .expect("PutObject must return OK once remote sandbox VFS is implemented");
}

#[tokio::test]
async fn red_stat_object_succeeds() {
    let s = RemoteSandboxServiceImpl::default();
    let _ = RemoteSandboxService::put_object(
        &s,
        Request::new(PutObjectRequest {
            session: "b".into(),
            path: "/f".into(),
            content: vec![1, 2, 3],
        }),
    )
    .await
    .expect("PutObject must seed StatObject red test");
    let _ = RemoteSandboxService::stat_object(
        &s,
        Request::new(StatObjectRequest {
            session: "b".into(),
            path: "/f".into(),
        }),
    )
    .await
    .expect("StatObject must return OK once remote sandbox VFS is implemented");
}

#[tokio::test]
async fn red_exec_checksum_succeeds() {
    let s = RemoteSandboxServiceImpl::default();
    let _ = RemoteSandboxService::exec_checksum(&s, Request::new(ExecChecksumRequest {}))
        .await
        .expect("ExecChecksum must return OK once LiveKit exec path is implemented");
}
