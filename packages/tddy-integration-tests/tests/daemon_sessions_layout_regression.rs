//! gRPC daemon and CLI share one layout: `{tddy_data_dir_path()}/sessions/<session_id>/`.
//! `DaemonService` is constructed with the parent of `sessions/` (same value as
//! `tddy_core::output::tddy_data_dir_path()`).

mod common;

use tddy_core::output::{create_session_dir_under, create_session_dir_with_id};

#[test]
fn grpc_daemon_session_path_matches_cli_layout_under_tddy_home() {
    let tmp = std::env::temp_dir().join(format!("tddy-grpc-session-layout-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();

    let sid_cli = "00000000-0000-7000-8000-00000000c111";
    let sid_grpc = "00000000-0000-7000-8000-00000000g222";

    let cli_session = create_session_dir_with_id(&tmp, sid_cli).expect("cli-style session dir");
    assert_eq!(cli_session, tmp.join("sessions").join(sid_cli));

    let grpc_session = create_session_dir_under(&tmp, sid_grpc).expect("grpc-style session dir");
    assert_eq!(grpc_session, tmp.join("sessions").join(sid_grpc));

    assert_eq!(
        cli_session.parent(),
        grpc_session.parent(),
        "CLI and gRPC must use the same sessions root directory"
    );

    let _ = std::fs::remove_dir_all(&tmp);
}
