# 2026-07-02 — `grpc_socket` arg made optional

**Type:** Feature

`SandboxRunnerArgs::grpc_socket` is now `Option<PathBuf>`; it was already vestigial (unused once `--stdio` is passed, never read anywhere else in the crate), but was still a required flag, which blocked `tddy-daemon` from spawning the runner with `--stdio` and no gRPC flags at all. Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-sandbox-runner, tddy-daemon)
