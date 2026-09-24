# 2026-09-24 — `users:` is one live holder, and a desktop's first login is written into it

**Type:** Feature

`#keyring` 2/9, PR [#509](https://github.com/uppin/tddy-coder/pull/509). Cross-package entry: [`docs/dev/changesets/2026-09-24-keyring-desktop-login.md`](../../../../docs/dev/changesets/2026-09-24-keyring-desktop-login.md)

`DaemonConfig.users` is a `LiveUsers` (`live_users.rs`): clones of a config share the rows, so a row enrolled through any service is seen by all of them without a restart. It serialises as the plain list. `os_user_for_github` is unchanged in behaviour and has no default arm; it returns `Option<String>`. `first_login_enrolment.rs` adds `enrol_first_login` and `EnrolmentRefusal { AlreadyEnrolled, ConfigNotWritable }`: it re-reads the file, inserts only `users:` into the YAML document and writes it atomically. `LiveUsers::enrol_first_login` serialises it (and `UpdateConfig`, through `while_rewriting_config_file`) on one file lock, and applies the row only once persisted. Known limits: comments are stripped; cloning shares `users:`. Pinned by `tests/first_login_enrolment_acceptance.rs`. Detail: [daemon-kernel.md](../daemon-kernel.md#users-the-live-holder-and-first-login-enrolment).
