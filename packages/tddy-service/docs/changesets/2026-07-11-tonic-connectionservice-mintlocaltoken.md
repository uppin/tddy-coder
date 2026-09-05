# 2026-07-11 — tonic `ConnectionService` + `MintLocalToken`

**Type:** Feature

wired a tonic codegen pass for `connection.proto` (server/client reusing prost messages via package `extern_path`); `StartSessionRequest.{repo_path,claude_args}`; new `MintLocalToken` RPC. Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). PR [#291](https://github.com/uppin/tddy-coder/pull/291) (draft). (tddy-service, tddy-daemon, tddy-sandbox-app)
