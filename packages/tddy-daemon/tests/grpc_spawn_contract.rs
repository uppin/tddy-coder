//! Contract: each spawned `tddy-coder` daemon must bind a gRPC port that was checked free
//! (or explicitly allocated), and the child must receive that port via `--grpc`. Relying on the
//! child default (50051) breaks concurrent sessions when the port is already taken.

#[test]
fn spawner_passes_verified_free_grpc_port_to_child() {
    let spawner_rs = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/spawner.rs"));
    assert!(
        spawner_rs.contains("verify_tcp_listen_port_free"),
        "spawner must verify the gRPC listen port is free before spawn"
    );
    assert!(
        spawner_rs.contains("\"--grpc\""),
        "spawner must pass --grpc to tddy-coder with the verified port"
    );
}
