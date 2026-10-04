# 2026-10-04 — Project account assignments (#keyring 5/9)

**Type:** Feature

PR [#512](https://github.com/uppin/tddy-coder/pull/512). A project row carries assigned accounts, an
RPC sets them, the Projects screen shows and changes them, and a resolver answers which account a
project uses for a provider, with nothing when none is assigned.

## What changed

| Package | Change |
|---|---|
| `tddy-projects` | `ProjectData.accounts`; `set_project_accounts` (replace-whole-list, one per provider); `SetProjectAccounts` on the handler trait. [project-service.md](../../packages/tddy-projects/docs/project-service.md) |
| `tddy-service` | `project.proto`: `ProjectEntry.accounts = 8`, `SetProjectAccounts` and `AccountAssignment`; the method is registered in the closed `RESIDUAL_METHODS` list (18 entries) |
| `tddy-accounts` | `AccountResolution` and `resolve_account`. [accounts-service.md](../../packages/tddy-accounts/docs/accounts-service.md) |
| `tddy-daemon-rpc` | `SetProjectAccounts` handler in `src/project/`; the acceptance test is in `tddy-daemon-rpc/tests/` because `#carve` moved `ProjectService` here |
| `tddy-daemon-livekit` | `forward_set_project_accounts_via_livekit` |
| `tddy-web` | per-provider account control. [projects-screen.md](../../packages/tddy-web/docs/projects-screen.md) |

Product behaviour: [project-concept.md](../../ft/daemon/project-concept.md#account-assignment),
[projects-screen-multi-host.md](../../ft/web/projects-screen-multi-host.md#accounts).

## Decisions

- `accounts` is `Vec<AccountAssignment { provider, account_id }>`, not bare ids: an `account_id` is
  unique only within its provider.
- No fallback for an unassigned project; the resolver lives in `tddy-accounts` so `tddy-projects`
  does not depend on `tddy-credentials`.
- No git or GitHub call site changed. [#516](https://github.com/uppin/tddy-coder/pull/516) (9/9)
  consumes the resolver; [#513](https://github.com/uppin/tddy-coder/pull/513) (6/9) makes an
  assignment meaningful on a peer host, which is when `UnknownOnThisHost` stops being steady state.

## Measured results (scoped; whole-workspace health is CI's)

- `./test` over `tddy-projects`, `tddy-accounts`, `tddy-daemon-rpc`, `tddy-daemon-livekit`,
  `tddy-service`: 636 passed, 0 failed, 1 ignored.
- Cypress component: `ProjectAccountsAcceptance` 8/8, `ProjectsScreenAcceptance` 13/13.
- `cargo clippy --all-targets -- -D warnings` clean on `tddy-projects`, `tddy-accounts`,
  `tddy-daemon-rpc`, `tddy-daemon-livekit`.
- File length: `coordinate_handlers.rs` 528 to 650 production lines and `livekit_peer_discovery.rs`
  1,636 to 1,658; both deferred, recorded in the packages' `oversized-file` code-issue records and,
  for the first, in the backlog entry `2026-10-04-keyring-assignments-grew-project-coordinate-handlers`.

## Known gaps

- No test exercises the peer-forward route of `SetProjectAccounts`; it needs a peer harness.
- `ProjectsAppPage` swallows RPC errors from `setProjectAccounts` and `useAssignableAccounts`, as
  `setDefaultBranch` does; they are to change together.
