# HostService (tddy-host-service)

Everything about the machines a daemon knows: the durable registry, each host's tooling probe, its
telemetry, the prompts it raises and the ssh keys it can be given — served as `host.HostService`.

## Where the code lives

The crate is the host subsystem plus the host-key path, and nothing else.

| Group | Files | What is in them |
|---|---|---|
| `service.rs` | 1 | `HostServiceImpl` — the eight handlers, the state they read, and the `with_*` builders that inject each OS seam |
| `stream.rs` | 1 | `MpscHostPromptStream`, `MpscHostStatsStream` — the two server-streaming adapters |
| the registry | `host_registry`, `multi_host`, `host_session_service` | which machines exist, and which project lives on which |
| the probes | `host_tooling`, `host_stats`, `remote_desktop_probe`, `host_desktop_targets` | what a host has, how busy it is, and whether a desktop on it can be reached |
| the prompt channel | `host_prompts`, `host_prompt_stream`, `host_messages` | the daemon-asks-the-operator mechanism and its wire mapping |
| the key path | `host_keypair`, `host_private_key`, `ssh_agent`, `ssh_agent_add` | the host RSA keypair, per-user private-key reading, and the agent protocol |
| `test_util.rs` | 1 | shared helpers, ungated, because the `tests/` suites reach for them |

**The key path lives here rather than with auth.** `AddHostKey` and `ListHostKeyCandidates` are
host-service methods, so the modules that serve them belong to the service that serves them — and
moving them here is also what cuts the `host_tooling ⇄ ssh_agent` dependency cycle, which no
crate boundary could have tolerated.

## Methods

| RPC | Purpose |
|---|---|
| `ListEligibleDaemons` | Eligible daemon instances for host selection (`instance_id`, `label`, `is_local`); sourced from `EligibleDaemonSource`. `is_local` compares against `local_instance_id_for_config`, the **routing** id, so it holds for a daemon carrying a configured `daemon_instance_id` or the startup-timestamp suffix |
| `ListKnownHosts` | Every host this daemon has a record of, live or not — one `KnownHostEntry` per host (`instance_id`, `label`, `online`, `first_seen_unix_ms`, `last_seen_unix_ms`, `repos_base_path`, `max_attachment_bytes`, `is_local`). Where `ListEligibleDaemons` answers "who can I route to now" and forgets a host the moment it leaves the room, this answers "what machines does tddy know about". **`online` is computed per call** by intersecting the durable registry with `EligibleDaemonSource::live_known_hosts()` (the roster keyed by **durable** host id, never the routing id) — it is never read from disk — and the serving daemon always has a row, flagged `is_local`. The registry is injected via `HostServiceImpl::with_host_registry`; the whole join, including that local-row guarantee, belongs to `HostRegistry::known_hosts`. Details: [host-registry.md](./host-registry.md) |
| `GetHostTooling` | What one host has installed and configured: the git identity its commits would carry, the state of the GitHub CLI there, and whether an ssh-agent is reachable for the host's OS user and what it is holding. Addressed by `daemon_instance_id` (empty = the daemon serving the call) and **routed before the caller is authenticated**. Each part of the answer carries a `ProbeOutcome` ahead of its findings, so "could not check" is never rendered as a negative finding. Details: [host-tooling-probe.md](./host-tooling-probe.md) |
| `StreamHostPrompts` | Server-streaming feed of the questions one host is waiting on **this operator** to answer. Each `HostPromptEvent` carries `prompt_id`, `daemon_instance_id`, `kind` (`HostPromptKind`), `subject` (what is being unlocked — never a secret), `host_public_key` (SPKI DER), `host_public_key_fingerprint` and `expires_at_unix_ms`. Filtered by the operator who raised the prompt, replays whatever is still outstanding to a late subscriber, and **watches `tx.closed()`** because the feed is silent by design |
| `AnswerHostPrompt` | Unary; answers one prompt with `encrypted_answer` — **RSA-OAEP(SHA-256) ciphertext under that event's `host_public_key`**, never a plaintext. Answerable once, and only by the operator who raised it; a prompt id that was never issued and one belonging to somebody else get the *same* rejection. Response is `{accepted, rejection_reason}` |
| `AddHostKey` | Unary, and **blocks for as long as the add takes**: it raises a passphrase prompt on `StreamHostPrompts`, waits for the ciphertext on `AnswerHostPrompt`, decrypts it, unlocks the OpenSSH key named by `subject` — read as the mapped OS user, out of their own home — and hands the identity to that user's ssh-agent. `subject` must be an **absolute** path; nothing expands `~`. Answers with `{added, outcome (AddHostKeyOutcome), fingerprint, failure_reason}` and never with anything derived from the passphrase |
| `ListHostKeyCandidates` | Unary; the private keys the caller's own OS user could load, out of their own `~/.ssh` — one `HostKeyCandidate` (`path`, `key_type`, `fingerprint`) per key **whose `.pub` sits beside it**, ordered by path. Every field comes from the public half, so no private key is opened to build the list, and every path offered is one `AddHostKey` accepts. Returns a list and never a failure: an absent, unreadable and empty `~/.ssh` are one answer |
| `StreamHostStats` | Server-streaming host telemetry — CPU, disk, memory and, where the platform provides one, load |

## How it is served

`tddy-daemon` owns the wiring. `runtime.rs` builds one `HostServiceImpl`, injects the live seams
through the `with_*` builders, and registers it three ways from the same `Arc`:

| Transport | Registration |
|---|---|
| Connect-HTTP `/rpc` | a `ServiceEntry` from `tddy_service::HostServiceServer::from_arc` |
| the LiveKit common room | the same `ServiceEntry`, pushed into the room's `MultiRpcService` |
| the local UDS socket | `HostServiceTonicAdapter`, a hand-written `#[tonic::async_trait]` impl — `tddy-codegen`'s `generate_tonic_adapter` is a stub — added to the **same** `Server::builder()` as `ConnectionService`, so a caller that reached `GetHostTooling` over the local socket before the split still does |

**Five of the eight methods route to a peer before the caller is authenticated.** A host question is
answered by the host it is about, and the daemon serving the call may not be that host — so the
routing decision is made from `daemon_instance_id` alone, ahead of the token. `tddy-daemon` hands
this crate the three things every such decision reads (the config, the roster and the token
resolver) as `ConnectionServiceImpl::routing_view`, and `to_tonic_status` is shared across the three
adapters so a refusal maps to the same code whichever service produced it.

**What the daemon kept.** `host_prompts::answer_before_expiry` is `pub` rather than `pub(crate)`
because `screen_sharing_service.rs` — which stays in `tddy-daemon` — consults it. That widening is
the seam's whole cost.

## `StreamHostStats` — host telemetry

One server-streaming RPC feeds the web's **Host Stats Footer**
([docs/ft/web/host-stats-footer.md](../../../docs/ft/web/host-stats-footer.md)) and the per-row
telemetry on the Hosts screen
([docs/ft/web/hosts-screen-telemetry.md](../../../docs/ft/web/hosts-screen-telemetry.md)) with
host-level readings for the daemon the client is addressing:

- `StreamHostStats(session_token)` → `stream HostStatsEvent`, carrying `cpu`, `disk`, `memory` and,
  where the platform provides one, `load`.

It authenticates `session_token` via the same GitHub → OS user path as every other method here, and is
addressed to the daemon participant directly (no `daemon_instance_id` payload — the LiveKit transport
already targets `daemon-{instanceId}`).

**Two cadences, one event.** The handler emits a full snapshot on subscribe, then runs two timers:
a fast tick (5 s) refreshing CPU, memory and load, and a slow tick (60 s) refreshing disk. Every
event carries the latest of all four, so a consumer never folds partial events together. Memory and
load ride the fast tick because they move on CPU's timescale; disk stays on the slow one because
enumerating mounts is the expensive read.

**A missing load average is reported as missing.** `sysinfo` implements `load_average()` only for
macOS, iOS, Linux, Android and FreeBSD, and returns an all-zero `LoadAvg` on every other target.
Forwarding those zeros would make an unsupported platform indistinguishable from an idle machine, so
`SysinfoHostStats::load_average` resolves the platform at compile time and returns `None` elsewhere;
the `load` block is then absent from the event rather than zeroed. Consumers render "no reading".

Backed by **`host_stats.rs`**: the `HostStats` trait — injected via
`HostServiceImpl::with_host_stats` so tests substitute a deterministic fake — with a
`sysinfo`-backed `SysinfoHostStats`. It reports:

| Method | Reading |
|---|---|
| `cpu_per_core_percent()` | utilization (0..100) of each logical core, core 0 first |
| `logical_cores()` | the core count, reported explicitly so a reader never infers it from a `per_core_percent` that is empty before the first sample |
| `memory()` | total and available physical memory, in bytes |
| `load_average()` | 1/5/15-minute averages, or `None` where the platform has none |
| `disk_for_project_dir()` | free/total capacity of the filesystem holding the default project directory |

A single long-lived `sysinfo::System` (constructed once with the service) backs CPU, memory and the
core list, so successive ~5 s-apart refreshes report real per-core deltas; the first sample reads ~0.
Disk resolution enumerates mounts and picks the filesystem whose mount point is the longest
**path-component** prefix of the project directory (`select_mount_for_path`), falling back to the
largest mount by capacity if none is a prefix. The default project directory resolves to
`$HOME/<repos_base_path_or_default>` (`DaemonConfig` has no explicit project-dir override today).

## `GetHostTooling` — the capability probe

One unary RPC reports what a host has **installed and configured**, as opposed to how busy it is —
backing the tooling cells on each Hosts row
([docs/ft/web/hosts-screen-tooling.md](../../../docs/ft/web/hosts-screen-tooling.md)):

- `GetHostTooling({session_token, daemon_instance_id})` →
  `{daemon_instance_id, git, github_cli, ssh_agent}`.
  `HostGitIdentity` carries `outcome`, `configured`, `user_name`, `user_email`, `failure_reason`;
  `HostGithubCli` carries `outcome`, `installed`, `authenticated`, `login`, `failure_reason`;
  `HostSshAgent` carries `outcome`, `reachable`, `keys` (`repeated SshAgentKey`) and
  `failure_reason`.

**Every outcome is distinguishable on the wire.** `ProbeOutcome` (`OK` / `FAILED` / `UNSUPPORTED`)
sits ahead of the findings in every block, so a probe that could not run is never collapsed into
"not configured" or "not installed". Collapsing them would put a fabricated fact in front of an
operator, and the two states send them to two different places. The enum is proto3, therefore open:
nodes extending this message add outcomes, and a consumer must treat only `OK` as licensing a
finding rather than listing the outcomes that do not.

**`login` is the host's `gh` login**, not the web session's user and not a `GITHUB_TOKEN` in some
environment. Three identities that can disagree, and the field's meaning is the narrow one.

**`reachable` is what separates an empty agent from an absent one.** Both arrive with an empty
`keys`, and they send an operator to two different fixes — load a key, or start an agent — so
whether anything answered is its own field rather than an inference from emptiness. `false` with
`OK` means no agent answered; `true` with an empty `keys` means one did, holding nothing.

**No `SshAgentKey` carries a path.** The agent knows a public key blob and a free-text `comment`,
commonly `user@host`, and does not know which file an identity came from. `fingerprint` is the
`SHA256:`-prefixed form `ssh-add -l` prints. Details, including what a supervised daemon can and
cannot reach: [host-tooling-probe.md § The ssh-agent](host-tooling-probe.md#the-ssh-agent).

**Routing precedes authentication.** `rpc_served_by_peer` runs before the `session_token` is
resolved, as it does for the roster RPCs and `ResolveStackBase`. Two reasons, and both matter:

- *A relay must not judge a peer's user mapping.* The token is verified by the daemon that **serves**
  the call. Authenticating first would refuse an operator whose GitHub user maps to an OS user on the
  host being probed but not on whichever host their browser happens to be talking to — the relaying
  daemon would be deciding a question that is not its to answer.
- *A question about another host, answered locally, comes back wrong in a way that reads right.* The
  serving daemon's own git identity under the addressed host's name is indistinguishable from a
  correct answer.

Once local, the handler resolves `session_token` → GitHub user → OS user by the same path as every
other endpoint, and runs the probe on the **blocking pool**: two of the three parts shell out and
wait, the third blocks on a Unix socket, and `gh auth status` can reach the network, so a runtime
worker is not parked for its duration. The
response stamps `local_instance_id_for_config`, so a relayed answer names the host that produced it.

Backed by **`host_tooling.rs`** — the `HostToolingProbe` trait, injected via
`HostServiceImpl::with_host_tooling` so tests substitute a deterministic double, with a
`SubprocessHostToolingProbe` that runs `git config --global --get` and `gh auth status` as the
host's OS user under one 5 s deadline. Program resolution, the deadline's kill-and-reap, and the rule
that unrecognised output is a probe failure are in
[host-tooling-probe.md](./host-tooling-probe.md).

## `AddHostKey` and the prompt channel

Four RPCs let an operator **load a key into a host's ssh-agent from the browser**, with the key's
passphrase carried **encrypted end to end** — see
[docs/ft/web/hosts-screen-add-key.md](../../../docs/ft/web/hosts-screen-add-key.md). The mechanism,
and every decision behind it, is [host-add-key.md](./host-add-key.md); what belongs here is the RPC
surface.

**The daemon asks and waits.** `StreamHostPrompts` (server-streaming) carries the question,
`AnswerHostPrompt` (unary) carries the answer, and `AddHostKey` (unary) is the operation that raises
one and consumes it. A server stream plus a unary reply rather than the ACP bidi stream: it mirrors
`StreamWorktreeStats` + `CalculateWorktreeSize`, correlation is an explicit `prompt_id` rather than
an envelope sequence, and the reply stays a unary call that can be transport-restricted the way
`mint_local_token` is.

**A prompt belongs to one operator.** Both the feed and the answer filter on the GitHub user whose
session raised it — not the mapped OS user, because `config.users[]` can map two GitHub users to one
OS user and they would then see and burn each other's prompts. Replayed to every subscriber the feed
would disclose the private-key path one operator named to every other operator watching.
Cross-operator answers get the `UnknownPrompt` rejection an unissued id gets, and are refused
*before* the prompt's one answer is spent.

**A prompt expires (120 s) and is answerable once.** An unanswered prompt cannot pin an `AddHostKey`
call forever, and a repeatable answer would turn the endpoint into a passphrase-guessing oracle
against one prompt.

**Only ciphertext crosses the wire.** `HostPromptEvent.host_public_key` is the host's published SPKI
DER; the browser encrypts under it with `SubtleCrypto` and `AnswerHostPrompt.encrypted_answer` is the
RSA-OAEP(SHA-256) result. The registry keeps no copy — the ciphertext moves through a `oneshot` to
the waiting `AddHostKey` — and the plaintext exists only inside the decrypt, in process.

**Two `AddHostKey` failures are deliberately indistinguishable.** "This host cannot decrypt your
answer" and "that passphrase did not unlock the key" share one arm and one message string. Told
apart, they are an adaptive RSA-OAEP decryption oracle — one clean bit per query against the host's
long-lived key, from any authenticated session. The real cause goes to the host's log only.

**The key is read as the operator, from inside their own home.** `subject` is free text from a
browser, so `host_private_key.rs` confines it lexically to the mapped user's home and reads the bytes
with that user's own privileges through `spawner::run_capture_as_user`. There is no `canonicalize`:
statting a caller-chosen path would restore the file-existence oracle the single `KEY_UNREADABLE`
refusal closes, and the privilege drop is the real boundary anyway. Absent, unreadable and malformed
share that one refusal; `KEY_OUTSIDE_HOME` stays distinct because it is decided by the caller's own
input and account and discloses nothing.

**`daemon_instance_id` is honoured on all four**, mirroring `GetHostTooling`: empty means the daemon
serving the call, and a request addressed to a host this daemon is not gets `invalid_argument` rather
than a locally served answer. A key silently loaded into the wrong host's agent is a worse version of
the failure that handler's own comment warns about. Note the caveat these RPCs share with
`GetHostTooling` and ~20 others: `classify_peer_route` compares the **routing** instance id while
`ListKnownHosts` publishes the **durable** one, equal only while
`daemon_instance_id_append_startup_timestamp` is false.

**The pump's teardown is not optional.** A prompt feed emits only while an operator is adding a key,
so the send failure a handler normally learns from is never attempted; the pump `tokio::select!`s on
`tx.closed()`, as `packages/tddy-codegen/docs/server-streaming.md` requires, or it leaks one task per
subscription. `pending_prompt_pump_count()` exists on the service so a wire-level test can see a leak
that is otherwise unobservable.

Injected for tests via `HostServiceImpl::with_host_prompts`, `with_host_keypair`,
`with_ssh_agent_key_adder` and `with_host_user_files` — the OS seams only. The prompt, the
encryption, the decrypt and the key unlock run for real.

## Testing

The suites travelled with the code they exercise:

| Level | Where |
|---|---|
| Unit | each module's own `#[cfg(test)]` — the probe classifications, the agent protocol, the registry's write policy |
| Handler | `src/host_add_key_handler_tests.rs`, `host_stats_handler_unit_tests.rs`, `host_tooling_handler_unit_tests.rs`, `known_hosts_handler_unit_tests.rs`, `ssh_agent_block_handler_tests.rs` — driven through the `host.HostService` trait, which is what the service now answers on |
| Integration | `tests/stream_host_prompts_rpc.rs` |
| Component | `packages/tddy-web/cypress/component/HostsScreen*.cy.tsx` |

## See also

- [host-registry.md](./host-registry.md) — the durable record, and the routing-id/durable-id split
- [host-tooling-probe.md](./host-tooling-probe.md) — the capability probe and its outcomes
- [host-add-key.md](./host-add-key.md) — the encrypted prompt channel end to end
- Sibling service: [`tddy-worktree-service`](../../tddy-worktree-service/docs/worktree-service.md)
- What stayed: [connection-service.md](../../tddy-daemon/docs/connection-service.md)
- Feature: [docs/ft/web/hosts-screen.md](../../../docs/ft/web/hosts-screen.md)
- [changesets/](./changesets/)
