# 2026-10-25 — DescribeSessionStartOptions lists a host's agents, models and choices

**Type:** Feature

`#start-parity` 5/9 ([#754](https://github.com/uppin/tddy-coder/pull/754)). A project-home session
can ask any eligible host for the choices a `StartSession` there can make: its agents, the models of
an agent or provider account, the provider accounts that can run a harness, its SSH config hosts,
subagents, the project checkout's branches, and the sessions that can parent or seed a PR stack —
each from the RPC the New-session form fills that picker from. The two stack filters are mirrored in
Rust and pinned to the form's TypeScript by one shared fixture. Developer decision: **the provider
accounts are a late-bound port** (`RpcHandlers::attach_provider_accounts`), not a crate dependency;
a host with no credential vaults answers them as unavailable with a reason. The stack-parent picker
is not limited to the project, exactly as in the form. Across:

- `tddy-tool-engine` — `start_options.rs` and the tool's definition, dispatch and port method.
- `tddy-daemon-rpc` — `exec_tool/project_home_start_options.rs`, `ProviderAccountsPort`, and a test
  host whose test user maps to the current OS user.
- `tddy-accounts` — `build_accounts_entry` takes an `Arc`, so the daemon keeps a handle on the
  instance the RPC serves.
- `tddy-daemon` — `runtime/mount_accounts.rs` attaches that instance as the port; the port's
  implementation for `AccountsServiceImpl` lives in `tddy-daemon-rpc` `families.rs`.
- `tddy-web` — `stackParentsParity.cy.ts` and the shared fixture.

Code issues (split deferred, stack-shared): `tddy-tool-engine/src/project_home_tools.rs` 558 → 601,
`tddy-tool-engine/src/lib.rs` 880 → 881, `tddy-daemon-rpc/src/exec_tool/project_home.rs` 665 → 708;
`tddy-accounts/src/service.rs` 772 (touched, not grown; new record). TODO added: the shared test
config's `testdev` OS user resolves on no machine.
