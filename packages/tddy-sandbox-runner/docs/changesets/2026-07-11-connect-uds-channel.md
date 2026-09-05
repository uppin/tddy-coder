# 2026-07-11 — `connect_uds_channel`

**Type:** Refactor

extracted/exported the AF_UNIX tonic `Channel` connector from `connect_sandbox_client_uds` so the daemon `ConnectionService` client reuses one UDS connector. Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). PR [#291](https://github.com/uppin/tddy-coder/pull/291) (draft). (tddy-sandbox-runner, tddy-sandbox-app)
