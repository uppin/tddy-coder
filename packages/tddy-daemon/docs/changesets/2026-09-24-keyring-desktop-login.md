# 2026-09-24 — First-login admission on an embedded host, and the declared sign-in flow

**Type:** Feature

`#keyring` 2/9, PR [#509](https://github.com/uppin/tddy-coder/pull/509). Cross-package entry: [`docs/dev/changesets/2026-09-24-keyring-desktop-login.md`](../../../../docs/dev/changesets/2026-09-24-keyring-desktop-login.md)

`runtime::first_login_enrolment` gives an embedded host started from a config file a `FirstLoginEnrolment` over `config.users`, that file and the OS user the process runs as (`this_process_os_user`), passed to `build_auth_entries_admitting`; the binary gets none. An embedded host serving sign-in to an empty `users:` with no config path refuses to build. `GET /api/config` and `GetClientConfig` carry `auth_flow` from `github_auth_flow`. `UpdateConfig` rewrites the file under `LiveUsers::while_rewriting_config_file`, so it cannot drop an enrolled row. The agent tool socket stamps `UnixSocket`. Pinned by `tests/first_login_enrolment_acceptance.rs`, `server_options_acceptance.rs` and `daemon_config_service.rs`. Detail: [daemon-endpoint.md](../daemon-endpoint.md#first-login-admission).
