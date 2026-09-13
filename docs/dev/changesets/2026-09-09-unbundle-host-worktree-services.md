# 2026-09-09 — Cross-crate moves, the daemon kernel, and the host and worktree services

**Type:** Architecture

Root node of the `#unbundle` stack ([#470](https://github.com/uppin/tddy-coder/pull/470), 1 of 8,
based on `master`). Three deliverables that are worthless apart and are therefore one PR: a
restructure operation that can cross a crate boundary, the shared kernel that makes a crate split
possible at all, and the first subsystem group carried all the way through by both.

## What landed

| | |
|---|---|
| `tddy-code-restructuring` | an eighth Rust operation, `move_module_to_crate`; `rename_symbol` re-points callers in other files; `check --budget LINES` reports a file budget |
| `tddy-daemon-kernel` *(new)* | `AgentActivityHub`, `now_unix_ms`, `HOST_DOCUMENT_FRAME_BYTES`, `SessionUserResolver`, `SessionsBaseResolver`, the spawn preamble, the trim helper — and `config.rs` whole |
| `tddy-host-service` *(new)* | 13 modules serving `host.HostService` (8 methods) |
| `tddy-worktree-service` *(new)* | 8 modules serving `worktree.WorktreeService` (9 methods) |
| `tddy-daemon` | 86 modules; `run_server(RunServerOptions)`; two more `ServiceEntry`s and two hand-written tonic adapters |
| `tddy-service` | `connection.proto` 90 → 73 rpcs and 51 fewer messages/enums; `host.proto` and `worktree.proto` appear |
| `tddy-web` | every host and worktree call site, six hard-coded bindings, the regenerated `src/gen/`, and the Cypress fakes |
| CI | `scripts/generated-code.sh` — a drift gate over all four committed generated directories |

Docs: [`tddy-host-service`](../../../packages/tddy-host-service/docs/host-service.md),
[`tddy-worktree-service`](../../../packages/tddy-worktree-service/docs/worktree-service.md),
[`connection-service.md`](../../../packages/tddy-daemon/docs/connection-service.md),
[`rust-code-restructuring.md`](../../ft/coder/rust-code-restructuring.md),
[`host-worktree-services.md`](../../ft/daemon/host-worktree-services.md).

## There is no `types.proto`, and there should not be

The plan assumed this node would introduce the shared types file every later node imports. Walking
the field types of `connection.proto`'s 238 messages refutes it for *these* families: the closure the
host methods reach is 31 messages, the worktree methods reach 20, and there is **zero overlap between
them and zero overlap with the closure of everything that stays**. `WorktreeRow`, `ProbeOutcome` and
`WorktreeSizeStatus` were all on the planning-time list of cross-family shared messages and are in
fact reached only from inside the moving set.

Creating one here would mean moving messages this node does not use so that a later node can — the
stubs-as-deliverable shape the boundary contract forbids. A test in
`packages/tddy-service/tests/unbundle_service_split.rs` pins the absence, so a later node cannot
re-couple the two protos by importing one out of habit. The shared-types decision belongs to node 6,
where four families genuinely do share `SessionAttachment`, `StagedAttachmentRef` and
`HostDocumentRef`.

## The cycle audit — the planning table was wrong about three, and missed six

Rust crates cannot be mutually dependent, so the module cycles had to be resolved before any crate
could be cut. Re-deriving the module graph at `ac002643` found **15** mutual pairs, not the nine the
discovery table listed, and three of the nine **are not cycles at all**:
`livekit_peer_discovery → common_room_supervisor` is zero (every arrow runs *out* of
`common_room_supervisor`), `worktree_files → context_files` is zero, and
`session_agent_status → session_agent_inference` is zero. What that last row was really describing is
`connection_service ⇄ session_agent_inference`.

Adopting the kernel plus inlining `DEFAULT_SESSION_ROOM_GIT_TIMEOUT` cuts **six** real cycles —
`connection_service` against `session_agent_inference`, `sandbox_session`,
`telegram_session_subscriber` and `context_files` (three of which the table missed entirely and which
the kernel cuts for free), plus `config ⇄ session_room`.

**Five of the nine that remain are classification cuts, not edits.** `host_tooling ⇄ ssh_agent`,
`host_tooling ⇄ remote_desktop_probe`, `livekit_peer_discovery ⇄ multi_host`,
`telegram_notifier ⇄ telegram_session_control` and `telegram_multi_select_shortcuts ⇄
telegram_notifier` each land in the **same** destination crate, so the crate graph is acyclic the
moment the modules move; forcing a module-level cut now would be churn with no boundary behind it.
The four that genuinely span destination crates — `host_registry ⇄ livekit_peer_discovery`,
`livekit_peer_discovery ⇄ split_session`, `connection_service ⇄ cursor_cli_spawn` (the spawn preamble
the kernel does **not** carry) and `connection_service ⇄ test_util` — belong to the nodes that move
those modules. **15 → 9.**

## `config.rs` moves into the kernel whole

A deliberate decision that widens the kernel's charter beyond "the five shared symbols", and the
*only* whole-module move the kernel takes.

Four of the moving modules (`host_registry`, `host_tooling`, `remote_desktop_probe`, `multi_host`)
and every handler in both new services take `&DaemonConfig` and read disjoint parts of it, so
`pub(crate)`-widening buys nothing and **there is no smaller cut: the symbol is the file**. The
alternative — a narrow value struct per consuming crate — is authoring rather than moving, and nodes
2–8 would each have to repeat it, so the collision it is meant to avoid is exactly the one it would
cause. `DaemonConfig` is not one of the *subsystems* the stack reserves for later nodes.
`tddy-daemon` keeps `pub use tddy_daemon_kernel::config;`, so no caller in it changed.

Everything else the kernel gained is a **symbol lift, measured before it was made**: `spawn_as_user`
is 179 of `spawner.rs`'s 2,539 lines, `privilege_drop` 63 of `pty_runtime.rs`'s 400, `user_paths` 34
of `user_sessions_path.rs`'s 210. `pty_registry.rs` was moved and then **retracted untouched**,
because nothing in the moving families reaches it. Every origin module re-exports every lifted name,
so no caller in `tddy-daemon` changed and there stays exactly one definition of each.

## `now_unix_ms` saturates — a behaviour decision, not a move

The symbol existed **three times**, and not identically: `session_agent_status.rs` saturated at
`u64::MAX`, `host_registry.rs` returned `i64` with an explicit pre-1970 refusal, and
`connection_service/host_messages.rs` used a bare `as u64` cast that **truncates**.

The kernel's one definition **saturates**. A truncating cast is the wrong behaviour for a timestamp:
it turns a clock far in the future into a timestamp in the past, silently, and downstream that is
indistinguishable from correct data. Saturation is chosen over the `i64` refusal because every caller
here stamps a record it is about to write, and a caller that cannot proceed without a plausible clock
is better served by checking the clock than by receiving an error from a timestamp function.
`host_registry::now_unix_ms` remains as a deliberate **adapter** over the kernel's — it returns `i64`
and keeps the pre-1970 diagnostic — not as a fourth copy.

## The move's own proof, and why it cannot report zero

`restructure verify --against bb0695b0` is the proof of a mechanical move, in place of a test. It
exits 1:

```
341,794 statements before, 341,882 after
plan is malformed: 50 statement(s) the tree lost and 138 it gained
```

**No logic statement is among them.** All of them: 31 `crate::X` → `tddy_daemon_kernel::X` qualifier
re-points, 9 doc-link rewrites, 5 `pub(crate)` → `pub` widenings, 2 statements from consolidating
`local_hostname_or_local` + `process_startup_unix_ms_suffix` into `daemon_identity`, 2 `include_str!`
path re-points, and 4 `#[must_use]` attributes on the new `daemon_identity` functions.

A cross-crate move **cannot** change zero tokens — the qualifier at the head of every moved `use`
changed meaning by definition — so "no lost or gained statements" was the wrong acceptance criterion
and is corrected as such. Corroborated by the diff shape: 37 files, +617/−390, nine of them moving
with a zero-line diff, and every moved file 85–100% similar by git's own rename detection.

## The file budget is a report, not a gate

Seams are cut where they are cohesive; whatever stays over 500 lines is recorded with a reason rather
than split to hit a number. **Twelve files stand over budget.** Eleven of them *moved* rather than
being written here — `host_registry.rs` (1,075), `host_tooling.rs` (1,074), `worktrees.rs` (1,052),
`host_private_key.rs` (995), `remote_git_service.rs` (864), `ssh_agent.rs` (799),
`project_storage.rs` (653), `worktree_files.rs` (629), `host_add_key_handler_tests.rs` (1,259, being
26 tests of one flow), and `config.rs` (2,448, one serde schema — splitting a config struct splits
nothing cohesive).

The twelfth, `crate_move.rs` (1,609), **this node wrote**, and it is absent from the
`restructure check --budget` run for a mechanical reason worth stating: that command measures only
the files a plan's **anchors** name, and `crate_move.rs` is the operation's own implementation, never
a plan anchor. The tool cannot see its own size, so the record has to say so rather than let the
omission read as a clean result. The two authored service files (880 and 753 lines) are each one
service's handlers plus the state they read; splitting them would separate a handler from the field
it consults, which is the cohesion the budget exists to protect.

`tddy-daemon` shrank: `connection_service/rpc_service.rs` 6,938 → 5,712,
`connection_tonic_adapter.rs` 1,505 → 1,306, and 21 modules left the crate.

## Baseline, and a failure ledger for all 21

| Gate | Before | After |
|---|---|---|
| `cargo build --workspace` | ✅ clean | ✅ clean |
| `cargo fmt --all --check` | ✅ clean | ✅ clean |
| `./test -p tddy-daemon` | 1027 passed / 1 failed | 583 passed / 0 failed (lib) — 226 unit tests travelled into the new crates |
| `cargo test -p tddy-daemon --no-fail-fast` | 2124 passed / **39 failed** | 1866 passed / **21 failed** — ledger below |
| `./test -p tddy-code-restructuring` | 251 / 0 | 287 / 0 |
| `cargo test -p tddy-daemon-kernel` | n/a | 85 / 0 |
| `cargo test -p tddy-host-service` | n/a | 129 / 0 |
| `cargo test -p tddy-worktree-service` | n/a | 81 / 0 |
| `cargo test -p tddy-service` | 104 passed, 2 failed (the completion criterion) | 109 / 0 |
| `scripts/generated-code.sh check` | ⛔ did not exist | ✅ exit 0, all four directories |
| `bun run --filter tddy-web cypress:component` | not run | ✅ 231 specs, 1419/1419, 5m34s |

**No test was lost to the split.** 583 + 85 + 129 + 81 = **878**, against 828 before.

The "exactly one known failure" figure the plan carried came from a run **truncated by cargo's
fail-fast** at binary 25 of 169. The true `--no-fail-fast` number is 21, and every one is
pre-existing or environmental:

| n | Failure | Status |
|---|---|---|
| 17 | `connection_service/svc_resolve_tddy_tools_path.rs:163` — `self_arc called before set_self_handle` | **Pre-existing**, the same root cause as the five `sandbox_behavior_acceptance` failures known on `master`: a missing `set_self_handle` in the test's own construction of `ConnectionServiceImpl`. It reaches more suites than the truncated baseline showed only because `--no-fail-fast` gets past binary 25 |
| 2 | `sandbox_session_stdio_acceptance` | **Pre-existing and stale**: the test `include_str!`s `src/connection_service.rs` and greps it for `"--stdio"`, a string at zero occurrences there before this PR as well — the spawn argv moved to `connection_service/svc_start_sandboxed_*.rs` in an earlier commit and the anchor was never updated |
| 1 | `sandbox_stdio_seatbelt_acceptance` | **Environmental** — tool dispatch timeout in a real Seatbelt jail |
| 1 | `session_room_acceptance` | **Environmental** — LiveKit container/port contention |

The 13 `tddy-remote-git-repo not built` failures in the *before* column are gone for an environmental
reason, not a code one: `cargo test` does not build that binary and a `cargo build --workspace` in
the same session did. They are a property of the runner, not of the tree.

## Other decisions worth keeping

- **The host-key path travels with the host service, not with auth.** `AddHostKey` and
  `ListHostKeyCandidates` are host-service methods, and moving `host_keypair`, `host_private_key`,
  `ssh_agent` and `ssh_agent_add` here cuts `host_tooling ⇄ ssh_agent` as a side effect of a move
  that was going to happen anyway. The cost is that node 4's auth crate is smaller than "auth"
  suggests.
- **A caller's rewritten path keeps the module segment**: `crate::host_registry::HostRegistry` becomes
  `tddy_host_service::host_registry::HostRegistry`, not `tddy_host_service::HostRegistry`. That is
  what makes the glob facade free.
- **Every acceptance test for the operation ends in `cargo check`.** The first live run emitted a
  tree that read correctly and did not compile (`E0433`/`E0432`) on **both** the facade and the
  no-facade path, while all 271 unit tests passed — because a manifest that is never compiled looks
  fine. Only a compiler distinguishes an edit that looks right from one that resolves.
- **The drift gate's inherited rot was settled, not excluded.** It was red the day it was added, on
  drift no PR introduced. The stale directories were regenerated through the script's own `write`
  mode and the two `codex_oauth_pb.ts` orphans deleted, rather than excluding a directory to get a
  green check.
- **Generated TypeScript stays in `packages/tddy-web/src/gen/`.** A workspace package per generated
  service would need a root `workspaces` entry, a `workspace:*` entry, a fresh `bun install` and
  regenerated lockfiles, for no gain — `buf generate` already produces one `*_pb.ts` per proto with
  no config change.
- **`tddy-desktop` is not a `run_server` caller**, contrary to the plan: its only mention is prose in
  a `TODO` comment. It was still built and linted locally, because it is outside the CI gate and this
  claim is not made on CI's authority.

## Open items

Recorded in `docs/dev/todo/`:
[versioned proto package names](../todo/2026-09-09-versioned-proto-package-names.md),
[hand-written tonic adapters](../todo/2026-09-09-tonic-adapters-are-hand-written-per-service.md),
[the cross-crate move's own defects](../todo/2026-09-09-restructure-defects-from-the-first-cross-crate-move.md).

Closed by this change:
`connection_service.rs` is 19,600 lines,
22,800 lines,
`run_server` takes 12 positional arguments,
the stale generated TypeScript,
`daemon_config_pb.ts` regenerated without `buf`.
