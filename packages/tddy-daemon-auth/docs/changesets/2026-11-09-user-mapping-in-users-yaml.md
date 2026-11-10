# 2026-11-09 — First-login enrolment writes users.yaml; a supervised mapping refuses enrolment

**Type:** Architecture

`#agent-tools` 31/32 ([#859](https://github.com/uppin/tddy-coder/pull/859)).

- `first_login_admission.rs`: `EnrolmentRefusal::OwnedBySupervisor` is refused `failed_precondition`, naming `users.yaml` (a guard: a supervised daemon is built with no admission). Enrolment persists to the `users.yaml` beside the config.
- Test fixtures (`auth.rs`, `lib.rs`, seven acceptance suites, `tests/support/mod.rs`) write `users.yaml` instead of a `users:` key.
- Code issue `oversized-file-auth`: 579, unchanged.
- Docs: [auth-service.md](../auth-service.md).

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-09-user-mapping-in-users-yaml.md).
