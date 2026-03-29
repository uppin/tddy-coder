# Evaluation Report

## Summary

Remote sandbox feature adds Connect/LiveKit RPCs, daemon wiring, tddy-remote CLI with RSH/rsync bridge, VFS helpers, and broad integration tests. cargo check for the touched crates passed; review notes codegen warnings, a stray test output file, and security follow-up for sandbox RPCs.

## Risk Level

medium

## Changed Files

- Cargo.lock (modified, +35/−0)
- Cargo.toml (modified, +1/−0)
- packages/tddy-daemon/Cargo.toml (modified, +2/−0)
- packages/tddy-daemon/src/lib.rs (modified, +1/−0)
- packages/tddy-daemon/src/main.rs (modified, +7/−0)
- packages/tddy-integration-tests/Cargo.toml (modified, +6/−0)
- packages/tddy-service/Cargo.toml (modified, +5/−1)
- packages/tddy-service/build.rs (modified, +10/−0)
- packages/tddy-service/src/lib.rs (modified, +8/−0)
- packages/tddy-daemon/src/remote_sandbox/mod.rs (added, +3/−0)
- packages/tddy-daemon/src/remote_sandbox/vfs.rs (added, +36/−0)
- packages/tddy-daemon/tests/remote_sandbox_connect_integration.rs (added, +455/−0)
- packages/tddy-daemon/tests/remote_sandbox_vfs_rules.rs (added, +15/−0)
- packages/tddy-integration-tests/tests/remote_sandbox_livekit.rs (added, +119/−0)
- packages/tddy-remote/Cargo.toml (added, +40/−0)
- packages/tddy-remote/src/config.rs (added, +66/−0)
- packages/tddy-remote/src/connect_client.rs (added, +48/−0)
- packages/tddy-remote/src/lib.rs (added, +13/−0)
- packages/tddy-remote/src/main.rs (added, +110/−0)
- packages/tddy-remote/src/rsh.rs (added, +306/−0)
- packages/tddy-remote/src/session.rs (added, +83/−0)
- packages/tddy-remote/src/vfs_path.rs (added, +41/−0)
- packages/tddy-remote/tests/cargo_graph_guard.rs (added, +61/−0)
- packages/tddy-remote/tests/cli_list.rs (added, +41/−0)
- packages/tddy-remote/tests/config_yaml_parse.rs (added, +20/−0)
- packages/tddy-remote/tests/shell_exit_code.rs (added, +172/−0)
- packages/tddy-remote/tests/vfs_path_normalize.rs (added, +16/−0)
- packages/tddy-service/proto/remote_sandbox.proto (added, +59/−0)
- packages/tddy-service/src/remote_sandbox_service.rs (added, +390/−0)
- packages/tddy-service/tests/remote_sandbox_stub_red.rs (added, +70/−0)
- .red-remote-sandbox-test-output.txt (added, +834/−0)

## Affected Tests

- packages/tddy-daemon/tests/remote_sandbox_connect_integration.rs: created
- packages/tddy-daemon/tests/remote_sandbox_vfs_rules.rs: created
- packages/tddy-integration-tests/tests/remote_sandbox_livekit.rs: created
- packages/tddy-remote/tests/cli_list.rs: created
- packages/tddy-remote/tests/shell_exit_code.rs: created
- packages/tddy-remote/tests/vfs_path_normalize.rs: created
- packages/tddy-remote/tests/config_yaml_parse.rs: created
- packages/tddy-remote/tests/cargo_graph_guard.rs: created
- packages/tddy-service/tests/remote_sandbox_stub_red.rs: created

## Validity Assessment

The changes substantially address the PRD intent: per-connection sandbox concepts, SSH-like exec streams and exit status, SFTP-like/VFS operations suitable for rsync, Connect registration on the daemon, a tddy-remote CLI with --rsh-style bridging, LiveKit coverage in integration tests, and a dependency guard so tddy-remote does not directly depend on tddy-workflow.

## Build Results

- cargo check -p tddy-service -p tddy-daemon -p tddy-remote -p tddy-integration-tests: pass

## Issues

- [low/hygiene] .red-remote-sandbox-test-output.txt: remove or gitignore
- [low/code_quality] Generated remote_sandbox.v1.rs warnings
- [low/code_quality] packages/tddy-remote/src/config.rs: default_authority unread
- [medium/security] Remote sandbox RPC attack surface / auth review
- [low/maintenance] Branch behind origin/master
