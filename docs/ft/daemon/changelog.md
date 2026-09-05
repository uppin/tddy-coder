# Daemon product area changelog

**Merge hygiene:** [Changelog merge hygiene](../../dev/guides/changelog-merge-hygiene.md) — newest **`##`** first; **distinct titles** when two releases share a date; single-line bullets; do not edit older sections for unrelated work.

## 2026-09-05 — The daemon as a library, and its own settings

- **tddy-daemon**: the bootstrap moves out of `main()` into **`tddy_daemon::runtime`** (959 → 183 lines); `build` is assembly — it binds no socket, joins no room, spawns no task — so the binary and an embedding process assemble the **same roster**. Feature **[tddy-desktop-tauri.md](../desktop/tddy-desktop-tauri.md)**.
- **tddy-daemon**: **`daemon_config.DaemonConfigService`** reads and writes the daemon's YAML — secrets redacted, validation before any write, atomic rename, `restart_required` for what cannot apply live — with a supervisor that genuinely reconnects the LiveKit common room. Feature **[daemon-settings.md](daemon-settings.md)**.
- **tddy-core**: `LogConfig` and its nested types gained `Serialize` so `DaemonConfig` can be written back.

## 2026-08-30 — A workspace session's tools run inside a jail on the host holding the checkout

- **`sandbox` now means something on a `workspace` session.** It confined the agent and left the tools it called running on the host — fine while both are on one daemon, wrong the moment the codebase lives on another, which is why `sandbox` was refused on a split placement at all.
- **The jail holds the worktree and nothing else of that host**, and every exec tool goes through it: the remote `ExecuteTool` and `StreamExecuteTool` callers and a seeded roster agent's own loop alike, since all three already dispatch through one function.
- **Nothing falls back to the host.** An unsupported platform is `failed_precondition`, a jail that will not provision unwinds the session, and a session recorded as sandboxed whose jail this daemon no longer holds is refused rather than quietly served unconfined.
- **A jail no longer outlives its session.** `DeleteSession` stops it; previously the registry held the only handle and never released it, so a jailed runner kept running against a deleted checkout.
- **Every jail on macOS stopped holding the host's shared per-user temp base** — read and write, by three separate rules. On a stock host that is where temp files and application caches live, and a jailed `Shell` could read a host file outside its worktree.
- ⚠️ **Sandboxed `claude-cli` and `cursor-cli` sessions share that profile and are unverified end-to-end** — their suites fail here on an unrelated pre-existing panic. On a stock-`TMPDIR` host, confirm each still reaches a prompt and check its egress log for `Operation not permitted` under `/private/var/folders`. Cursor is the higher risk: its wrapper resolves its install through `realpath` and execs a bundled Node.
- ⚠️ **Split placement still refuses `sandbox`.** This is the primitive that refusal was waiting on, not its removal — `split-sandbox-orchestration` lifts it.

## 2026-08-30 — Agents read their project's own rules on a managed codebase

- **A managed session's agent now gets the target repo's own guidance**, chosen by a per-backend allow-list of globs rather than the fixed, project-specific list it used before — so a Cursor session receives `.cursor/`, and no session receives this repository's `docs/` and `skills/` conventions.
- **A split session gets it at all.** Its agent works on a host with no repository, and its context directory previously held only the managed-codebase notice; it is now populated from the codebase daemon before the agent starts, on start and on resume.
- **The managed-codebase notice moved to the top** of `CLAUDE.md` and `AGENTS.md`, so the rule that the codebase lives elsewhere is read before the project's own instructions, and it names which paths the sync owns and will replace.
- **A failed guidance fetch fails the session** rather than starting an agent that silently has no project rules.
- ⚠️ **Not yet continuous.** The directory is built at start and resume and is not updated again for the life of the session; the re-sync trigger is unwired and tracked in [docs/dev/TODO.md](../../dev/TODO.md).
- ⚠️ **Both halves of a split placement must be upgraded together** — start and resume now make two mandatory peer calls that an older codebase daemon cannot answer.
## 2026-08-30 — Session notifications became a bus, and Telegram one subscriber on it

- **Telegram was not *a* notification surface, it was *the* notification system** — one method classified the event, rendered the copy, resolved recipients and sent, so a second consumer could not be added without reaching into it.
- **A session notification is now a published event** carrying `{session_id, label, kind, source, text, at_unix_ms, os_user}`; subscribers declare which kinds they want, and the copy is rendered once at publish so every surface reads the same sentence.
- **Telegram takes only attention-worthy activity-status events**, so chat traffic did not grow: `ACTIVITY` exists for indicators, and presenter elicitations keep their own keyboard-bearing surface.
- **Activity alerts name a session the way the web drawer does** — repo basename → workflow goal → short id — so a chat message and a drawer row are finally matchable. The other Telegram surfaces still use the short-id label.
- **`StreamSessionNotifications`** is a daemon-level feed: one subscription serves a drawer of any size, live-only, and scoped to the caller's OS user — the bus is host-wide, so the relay is the only thing between one operator and another's sessions.
- ⚠️ **Removed `TelegramSessionWatcher::on_claude_cli_activity_status_changed`** and its dedupe field; every behaviour they pinned is re-pinned against the new subscriber and the existing acceptance suite passes unmodified.
- **Known limitation:** a workflow session *started from Telegram* does not yet publish presenter events; web-started and resumed sessions do.
- See [session-notifications.md](session-notifications.md).

## 2026-08-29 — A peer agent session's row says what its agent is doing

- **`SessionEntry` gains `agent_status` and `last_activity`**, inferred from the session's own conversation. They reuse the agent roster's `SessionAgentStatus` / `SessionAgentActivity` verbatim, so one badge renders a roster agent and a peer agent session alike, and they ride `ListSessions` rather than a second stream a reader would have to correlate against a row.
- **Neither is a new source.** The resolved transcript (`acp-transcript.jsonl` merged with `agent-activity.jsonl` — the view `StreamAcpReplay` already replays) seeds the newest signal once per session; the per-session `AgentActivityHub` broadcast keeps it current.
- **A live record and its replayed frame go through one mapper**, so a row cannot word the same call differently from the replay of it — which would surface as a session that rewords its own status when the daemon restarts.
- **A hook word of `Done` outranks a tool call left in flight.** A `running` row whose terminal record never arrived would otherwise pin the badge at `EXECUTING_TOOL` for the rest of the session's life. Otherwise a call in flight outranks the hook's `Running`: it is strictly more precise, and the only source of the call's name.
- **`UNSPECIFIED` stays "this daemon has nothing to say", never "idle"** — and is what every session type that runs no agent reports. Only `claude-cli` and `cursor-cli` sessions are tailed, the gate `ReportSessionStatus` already applies.
- **`activity_status` is unchanged** and keeps its own field: it is the raw hook word a worktree reports, and this is an inference built partly on top of it.
- Known: a seed that fails to read is not retried, the seed is read once per daemon lifetime per session, there is no cross-daemon inference, and `WAITING_FOR_INPUT` still has no producer but the hook word. See [agent-session-status.md](agent-session-status.md).

## 2026-08-29 — A tick attributes only the calls its measurement could have seen

- **A session room's poll tick now cuts the activity log before it measures the checkout.** Reading it afterwards admitted rows the measurement could not have seen: a tool writes its file and then records the completed call, so a call appended during the measure-and-announce span was stamped with the seq of a delta cut before its write existed.
- **The effect was an empty `DELTA_SCOPE_CALL` patch, permanently** — the bytes surfaced in the tick's residual instead, so nothing was lost on the wire, but a client that trusts a call's own delta never saw that edit (AC6). It showed up as an intermittent `git apply` failure in `session_room_livekit_acceptance`, because `git apply` answers an empty patch with `No valid patches in input`.
- **Rows appended after the cut wait for the next tick**, whose measurement is later than the write they describe. A tick boundary falling *between* a write and its record still mis-attributes that one call — microseconds against a 2 s poll, where the closed window was a large fraction of every interval; see [TODO](../../dev/TODO.md). Feature: [Session worktree sync](session-worktree-sync.md).
## 2026-08-29 — Waiting for an agent, instead of asking again

- **The main agent can now ask to be told when an agent is ready**, rather than checking and checking again. Attaching an agent returns before it can be used — its checkout may still be building — so the first prompt was often refused for a reason nobody could have anticipated. Asking repeatedly costs a full turn of the agent's own thinking each time, which made the cheapest question in the system one of the most expensive.
- **A wait always ends, and never silently.** It gives up after 30 seconds by default and can never be asked to wait longer than two minutes; when it does give up it hands back the current rows marked `timedOut`, because knowing an agent is *still* connecting is worth more than an error that throws that away.
- **Waiting on an agent that cannot recover ends immediately.** A checkout that has failed is not something waiting repairs, so the wait stops and reports it — otherwise the same failure would surface two minutes later wearing a timeout's clothes.
- **A misspelled agent id is refused, not answered.** Quietly settling one would report success for an agent that does not exist, and send the main agent on to prompt it. An agent that was there and then detached is different, and is reported by simply no longer appearing.
- **Asking to wait changes nothing for anyone who does not.** A plain read returns exactly what it always did, including when it names an agent — naming one chooses what to wait for, not what to show.

## 2026-08-29 — A session's room is enough to rebuild its worktree

- **The room now carries what the agent did, not just that something changed.** Each `AgentActivityRecord` is stamped with the commit it ran upon, the tick its delta belongs to and the paths it declared, then broadcast on `session.activity` — so a participant can attribute a change to a call and fetch exactly that call's patch. `worktree.activity` is unchanged.
- **Each poll tick stages a WIP tree and publishes it as an ordinary git ref.** `refs/tddy/session/{id}/wip` is a commit parented on the measured `HEAD`, staged through a scratch index so the agent's own index is never touched, and authored by a fixed `tddy-daemon@tddy.invalid` identity because the object is a machine-made snapshot rather than signed work. Under `refs/tddy/` so it is never a branch an agent sees or can push to. It is deleted when the room closes, under an interlock that stops a tick in flight republishing after the delete.
- **Reconciling is a `git fetch`, not a patch.** A mirror is a clone of the same repository, so recovering from a lost broadcast, a rejected patch or a hand-edited directory moves only the objects the clone lacks — where a cumulative patch would resend the whole dirty tree.
- **Two new server-streaming RPCs.** `StreamAgentActivityDelta` serves a tick's patch, scoped to one call's files or whole, with an unknown `call_id` distinguished from one that aged out of the ring. `StreamReadWorktreeFile` reads a worktree file as bytes — byte-exact and unbounded by the 1 MiB cap the unary reader truncated at.
- **New `tddy-session-sync` client.** `tddy-session-sync --session-id <id> --dest <dir>` maintains a fully managed local mirror of a session's worktree, committed history and uncommitted edits alike. The destination is owned: it carries a marker, local edits under it are discarded, and a non-empty unmarked directory — or one marked for another session — is refused rather than adopted. See [session-worktree-sync.md](session-worktree-sync.md).
- ⚠️ **Known limitations, recorded not hidden.** `StreamReadWorktreeFile` is served but not yet called by the client, so a path the session's `.gitignore` excludes is absent from the mirror rather than fetched. A room's first delta is numbered 0, which is also the wire's "no tick has measured this call" sentinel, so a session's first change reaches the mirror at the next reconcile rather than from the delta carrying it. The client's first attach fetches the WIP ref once without retrying, where the daemon's in-process sibling waits for it to appear. The client holds `LIVEKIT_API_SECRET`, a real widening of the trust surface forced by `MintLiveKitToken` granting only the common room. All four are in `docs/dev/TODO.md`.
## 2026-08-29 — A roster agent says what it is doing

- **Every attached agent now reports a status** — idle, running, executing tool, waiting for input, connecting or error — on the roster stream that already carries the agent list, so nothing new has to be subscribed to and no row can be shown a status for an agent it no longer holds.
- **The checkout outranks the conversation.** An agent whose clone is still being built reports `connecting` however idle its conversation looks: it refuses prompts, so calling it idle would offer an operator an agent that cannot answer. A clone this process has never measured reports `connecting` too, for the same reason it is never called ready.
- **"Nothing has been observed" is its own state, never "idle".** A restarted daemon reports `unknown` for every agent until a signal reaches it — a status is a fact about a running turn loop, and one restored from disk would claim a turn is in flight in a process that never started one.
- **Each row also carries what the agent was last seen doing** — one short line and the time it happened — which is the only useful thing to show for an agent that is idle.
- **A turn that fails leaves the agent idle, not in error.** It is still attached and still promptable; `error` is reserved for a broken checkout, and the summary is what says what happened.
- **An agent whose loop runs inside the sandbox reports itself** (`ReportAgentConversationState`), because the daemon is never asked to open a conversation for a seeded agent and would otherwise have nothing to say about one that is demonstrably working. It may only report conversation states: the checkout is the daemon's own to measure.
- **Known and unchanged:** a remote or sandbox-run loop reports `running` and `idle` but never `executing tool` (its tool calls happen where the daemon cannot see them), `waiting for input` has no producer yet, and the per-worktree activity hooks stay session-scoped because they carry no agent id.

## 2026-08-29 — A specialized agent can be seeded on any placement, and the index is built where the worktree is

- **`specialized_agents` is admissible on every codebase placement.** Naming an agent another daemon owns was refused on *every* start (`remote_agent_at_start_unsupported`), and naming *any* agent was refused on a split one — both on the premise that a peer is admitted to the session's room only once the spawn opens it. Claiming a clone opens that room itself, so the premise was false. A seed is now the same operation an attach is, performed before the spawn: each reference resolves from the daemon that owns it, an agent co-located with the authoritative worktree reads it directly, and an agent anywhere else gets the clone an attach would have given it. Where an agent runs decides **how the session is split across hosts**, never whether it may be selected. See [session-agent-roster.md § Seeding at start](session-agent-roster.md).
- **A seeded agent's tools are withdrawn at launch, not at the next resume.** A spawn's `--allowedTools`/`--disallowedTools` are fixed when the process starts, so the roster is written — and any clones claimed — *before* it: resolve placement → resolve records → claim clones → spawn from the persisted roster. A start that fails after a claim releases every clone, leaving no entry, no half-built checkout and no room membership, the contract a failed attach already kept.
- **`semantic_index` is served on a split placement, on the codebase host.** It indexes a worktree; the worktree is on the codebase host; so that host builds it for the session's `workspace` half. Nothing is indexed on the agent host, which has no checkout to index. ⚠️ Unchanged and still open: `SemanticSearch` cannot answer a query on *any* session shape — the query-side embedder is not wired into the tool engine — so what this delivers is the index built on the correct host. See [semantic-index.md](../coder/semantic-index.md).
- **`recipe` and `sandbox` stay refused on a split placement**, each naming its field. Both resolve a repository on the daemon running the agent, which a split session does not have.
- **The create-session pane stops withdrawing the agent picker and the Semantic index toggle** when a codebase host is chosen, and stops blanking both fields at submit — the request now equals what the form shows. The Recipe and Sandbox withdrawals stay, because those two are genuinely refused.
- ⚠️ **Behaviour change worth knowing:** a seeded agent declaring `replaces` is now refused on a session shape where the withdrawal cannot be enforced — an unsandboxed cursor-cli start, for one — where it was previously accepted and silently unenforced. This is the rule `AttachSessionAgent` has always applied, now applied to the seed as well.

## 2026-08-23 — A roster RPC is routed before it is looked up, and a withdrawn tool leaves the catalog

- **All seven roster RPCs classify `daemon_instance_id` before the session lookup.** A split session's roster lives beside its codebase on another daemon, so the agent host holds no session directory to look the request up in — served locally, `ListSessionAgents`/`Attach`/`Detach`/`Stream`/`Open`/`Prompt`/`Cancel` answered `NOT_FOUND` for a roster that exists. Routing first makes the same call answer identically whichever half of the pair it is addressed to. See [session-agent-roster.md](session-agent-roster.md).
- **A resumed split session refuses rather than relaunching with an unknown roster.** "The peer is unreachable" and "no agents are attached" both look like an empty roster, and reading the first as the second silently hands back every tool an operator withdrew. The resume issues a routed `ListSessionAgents` at the codebase host and fails with a message naming that host and session — the transport's own refusal names neither. See [remote-managed-worktree.md](remote-managed-worktree.md).
- **A withdrawn tool is withdrawn from the split agent's spawn as well as from the catalog.** A split session's *native* file tools are already hard-disabled, so its `mcp__tddy-tools__*` form is the only route its main agent ever had — and that form stayed callable through the permission prompt until `--disallowedTools` named it. Its `--allowedTools` also no longer hardcodes an empty roster.
- **`tools/list` and the call-time check now answer from one read of the roster**, so the advertised set always describes a single revision rather than an empty roster's conversation tools beside a populated one's withdrawals.
- **The in-jail roster follower paces reconnects on how long a pass lasted, not on whether it served anything.** The daemon publishes the current roster on subscribe, so any failure that gets as far as a subscription delivered a frame first — which pinned the delay at 500 ms and opened a fresh subscription on the daemon twice a second, forever, with nothing in the logs saying so. Unavailability still keys off a pass that served nothing, which is a different question. Once that throttle reaches its ceiling the roster stops reporting itself as current: throttling is right for a hot loop, but being throttled *and* authoritative would answer every call from a snapshot a whole ceiling old, and a withdrawal is still enforced across it.
- **A quiet roster stream keeps talking**, re-sending the revision it last sent, so a forwarded subscription is never terminated by a relay's idle deadline; that two keepalives fit inside the deadline is now a compile-time assertion rather than a comment, and that the deadline is not shorter than the subscriber's service threshold — which would make every relay-idle teardown read as churn — is pinned by a test, `tddy-tools` being a dev-dependency only.
- **A tool-replacing agent is refused on a `workspace` session no agent works in.** The check qualified every `workspace` session, and that is also what an operator's standalone checkout and an agent clone's mirror are — neither has an agent anywhere whose tools could be taken away, so the attach was accepted and the withdrawal enforced by nothing. The daemon placing a split session's worktree now names the agent half in the forwarded start (`StartSessionRequest.split_agent`), the `workspace` session persists it, and enforcement turns on that pairing rather than on the session type. A workspace session created before the field existed carries none, so its attaches are refused until the split session is restarted. See [remote-managed-worktree.md](remote-managed-worktree.md).
- **Web**: a host the common room advertises is read once, at its own identity, instead of twice. A bundle served by a daemon that did not name itself was read both over HTTP and as a LiveKit peer to itself; the rows deduped, but a healthy host could still be reported unreachable off the second answer. Every advertised host is still read — the pane's own client is the read only when it names its host, or when the room advertises none.
- **Web**: the Agent catalog reads every host in the common room and labels each row with the host it came from, and the new-session Agent picker does the same — a split session showed an empty catalog because it addressed the codebase host over an HTTP route that does not reach it. See [session-agent-roster.md](session-agent-roster.md).

## 2026-08-19 — An assistant you can start as is an assistant you can attach

- **`ListSubagents` advertises every def source the serving daemon resolves against** — its `<tddyhome>/agents/*.yaml` defs *and* its registry assistants (Models & Agents) — answered from the same `resolvable_agent_defs()` an attach resolves the id it is handed against. It read the agents directory alone, so an assistant created in Models & Agents could be started *as* and attached by typing its id, but appeared in no roster or specialized-agent opt-in picker on any host. See [session-agent-roster.md](session-agent-roster.md).
- **A cross-host assistant is now attachable at all.** A peer's defs are resolved through the peer's `ListSubagents`, so an assistant on another host was unresolvable rather than merely unlisted — a qualified id did not help.
- **Each daemon advertises its own assistants only**; a picker sees another host's because it fans the call out, not because any daemon forwards. Both ends of a common room must run a build that advertises them.
- **The listing stays keyless and now fails rather than half-answers.** A provider credential is attached on the session-start path alone. If reading the registry fails so does the RPC — no fallback to the YAML half, because a partial list is precisely how this read as "no agents exist" instead of "one source is broken"; the web renders a failing host as its own error row above the picker.

## 2026-08-18 — Session agent roster — attach any number of agents, from any daemon

- **A session's specialized agents are a revisioned roster mutated on a live session, addressable as `name@daemon_instance_id`**, replacing a fixed list of names frozen at spawn. The roster is the single source of truth on the wire (`SessionAgentRoster`, revisioned), at rest (`.session.yaml` `agents`/`agents_rev`), and in the in-jail registry (rebuilt from `StreamSessionAgents`). Every hardcoded builtin agent (`fastcontext`), `--fastcontext-*` flag, `TDDY_SUBAGENT` default, and action-author/coder `replaces` role is deleted. See [session-agent-roster.md](session-agent-roster.md).
- **A remote agent's def is resolved on its owning daemon; its loop runs there against a clone kept current by the in-process session-worktree-sync mirror, and its mutating tools proxy back to the facilitating daemon.** One clone per (session, owning daemon), shared by every agent that daemon owns. The checkout is a detached worktree under the owning daemon's sessions base, not a branch. See [session-agent-roster.md](session-agent-roster.md).
- **The owning daemon joins the session room with a facilitator-minted, scoped, short-TTL admission token** — it does not self-mint. A full-rejoin re-admit loop preserves clone state across reconnects; revocation (last detach, session delete) stops the mirror cleanly. `SessionAdmissionRegistry` + `SessionAdmissionService.AdmitOwningDaemon` implement the handshake.
- **Roster and conversation RPCs reach a jailed agent over the sandbox `SessionChannel`**, multiplexed by `request_id` (`RpcRequest`/`RpcStreamFrame`), so a lifetime-long roster stream does not occupy the positionally-paired `ExecuteTool` slot. `tddy-sandbox-runner` dispatches to an injected `HostRpcHandler`; `tddy-daemon`'s `DaemonRpcHandler` recovers the `Arc<ConnectionServiceImpl>` through a self-handle.
- **Clone readiness is pushed, not polled** (`ReportAgentCloneState`); `replaces`/`tools` are frozen into the roster entry at attach; a replaced tool is refused at call time rather than by relaunch; `RosterCurrency` distinguishes a roster that never received a frame (does not enforce withdrawal) from one that went stale (still enforces). See [session-agent-roster.md](session-agent-roster.md).
- **Web**: a fanned-out agent picker (per-daemon error isolation, qualified ids) and an Agent roster pane (four distinct states, add/detach with confirmation). See [session-agent-roster.md](session-agent-roster.md).

## 2026-08-15 — A split agent addresses the daemon that is actually in its room

- **A split session's agent is wired to the RPC identity hosting the room it joins** — the facilitating daemon — instead of the codebase daemon, which hosts no room and joins none. Since the move to per-session rooms, every tool call a split agent made waited out its timeout for a participant that would never arrive. See [remote-managed-worktree.md](remote-managed-worktree.md).
- **`TDDY_REMOTE_DAEMON_INSTANCE_ID` is unchanged and still names the codebase daemon**: it is the forwarding hint the room's host routes on to reach the checkout, not the identity to call. Naming the codebase host in both places is what conflated the two.
- **The room's host identity now travels with the room name** in the value the daemon resolves once and both the start and resume paths read, so an agent cannot be pointed at a participant that is not in the room it was given.
## 2026-08-15 — Rewriting the systemd units is a flag, not an env var

- **`./install --update-systemd-unit`** replaces the existing **`tddy-supervisor.service`** / **`.socket`** (or the **`--user`** **`tddy-daemon.service`**) with this script's templates. It replaces **`INSTALL_OVERWRITE_SYSTEMD_UNIT=1`**, which no longer does anything: what to install is an argument of the install, not part of its environment. Behavior is unchanged — without it an existing unit file is preserved. See [systemd-install.md](systemd-install.md#flags).

## 2026-08-14 — Every agent session gets a room of its own

- **A session that runs an agent now has a LiveKit room, `session-{session_id}`**, hosted by its **facilitating daemon** — the one running the agent. It opens and joins the room *before* the agent process is spawned, so being the first participant is a consequence of ordering rather than a race. See [session-room.md](session-room.md).
- **The room belongs to the session, not the checkout.** A session has exactly one agent-running daemon but its repo may live elsewhere; keying the room on the worktree would leave it homeless whenever the two are split.
- **A `workspace` session gets no room.** It runs no agent, so it has no facilitating daemon and nobody to serve.
- **File access is served in the room and hides where the files are.** Participants address one identity; a local repo is answered from disk, a remote one is forwarded to the codebase daemon over the peer routing that already existed. A split session's agent no longer addresses the codebase daemon directly.
- **Worktree activity is broadcast**, once, to every participant on the `worktree.activity` topic — the first non-RPC data-channel topic in the system. Events carry counts and the HEAD sha, never file paths or contents; reading the checkout is what the file-access RPCs in the same room are for. Receivers log them at `DEBUG` and derive no state from them yet.
- **New `GetWorktreeSnapshot` RPC** lets a facilitating daemon measure a checkout it does not hold, in one round trip per poll rather than three. A peer that cannot be reached costs the room a tick, never a wrong answer.
- **Room metadata carries the working-tree summary** — changed paths (capped at 200), `+`/`-` lines, branch, HEAD, attachments — so an agent joining mid-session knows where things stand without waiting for an event. Metadata is written *before* the event that announces it.
- **New config `session_room.poll_interval_ms` (2000) and `session_room.git_timeout_ms` (5000)**, both overridable by `TDDY_SESSION_ROOM_*`. Out-of-range values are rejected at load rather than clamped, and the git timeout is enforced on the child process so a wedged repo cannot leak blocking-pool threads.

## 2026-08-14 — A session's codebase can live on a different daemon than its agent

- **`StartSessionRequest.codebase_daemon_instance_id` is a second host axis.** `daemon_instance_id` says where the agent runs; this says whose filesystem holds the worktree. Empty or self-matching is exactly today's behaviour. See [remote-managed-worktree.md](remote-managed-worktree.md).
- **Restricted to `claude-cli`, and to managed codebase.** claude-cli is the one agent that can be *prevented* from reaching a local filesystem (`--allowedTools`/`--disallowedTools`); cursor-agent has no equivalent, so a split there would be guidance rather than enforcement.
- **`recipe`, `semantic_index` and `sandbox` are refused by name for a split** — each resolves a worktree on the daemon running the agent. The new-session form withdraws them rather than offering a choice whose only effect is a rejected request.
- **Atomicity needed two halves.** The codebase session's id is caller-chosen (`requested_session_id`, workspace-only) so a timed-out forward can still name what to tear down; and the split forward waits `spawn_worker_request_timeout + PEER_FORWARD_TIMEOUT`, because the peer's own worktree budget outlasts the ordinary forward deadline. With only the id, this daemon tore down while the peer went on building.
- **Paired teardown is idempotent, not credulous.** "The peer has no such session" completes the delete; unreachable refuses. The common room is checked to be *connected* first — a local no-room fault returns the same status code the peer uses for a missing session, and conflating them stranded the checkout.
- **New `StreamExecuteTool`** carries tool results in bounded frames, so a large `Read` or broad `Grep` cannot silently wedge on the transport's chunk framing. A stream ending without its final frame is an error and the partial result is discarded.
- **Fixed on the way:** `DeleteSession` removed worktrees only for `claude-cli`, so every `workspace` session leaked both its checkout and its `git worktree` registration — contradicting [remote-codebase-mode.md](remote-codebase-mode.md) criterion 3. `cursor-cli` still leaks; see `docs/dev/TODO.md`.
- **Known properties, deliberately:** the agent process holds the caller's session token (not scoped to exec tools), no LiveKit RPC carries a client-side deadline, and `tddy-coder --remote` remains unimplemented. All recorded in `docs/dev/TODO.md`.

## 2026-07-30 — A session creation on a branch another session owns is refused, not silently suffixed `-1`

- **`StartSession` refuses a `new_branch_from_base` request whose `new_branch_name` a session already owns**, instead of letting the worktree layer's suffixing retry silently produce `<branch>-1`. Nothing is created: no session directory, `changeset.yaml`, branch, worktree or remote push. See [session-branch-conflict.md](session-branch-conflict.md).
- **The refusal is a response field, not an RPC error** — `StartSessionResponse.branch_conflict` (field 5, `BranchConflict { branch, owner, suggested_branch_name }`) with an empty `session_id`, because `tddy_rpc::Status` carries no error details and `StartSession` is forwarded between hosts over LiveKit where only the response message is guaranteed intact.
- **Opt-in via `StartSessionRequest.on_branch_conflict` (field 30)**: `""` keeps today's suffixing for every existing caller, `"reject"` asks to be refused. Only a surface that can prompt an operator opts in — recipe hooks, PR-stack chain spawns and `RestoreSessionWorktree` keep suffixing.
- **Scope is session-owned branches only:** a branch that merely exists in git still suffixes, since there is no session to switch to and no second agent to add. `work_on_selected_branch` is never refused — it is the intent that deliberately joins an existing branch.
- **One ownership rule, three callers:** `branch_owner::find_session_owning_branch` (prefer active, then most recently updated) now backs `QueryBranch`, the `StartSession` guard and the Telegram spawn flow, so they cannot drift apart. See [connection-service.md § Branch-conflict guard](../../../packages/tddy-daemon/docs/connection-service.md#branch-conflict-guard-on-startsession).
- **Telegram gets the same guard at its own call site** (it bypasses `StartSession`): the base-branch pick sends a three-choice `tbc:` inline keyboard — switch to the owning session, add another agent on that branch, or take the suggested suffixed name — instead of going straight to the model picker. See [telegram-session-control.md](telegram-session-control.md).

## 2026-07-28 — Terminal replay is lazy (last-frame-first) + reconnect resumes by offset, and the PTY-over-RPC bridge is unified

- **`StreamTerminalOutput` sends the current last frame first** (tagged with absolute `start_offset`/`end_offset`/`at_oldest`), then tails live output — older history is fetched on demand as the user scrolls up via the new `GetTerminalHistory` RPC (forward chunking: `from_offset` → `until_offset` → `at_end`). See [terminal-sessions.md § Lazy replay & scroll-up history](terminal-sessions.md#lazy-replay--scroll-up-history).
- **`StreamTerminalOutputRequest` (and the bidi `StreamSessionTerminalIO` open frame) gain a `mode` (`StreamReplayMode`) + `from_offset`:** `TAIL` (default, first connect) sends the mode prologue + last-frame tail + PTY resize/drain + live; `FROM_OFFSET` (reconnect) sends chunked catch-up via `replay_from(from_offset, tip)` until `at_end` then live — no tail chunk, no resize/drain — so a reconnecting terminal receives only the bytes it missed, with no duplicate content.
- **The duplicated PTY-over-RPC bridge logic (replay/ACK/resize/drain/input-forward/exit) is unified into the shared `tddy-terminal-rpc` crate** behind `TerminalSession`/`TerminalSessionStore` async traits; the daemon (`DaemonTerminalSessionStore`) and coder (`CoderTerminalSessionStore`) adapt their `PtyHandle`s and delegate `StreamTerminalOutput`/`GetTerminalHistory`/`SendTerminalInput` to it. The proto additions are additive and backward-compatible (terminal RPCs stay on `ConnectionService`; new offset fields default to `0`).
- **`tddy-task::TerminalCapture` gains absolute byte-offset tracking** + `replay_last`/`replay_from(from_offset, until_offset, max_bytes)` forward replay.

## 2026-07-26 — GitHub access tokens are retained per login (BREAKING: re-login + writable `auth_storage`)

- **The daemon now keeps each real login's GitHub access token** (`auth_storage/github-tokens.json`, mode `0600`, published via temp-file + `rename`) so it can read PRs as that operator instead of falling back to a `GITHUB_TOKEN` the systemd unit never had. `auth_storage` finally has a reader. See [session-auth.md § GitHub access-token retention](session-auth.md#github-access-token-retention-added-2026-07-26).
- **BREAKING — every already-signed-in operator must log out and log in again:** the OAuth authorize scope widened from `read:user` to `read:user repo`, and an existing grant cannot be widened in place. Until then, GitHub-backed reads report themselves *unavailable* with a reason.
- **BREAKING — a configured `auth_storage` must be writable by the daemon user or the daemon refuses to start** (boot probe: create dir, write, remove). Retention is a hard login dependency, so an unwritable path would otherwise fail every login one operator at a time. Leaving `auth_storage` unset remains allowed (logins work; PR reads are *unavailable*). `./install` creates only the parent `/var/lib/tddy`.
- A login whose token cannot be retained **fails** rather than minting a half-login that looks signed in while every GitHub read is unavailable; the client-visible error names only the login, with the file path logged server-side. A stub/demo login retains nothing and is never a failure.
- `QueryBranch` gains a `remote` leg (`origin/<branch>` existence + sha) and its `pr` leg gains `unavailable` / `unavailable_reason`; a failed PR lookup degrades only that leg and never fails the RPC. `GetPrStatus` is unchanged and still served, but no longer called by the web.

## 2026-07-26 — Telegram model keyboards read the shared model catalog

- The Telegram **Claude** and **Cursor** model keyboards no longer carry their own copies of the model lists — they render `tddy_core::backend::claude_cli_models()` / `cursor_cli_models()`, so a catalog change reaches Telegram, the web dropdowns, and the CLI defaults at once. This also corrects the Claude keyboard's stale labels and grows the Cursor keyboard from 3 entries to the catalog's 5. See [telegram-session-control.md](telegram-session-control.md).
- An out-of-range `tcm:`/`tcur:` model index is now **rejected with an error** rather than resolving to some model; picking a Claude model still stores it in `changeset.yaml` and `.session.yaml` unchanged.

## 2026-07-23 — `SetProjectDefaultBranch` RPC + unified default-branch resolution

- New `ConnectionService.SetProjectDefaultBranch(project_id, main_branch_ref, daemon_instance_id)` RPC stores a project's default branch (`main_branch_ref`) in `projects.yaml`, validating the ref and project up front (`INVALID_ARGUMENT`/`NOT_FOUND`) before any write and peer-forwarding by target host like `AddProjectToHost` so the default is a property of the logical project across hosts. `ProjectEntry` now carries `main_branch_ref` so clients can read it. See [project-concept.md](project-concept.md) and [git-integration-base-ref.md](../coder/git-integration-base-ref.md).
- `effective_integration_base_ref_for_project` is unified: a stored ref wins outright; a legacy project (no stored ref) resolves its default **live** from the repository (`origin/master` → `origin/main` → `origin/HEAD`) rather than a hardcoded constant. `StartSession` uses the project's stored default when the client sends no override, so the default applies to web sessions, not only Telegram.

## 2026-07-06 — `ListAgentModels` RPC + tool-session `--model`

- New `ConnectionService.ListAgentModels(agent, daemon_instance_id)` RPC enumerates a backend's models on demand, shelling out to `tddy-tools list-models --agent <agent>` and returning `{models, default_model}`. Results are cached per (agent, daemon, OS user) with a short TTL — keyed by OS user because cursor/ACP catalogs are account-specific — and the probe runs as the user with its `current_dir` set to that user's home (the daemon cwd may be unreadable after setuid). A failed probe surfaces as an RPC error, never an empty catalog. See [tool-session-model-selection.md](../web/tool-session-model-selection.md).
- `StartSession` now threads `model` into the spawned **tool** (tddy-coder) session as `--model <m>` (previously claude-cli only), so a session runs with the operator-selected model.

## 2026-07-11 — Unprivileged daemon: Linux cgroups sandbox under `User=tddy`

- `./install --systemd` now generates the unit to run the daemon as the unprivileged **`tddy`** user by default, with **`Delegate=yes`** (a writable cgroup v2 subtree for per-session sandbox scopes) and **`AppArmorProfile=tddy-daemon`** (unprivileged user namespaces on hosts like Ubuntu 24.04 where they are AppArmor-restricted). The install creates the service user/group, chowns the log + auth-storage dirs, and ships + auto-loads (`apparmor_parser -r`) the AppArmor profile before starting the service. Overriding requires `INSTALL_OVERWRITE_SYSTEMD_UNIT=1` on an existing install; `INSTALL_DAEMON_USER=root` restores the previous multi-user setuid mode. See [systemd-install.md § Unprivileged service](systemd-install.md#unprivileged-service-linux-cgroups-sandbox).
- The Linux cgroups sandbox no longer requires root or a hardcoded `/sys/fs/cgroup`: it derives its delegated cgroup base from `/proc/self/cgroup` at runtime, optionally overridden by a new commented **`sandbox_cgroup:`** block in `daemon.yaml.production`. The userns precondition is now a functional probe (real `unshare` + uid/gid mapping), so a per-binary AppArmor grant is actually detected. It still fails fast (`FAILED_PRECONDITION`) rather than degrading to an unconfined process.

## 2026-07-16 — Shared PTY crate; Bash tool login shell

- The daemon's PTY runtime and registry moved into a shared `tddy-pty` crate (reused by tddy-coder for its bash terminal tabs); OS-user impersonation stays in the daemon over a thin adapter. See [terminal-sessions.md](terminal-sessions.md).
- The Bash tool (`StartTerminalSession`) now spawns the target user's passwd login shell instead of the daemon's `$SHELL`.

## 2026-07-12 — Fast session change: daemon-direct delete/signal + inspector `SessionEntry` bytes

- Session-scoped `ConnectionService` methods (tools, terminal control, VNC, screen-sharing) for a LiveKit-backed (tddy-coder) session are served by the coder's own LiveKit participant (`daemon-{instanceId}-{sessionId}`); the daemon no longer relays them. The daemon stays the bootstrap/directory authority (`StartSession`/`ConnectSession`/`ResumeSession` + `ListSessions`/`ListProjects`/…).
- `DeleteSession` / `SignalSession` are **daemon-direct**: the web calls them on `daemon-{instanceId}` with the caller's `session_token`; the coder is not on the path, so lifecycle control still works when the coder participant is stuck. Daemon errors surface verbatim. A contract test guards the daemon-direct path.
- `SessionEntry` gains `bytes_in` / `bytes_out` / `last_data_received_at`; the daemon populates them from the `GrpcSessionTerminal` traffic meter for claude-cli/cursor-cli/workspace sessions and reports zero/empty for stopped tddy-coder sessions, so the web inspector can render traffic for sessions with no LiveKit participant.
- Non-LiveKit (claude-cli / cursor-cli / workspace) sessions' `ConnectionService` path is unchanged.
- Feature: [terminal-sessions.md § Session-scoped RPC routing & daemon-direct lifecycle](terminal-sessions.md#session-scoped-rpc-routing--daemon-direct-lifecycle). PR [#297](https://github.com/uppin/tddy-coder/pull/297).
## 2026-07-06 — Cursor CLI sandbox parity with Claude CLI

- **`session_type = "cursor-cli"` + `sandbox = true`** succeeds on macOS (Seatbelt) and Linux (cgroups+namespaces) via `start_sandboxed_cursor_cli_session` and `tddy-sandbox-recipes::cursor_cli`; managed codebase, specialized subagents, and `TDDY_SOCKET` workflow wiring mirror claude-cli.
- In-jail `agent` spawns via direct `node index.js`; MCP config via `$HOME/.cursor/mcp.json` (no auto-injected `--approve-mcps` / `--force` / `--trust`).
- **`WaitingForInput`** remains unmapped (documented gap); sandboxed cursor-cli resume relaunch and jail Keychain auth are follow-ups. Feature: [cursor-cli-session.md](cursor-cli-session.md). PR [#287](https://github.com/uppin/tddy-coder/pull/287).

## 2026-07-05 — Cursor Agent CLI session

- **`session_type = "cursor-cli"`** — web **Create session** pane, RPC start/resume/connect, gRPC terminal I/O (same path as claude-cli), per-worktree **`.cursor/hooks.json`** → `ReportSessionStatus`, curated model catalog via **`ListAgentModels("cursor-cli")`**, Telegram **`/start-cursor`**. Sandbox and **`WaitingForInput`** are out of scope for v1. Feature: [cursor-cli-session.md](cursor-cli-session.md).

## 2026-07-04 — Durable web session (refresh token + RPC token gate)

- Session tokens split into a short-lived **access** token (5 min, unchanged, sent on every RPC) and a new long-lived **refresh** token (7-day sliding); `ExchangeCode` now mints both, and `RefreshSession` consumes a refresh token to mint a new access token plus a slid refresh token — fixing users being logged out whenever a device slept or a tab was backgrounded past the 5-minute access-token TTL
- Kind is enforced both ways: the daemon's per-RPC resolver rejects a refresh-kind token, and `RefreshSession` rejects an access-kind token, so neither credential can do the other's job; a token with no `kind` (pre-upgrade) still verifies as access-kind
- Web client gates RPC calls behind a request-time-fresh access token (`sessionTokenStore` + `authGateInterceptor`, single-flight refresh) instead of relying solely on a background timer, so the first call after waking transparently refreshes rather than failing; a top-bar indicator shows while a refresh is in flight
- Feature: [session-auth.md § Durable sessions](session-auth.md#durable-sessions-access--refresh-tokens)

## 2026-07-04 — Cross-daemon session authentication

- App session tokens are now stateless HMAC-SHA256-signed tokens (`v1.<payload>.<tag>` carrying the GitHub identity + `iat`/`exp`) signed with the shared `livekit.api_secret`, so a token minted by one daemon is verifiable by every daemon in the room — fixing `invalid or expired session` when switching daemons in the web UI and the silently-broken peer `ListProjects`/`StartSession`/`AddProjectToHost` forwarding paths
- New `RefreshSession` RPC re-mints a fresh token from a valid one; the web client refreshes every 4 min ahead of the 5-min TTL; logout is client-side; with no `livekit.api_secret` configured, auth is fail-closed (minting errors, every token rejected)
- Removed the previous per-daemon opaque-UUID / in-memory-map / `auth-sessions.json` session model (server-side logout and disk persistence are no longer meaningful for stateless tokens)
- Feature: [session-auth.md](session-auth.md)

## 2026-06-29 — Unified actions → tasks with optional sandbox execution

- New `tddy-actions` crate unifies subprocess, PTY, and pipeline execution behind `ActionSpec`; all long-running daemon work registers in the shared `TaskRegistry` (`ProcessRuntime`, `PtyRuntime`, `PipelineRuntime`)
- `actions.ActionService` RPC (`ListActionKinds`, `StartAction`, `GetAction`) complements existing `tasks.TaskService`; PTY terminals and action tasks appear in `ListTasks`
- Optional `ActionSpec.sandbox` runs confined process or runner-PTY actions via `sandbox_plan_builder` + `tddy-sandbox-recipes`; `SandboxSpec.cwd` and `extra_read_paths` wire working directory and read-only mounts; unsupported hosts return `failed_precondition`
- Session-action async jobs, `tddy-build` executor, fast tools, and sandboxed `tddy-coder` all share the same task model (`job_id == task_id`)
- Feature: [background-tasks.md](background-tasks.md), [terminal-sessions.md](terminal-sessions.md), [claude-cli-session.md](claude-cli-session.md). PR [#244](https://github.com/uppin/tddy-coder/pull/244)

## 2026-06-28 — Linux cgroups sandbox + cross-platform sandboxed sessions

- Sandboxed `claude-cli` sessions now run on **Linux** via a rootless jail (`tddy-sandbox-cgroups`): unprivileged user namespace + network namespace (loopback-only egress, forcing the in-jail `HTTPS_PROXY`) + private mount namespace + cgroup v2 limits
- `spawn_sandbox_runner` dispatches darwin (Seatbelt) / linux (cgroups) by target OS; on Linux the in-jail gRPC `SessionChannel` is served over **AF_UNIX** (survives the network namespace), dialed via `connect_sandbox_client_uds`
- Fails fast with `failed_precondition` when the host lacks unprivileged user namespaces or a writable cgroup v2 subtree — no silent unconfined fallback (production daemon runs as root systemd, where the restriction doesn't apply)
- In-jail runner + host-side relay extracted to a shared `tddy-sandbox-runner` crate; the CONNECT egress shim now waits for the host to attach before relaying (fixes an early-tunnel race)
- Sandbox opt-in exposed in the tddy-web new-session form (the `tddy-tools pty-relay --sandbox` CLI flag already existed)
- Feature: [claude-cli-session.md](claude-cli-session.md). Technical: [tddy-sandbox architecture](../../../packages/tddy-sandbox/docs/architecture.md). Known follow-ups: `pivot_root` filesystem write-confinement, config-driven cgroup limits

## 2026-06-27 — Darwin-sandboxed Claude CLI sessions (local gRPC)

- `StartSessionRequest.sandbox`: when `session_type:"claude-cli"` and `sandbox:true` on macOS, spawns `claude` inside Seatbelt via `tddy-tools sandbox-runner`; host daemon dials in-jail `SessionChannel` for PTY I/O, MCP tool exec, and LLM egress relay
- New crates `tddy-sandbox` (trait + context dir) and `tddy-sandbox-darwin` (SBPL profile + `sandbox-exec` spawn); non-macOS returns `failed_precondition`
- `ResumeSession` / `DeleteSession` stop the sandbox child and tear down the worktree; `.session.yaml` records `sandbox: true`
- Feature: [claude-cli-session.md](claude-cli-session.md). Technical: [connection-service.md](../../../packages/tddy-daemon/docs/connection-service.md#sandboxed-claude-code-cli-sessions), [tddy-sandbox architecture](../../../packages/tddy-sandbox/docs/architecture.md).

## 2026-06-26 — Browser DEBUG mask — config-driven terminal diagnostics

- `DaemonConfig.debug: Option<String>` threaded through `run_server` → `ClientConfig.debug` and served at `GET /api/config`; browser picks up the mask for scoped `[tddy]` console logging
- `dev.daemon.yaml` ships `debug: "tddy:term:*"` — covers all terminal namespaces; comment out or set `""` to disable

## 2026-06-26 — PTY terminal width fix — correct cols/rows on gRPC reconnect

- `StreamTerminalOutputRequest` now accepts `initial_cols`/`initial_rows`; when non-zero the daemon resizes the PTY, drains stale broadcast output, and triggers a SIGWINCH redraw before forwarding live output — eliminates 220-column garbling on browser reconnect
- `PtyHandle::send_input` strips `\x1b]resize;{cols};{rows}\x07` OSC escape sequences from stdin data and calls `PtyHandle::resize` transparently
- `kill_all()` on daemon shutdown: sends SIGTERM to every registered PTY process, waits up to 5 s, then SIGKILL for survivors; clears the registry
- Capture replay buffer limit raised 64 KB → 512 KB (more history for reconnecting clients)
- New `tddy-demo-tui` binary: reads PTY dimensions via TIOCGWINSZ, draws `DEMO TUI W={cols} H={rows}`, redraws on SIGWINCH — used as fake claude CLI in e2e tests
## 2026-06-26 — Single-screen terminal control mutex

- Per-session exclusive control lease in `ClaudeCliSessionManager`: first browser tab to attach becomes the controller; subsequent tabs see a **"Claim terminal"** overlay and cannot send input
- New `ConnectionService` RPCs: `ClaimTerminalControl` (unary, `steal` flag to evict the current holder) and `WatchTerminalControl` (server-stream, snapshot-then-delta via broadcast channel)
- `control_token` field added to `SessionTerminalInput`, `SignalSessionRequest`, `StartTerminalSessionRequest`, `StopTerminalSessionRequest`; input RPCs return `FAILED_PRECONDITION` when the token is wrong
- Uncontrolled sessions (no lease held) accept all inputs — fully backwards compatible

## 2026-06-25 — Multiple tools per session (Bash tool)

- A session can run multiple identified tools, not just `claude`: the main terminal is the reserved id `"main"` (kind `"claude-cli"`); on-demand **Bash** tools (kind `"bash"`) run `$SHELL` (fallback `/bin/bash`) in the worktree, no inputs
- New `ConnectionService` RPCs `StartTerminalSession` / `StopTerminalSession` / `ListTerminalSessions` (`TerminalSessionInfo{terminal_id, kind, pid}`); stopping `"main"` is rejected with `INVALID_ARGUMENT`
- Terminal I/O RPCs (`StreamSessionTerminalIO`, `StreamTerminalOutput`, `SendTerminalInput`) gain an optional `terminal_id` (empty ⇒ `"main"`); unknown id → `NOT_FOUND`
- RPC-only; no web UI integration in this release

## 2026-06-24 — Long-running background Tasks

- New `tasks.TaskService` gRPC service: `ListTasks`, `GetTask`, `WatchTask` (replay-then-live stream with `is_replay` flag), `CancelTask`, `SendInput`
- Every `ExecuteTool` invocation (fast Read/Write/etc.) registers a `Task` in the shared `TaskRegistry`; background Shell tasks observable via `WatchTask`; `Await` tool blocks on `TaskRegistry`
- VM image builds (Buildroot) are now cancellable tasks: `CancelTask` sends SIGINT to the `make` PID; build failure maps to `TaskStatus::Failed` (not `Completed`)
- Cooperative cancellation via `tokio_util::CancellationToken` per task; SIGTERM→SIGKILL escalation safety net after 5 s grace period
- Terminal tasks retained for 5 minutes then evicted; registry capped at 200 terminal tasks (oldest-first eviction)
- Minimal `/tasks` web page: 3-second polling, colour-coded status, Cancel button
- Feature: [daemon/background-tasks.md](background-tasks.md). Cross-package: [docs/dev/changesets.md](../../dev/changesets.md).

## 2026-06-21 — Demo goal Phase 2: daemon VM lifecycle RPCs

- `StartDemoVm` RPC: reads session's `demo-plan.md`, builds `DemoVmConfig`, spawns `QemuDemoVm::boot()` background task, tracks handle per session
- `StopDemoVm` RPC: removes handle and calls `shutdown()` via monitor socket
- `GetDemoVmStatus` RPC: returns `DemoVmState` (`BOOTING`/`RUNNING`/`STOPPED`/`ERROR`), `ssh_host_port`, and `share_url`
- Feature: [coder/demo-goal.md](../coder/demo-goal.md). Cross-package: [docs/dev/changesets.md](../../dev/changesets.md).

## 2026-06-15 — RPC Playground

- **Backend**: `grpc.reflection.v1.ServerReflection` service (`reflection_service.rs`, vendored `reflection.proto`, embedded `FileDescriptorSet` from build.rs); `MultiRpcService::service_names()` in `tddy-rpc`; `reflection_entry_from()` helper registered in daemon `main.rs` and all `tddy-coder` `MultiRpcService` sites; daemon spawns a dedicated `LiveKitParticipant` in the common room (identity `daemon-{id}`) so the playground reaches it via data channel.
- **Frontend** (`tddy-web`): `/rpc-playground` route (hash-based, no 404 on reload); participant picker filtered to `coder`-role only; `RpcPlaygroundScreen` + `RpcPlaygroundAppPage`; request editor (builder ↔ raw JSON, synced); streaming panel; `invoke.ts` auto-injects `sessionToken` when the request type has a `session_token` field.
- **Reflection codegen** (`tddy-livekit-web`): `reflection_pb.ts` generated via buf; `createLiveKitTransport` used for all reflection + invocation calls (avoids fetch streaming body limits).
- **Test infrastructure**: Cypress tests in `tddy-livekit-web` and `tddy-web` auto-start a Docker LiveKit container when `LIVEKIT_TESTKIT_WS_URL` is not set.
- Feature: [rpc-playground.md](rpc-playground.md). Cross-package: [docs/dev/changesets.md](../../dev/changesets.md).

## 2026-06-14 — Remote-codebase mode

- **Remote daemon**: workspace sessions (`session_type:"workspace"`) with git worktree, no PTY; `ExecuteTool` (Read, Write, StrReplace, Delete, Grep, Glob, Shell, Await, SemanticSearch, ReadLints) + `ListExecTools` RPCs; `contain_path` security; background shell jobs + Await polling.
- **Relay daemon** (`--relay`): joins LiveKit common room; `forward_to_peer` + per-peer `RpcClient` cache routes `ExecuteTool`/`ListExecTools` to named remote peer; `IdleTimeoutTracker` triggers graceful shutdown after idle timeout; external oneshot shutdown channel in `run_server`.
- **`tddy-tools remote`**: `list-tools` via `ListExecTools` Connect POST; `start-session`, `connect-session`, `sync-context` subcommands; lazy relay daemon startup via `ensure_relay_daemon`.
- **`tddy-coder --remote`**: `--remote-daemon-url`/`--remote-session-token`/`--remote-daemon-id` flags; `run_remote` shells out to `tddy-tools remote list-tools`, builds dynamic `mcp__tddy-tools__*` allowlist, runs free-prompting workflow with remote ctx keys and read-only local ctx dir.
- Feature: [remote-codebase-mode.md](remote-codebase-mode.md). Cross-package: [docs/dev/changesets.md](../../dev/changesets.md).

## 2026-06-13 — Claude Code CLI permission mode selection

- **`tddy-service`**: `StartSessionRequest.permission_mode` (proto field 14, string).
- **`tddy-daemon`**: `build_claude_argv` appends `--permission-mode <mode>` (5th param; `None`/empty/whitespace → `auto`); `ClaudeCliSessionManager::start()` accepts `permission_mode: Option<&str>` (6th param); `connection_service::start_session` extracts and trims `req.permission_mode`, passes through `start_claude_cli_session` → `manager.start` → `build_claude_argv`. Tests: `claude_cli_permission_mode_acceptance` (16 tests). **`tddy-tools`**: `pty-relay --permission-mode` optional CLI arg wired into `StartSessionRequest`. Feature: [claude-cli-permission-mode.md](claude-cli-permission-mode.md). **Cross-package**: [docs/dev/changesets.md](../../dev/changesets.md).
## 2026-06-13 — Per-worktree hooks: claude-cli session activity status

- **`tddy-core`**: **`session_activity`** — **`SessionActivityStatus`** enum (`Started`, `Running`, `ExecutingTool`, `WaitingForInput`, `Done`, `Ended`) with `as_wire()`/`from_wire()`; **`activity_status_from_hook(event, notif_type)`** maps Claude Code hook events; **`HookEvent`** serde struct + `parse_hook_event`; 15 unit tests. **`claude_hooks`** — **`HookCommandParams`**, **`build_claude_hooks_settings()`** builds the 6-event settings JSON; 4 unit tests. **`session_metadata`** — **`activity_status: Option<String>`** and **`hook_token: Option<String>`** on **`SessionMetadata`** (serde-default, backward-compat); **`update_activity_status()`** read-modify-write helper.
- **`tddy-service`**: **`connection.proto`** — **`ReportSessionStatus`** RPC; **`ReportSessionStatusRequest/Response`** messages; **`SessionEntry.activity_status`** (field 15); **`StartSessionRequest.initial_prompt`** (field 13).
- **`tddy-daemon`**: **`connection_service`** — **`report_session_status`** handler (path-traversal guard, `os_user` sessions_base, constant-time `hook_token` check, `update_activity_status`); hook wiring in **`start_claude_cli_session`** (UUID token, resolves `tddy_tools_path`/`daemon_url`, writes `<worktree>/.claude/settings.local.json`, persists `hook_token` in metadata); **`session_list_enrichment`** surfaces `activity_status` via **`ListSessions`**. **`config`** — `tddy_tools_path`, `daemon_url` on **`ClaudeCliConfig`**. 6 handler unit tests. **`ClaudeCliSessionManager`** extracted as constructor parameter; `build_claude_argv()` helper; **`PtyHandle::resize()`** + `current_size`.
- **`tddy-tools`**: **`session-hook`** subcommand — reads stdin JSON, maps event, POSTs **`ReportSessionStatus`**; fail-quiet (always exit 0, 2s timeout); 5 CLI acceptance tests. **`pty_relay`** — `encode_resize()` corrected to OSC format `\x1b]resize;{cols};{rows}\x07`.
- **Feature docs**: [claude-cli-session.md](claude-cli-session.md#session-activity-status-via-per-worktree-hooks); technical: [connection-service.md](../../../packages/tddy-daemon/docs/connection-service.md#claude-code-cli-sessions). **Cross-package**: [docs/dev/changesets.md](../../dev/changesets.md).

## 2026-06-06 — Session chaining: stable parent id in Telegram callback

- **`tddy-daemon`**: **`telegram_session_control`** — **`tcp:`** chain callback format changed from `tcp:<idx>|s:<child>` to `tcp:p:<parent_tail8>|s:<child>` (last 8 chars of parent session id); **`handle_chain_parent_callback`** scans candidates by tail instead of index position — stable across session churn between keyboard render and tap; **`session_tail8()`** helper; **`parse_telegram_chain_parent_callback`** signature updated to return `(String, String)`. Unit tests: **`parse_chain_workflow_prompt`** (strip/trim/wrong-prefix), **`parse_telegram_chain_parent_callback`** round-trip, empty-tail rejection, **`session_tail8`** boundary cases. **Cross-package**: [docs/dev/changesets.md](../../dev/changesets.md).

## 2026-06-06 — Claude Code CLI session type

- **`tddy-service`**: **`connection.proto`**: `session_type` (field 7) and `model` (field 8) on **`StartSessionRequest`**; **`StreamSessionTerminalIO`** bidi RPC, **`SessionTerminalInput`** and **`SessionTerminalOutput`** messages.
- **`tddy-core`**: **`SessionMetadata`** gains **`session_type: Option<String>`** and **`model: Option<String>`**; **`InitialToolSessionMetadataOpts`** extended; **`write_initial_claude_cli_session_metadata()`** convenience wrapper; backward-compatible YAML serde (missing fields → `None`). Tests: **`claude_cli_metadata_round_trip`**.
- **`tddy-daemon`**: **`claude_cli_session`** — **`ClaudeCliSessionManager`** (tokio-channel subprocess registry; `start()` spawns `claude --model <m> --session-id <session_id>`; `resume()` relaunches in same worktree; background exit monitor; broadcast stdout, mpsc stdin); **`connection_service`** — `start_session` claude-cli branch (worktree creation, metadata write, `ClaudeCliSessionManager::start`, empty LiveKit response), `connect_session` early return for claude-cli, `stream_session_terminal_io` bidi handler, `delete_session` worktree cleanup; **`session_list_enrichment`** populates `agent = "claude-cli"` and `model` from metadata; **`config`** optional `claude_cli.binary_path`. Tests: `claude_cli_session_acceptance` (`claude_cli_session_metadata_fields_persisted`, `claude_cli_session_livekit_fields_empty`, `claude_cli_session_enrichment_reads_from_metadata`, `claude_cli_session_resume_relaunches_in_worktree`, `claude_cli_start_session_requires_model`).
- **`tddy-web`**: **`ConnectionScreen`** session type selector + model dropdown; **`ConnectedClaudeCliTerminal`** bidi gRPC stream component; **`GhosttyTerminalGrpc`** (`GrpcStream` interface, output buffer before ready, OSC resize, optional chrome bar); **`constants/claudeCliModels.ts`** (`CLAUDE_CLI_MODELS`, `isClaudeCliSession`); `multiSessionState` extended with optional `claudeCli` discriminant. Tests: `claudeCliModels.test.ts`, `GhosttyTerminalGrpc.cy.tsx`.
- **Feature docs**: [claude-cli-session.md](claude-cli-session.md); technical: [connection-service.md](../../packages/tddy-daemon/docs/connection-service.md), [web-terminal.md](../web/web-terminal.md). **Cross-package**: [docs/dev/changesets.md](../../dev/changesets.md).

## 2026-05-02 — Session chaining (`/chain-workflow`, `tcp:` callbacks, chain base merge)

- **`tddy-daemon`**: **`handle_chain_workflow`** creates the child session, lists parent rows via **`parent_candidates_page_for_chain_picker`**, sends **`tcp:<idx>|s:<child_session_id>`** buttons, then the recipe keyboard; **`parse_telegram_chain_parent_callback`**, **`handle_chain_parent_callback`** (child id validated with **`validate_session_id_segment`**), **`CB_TELEGRAM_CHAIN_PARENT`**. **`telegram_bot`** routes **`tcp:`** through **`maybe_dispatch_tcp_chain_parent_callback`** (answers the callback query and sends **`telegram_workflow_error_message`** text on failure, same pattern as **`intent:`** / **`tp:`** workflow steps); **`workflow_callback_gate_authorized`** centralizes allowlist checks on workflow callbacks. **`spawn_telegram_workflow`** runs **`merge_chain_integration_base_with_explicit_operator_overrides`** inside **`tokio::task::spawn_blocking`** when **`.session.yaml`** carries **`previous_session_id`**. Integration **`telegram_chain_workflow_shows_parent_pick_first`**, **`telegram_chain_parent_tap_persists_previous_session_id_on_child`**, **`parent_candidates_page_for_chain_picker_excludes_child_and_caps_page`**, **`telegram_chain_parent_callback_rejects_invalid_child_session_id_segment`**; **`telegram_bot_rs_dispatches_chain_workflow_command`**; **`session_chaining_phase2_acceptance`** / **`session_chaining_phase2_unit`** guard wiring and merge behavior.
- **`tddy-core`**: **`session_chain`** (**`resolve_chain_integration_base_ref_from_parent_session`**, **`integrate_chain_base_into_session_worktree_bootstrap`**); parent **`repo_path`** required when parent **`changeset.yaml`** names a branch; **`SessionMetadata.previous_session_id`** / **`InitialToolSessionMetadataOpts`**; **`slash_menu_entries`** includes **`/chain`**. Tests: **`session_chain_acceptance`**, **`session_metadata::chain_child_metadata_records_previous_session_id`**, **`session_chain`** unit tests.
- **`tddy-tui`**: **`ViewState::chain_workflow_parent_picker_active`** clears when **`AppMode`** is not **`FeatureInput`** (**`on_mode_changed`**). Tests: **`chain_phase2_acceptance`**, **`chain_phase2_unit`**.
- **Feature docs**: [telegram-session-control.md](telegram-session-control.md), [git-integration-base-ref.md](../coder/git-integration-base-ref.md), [session-layout.md](../coder/session-layout.md), [feature-prompt-agent-skills.md](../coder/feature-prompt-agent-skills.md). Optional follow-ups: [2026-05-02-changeset-session-chaining.md](../../dev/1-WIP/2026-05-02-changeset-session-chaining.md). **Cross-package**: [docs/dev/changesets.md](../../dev/changesets.md).

## 2026-05-02 — Telegram tracked session gate and chat traffic logs

- **`tddy-daemon`**: **`telegram_tracked_session`** — per-chat optional **`session_id`** binding (**`SharedTelegramTrackedSessionCoordinator`**) shared with **`TelegramSessionWatcher`** and **`telegram_session_control`**; presenter **`ModeChanged`** workflow keyboards suppress under **no / mismatched** tracking with **Enter session** fallback; **queue promotion replay** bypasses the gate; **Enter** binds + **elicitation replay**; clears on **WorkflowComplete**, matching **delete**, or explicit per-chat clear. Structured **`telegram_traffic`** logs on **`tddy_daemon::telegram`**; inbound message/callback summaries on **`tddy_daemon::telegram_bot`**. Integration **`telegram_tracked_session_acceptance`**; concurrent + multi-select suites bind tracking where full keyboards are asserted.
- **Feature docs**: [telegram-session-control.md](telegram-session-control.md), [telegram-notifications.md](telegram-notifications.md). **Package**: [telegram-notifier.md](../../packages/tddy-daemon/docs/telegram-notifier.md), [changesets.md](../../packages/tddy-daemon/docs/changesets.md). **Cross-package**: [docs/dev/changesets.md](../../dev/changesets.md).

## 2026-05-02 — Telegram MultiSelect shortcuts (`eli:mn:` / `eli:mr:`)

- **`tddy-daemon`**: **`telegram_multi_select_shortcuts`** — compact **Choose none** (**`eli:mn:`**) and **Choose recommended** (**`eli:mr:`**, when **`recommended_other`** is present) keyboards within Telegram’s **64-byte** **`callback_data`** limit; **`TelegramSessionWatcher`** **`MultiSelectShortcutElicitationMeta`** cache keyed by Telegram chat plus session (**`recommended_other`** for **Choose recommended**); **`telegram_bot`** dispatches **`eli:mn:`** / **`eli:mr:`** through **`authorized_elicitation_surface_gate`**; **`handle_elicitation_multi_select_shortcut`** submits **`PresenterIntent::AnswerClarificationMultiSelect`**. Integration tests **`telegram_multi_select_acceptance`**; **`telegram_concurrent_elicitation_integration`** asserts primary-keyboard alignment for MultiSelect shortcuts.
- **`tddy-core`**: Presenter rejects **`AnswerClarificationMultiSelect`** with empty indices and no **Other** text when **`allow_other`** on the clarification is **false**.
- **`tddy-service`**: **`ClarificationQuestionProto.recommended_other`** on MultiSelect wire events.
- **Feature docs**: [telegram-session-control.md](telegram-session-control.md), [telegram-notifications.md](telegram-notifications.md). **Cross-package**: [docs/dev/changesets.md](../../dev/changesets.md).

## 2026-04-11 — Operator OAuth loopback tunnel (daemon)

- **`tddy-daemon`**: **`oauth_loopback_tunnel`** — **`TcpListener`** on operator **`127.0.0.1:{callback_port}`**, **`RpcClient::start_bidi_stream`** **`loopback_tunnel.LoopbackTunnelService`/`StreamBytes`**, **`pick_daemon_oauth_target`** over common-room **`daemon-*`** metadata; **`run_oauth_tunnel_supervisor_follow_room_slot`** with **`livekit_peer_discovery`**; **`codex_oauth_participant_metadata`**. Package **[oauth-loopback-tunnel.md](../../packages/tddy-daemon/docs/oauth-loopback-tunnel.md)**; feature **[codex-oauth-relay.md](codex-oauth-relay.md)**, **[livekit-peer-discovery.md](livekit-peer-discovery.md)**. **Cross-package**: [docs/dev/changesets.md](../../dev/changesets.md).

## 2026-04-11 — LiveKit common-room peer discovery and cross-daemon StartSession

- **`tddy-daemon`**: Module **`livekit_peer_discovery`** — JSON metadata advertisement, **`CommonRoomPeerRegistry`**, **`LiveKitEligibleDaemonSource`**, **`LiveKitDiscoveryHandles`**, background join/sync for **`livekit.common_room`**, **StartSession** forward via **`tddy_livekit::RpcClient`** to peer identity; **`local_instance_id_for_config`** shared with **ConnectionService**; **`TDDY_PROJECTS_DIR`** test hook documented on **`projects_path_for_user`**. Integration tests **`livekit_peer_daemons_acceptance`**, **`multi_host_acceptance`** (remote routing). **`tddy-livekit`**: **`RpcClient::new_shared`** (**`Arc<Room>`**).
- **Feature doc**: [livekit-peer-discovery.md](livekit-peer-discovery.md) (includes operator / CI notes). **Web**: [web-terminal.md](../web/web-terminal.md) (eligible daemons, host ordering). **Cross-package**: [docs/dev/changesets.md](../../dev/changesets.md).
## 2026-04-11 — Connection service: project entries with owning daemon and peer row hook

- **`connection.proto`**: **`ProjectEntry.daemon_instance_id`** identifies the registry row’s owning daemon.
- **`tddy-daemon`**: **`list_projects`** merges local disk projects with **`EligibleDaemonSource::peer_project_entries(session_token)`**; the default **`EligibleDaemonSource`** supplies an empty peer list. Integration test **`list_projects_multi_daemon_aggregation`** exercises merge and per-row **`daemon_instance_id`**. Cross-package note: **[docs/dev/changesets.md](../../dev/changesets.md)**; web feature doc: **[web-terminal.md](../web/web-terminal.md)** (eligible daemons / **`ListProjects`**).

## 2026-04-06 — Telegram user ↔ GitHub identity (library)

- **`tddy-daemon`**: Module **`telegram_github_link`** — **`TelegramOAuthStateSigner`** (HMAC-SHA256 OAuth **`state`** bound to **`telegram_user_id`**), **`TelegramGithubMappingStore`** (JSON on disk, atomic replace), **`resolved_os_user_for_telegram_workflow`**, **`complete_telegram_link_via_stub_exchange`** (**`StubGitHubProvider`**). **`TelegramSessionControlHarness::with_telegram_github_link`** optional mapping path; **`handle_start_workflow`** rejects unlinked Telegram users when that path is set (error text references **`/link-github`** / web OAuth). Dependencies: **`base64`**, **`hmac`**, **`sha2`**, **`subtle`**.
- **Feature doc**: [telegram-session-control.md](telegram-session-control.md). Package: [telegram-github-link.md](../../packages/tddy-daemon/docs/telegram-github-link.md), [changesets.md](../../packages/tddy-daemon/docs/changesets.md).

## 2026-04-06 — Telegram: concurrent elicitation (one chat, active token)

- **Coordinator:** **`ActiveElicitationCoordinator`** maintains a per-chat FIFO queue of workflow sessions; the head session owns the **active elicitation token** for Telegram interactive surfaces.
- **Outbound:** **`TelegramSessionWatcher`** registers elicitation requests on **`ModeChanged`**; sessions that are not primary for a chat receive a **deferred** text notice without a competing full **`eli:s:`** inline keyboard.
- **Inbound:** **`telegram_bot`** applies the same **active-token** policy to **`eli:s:`**, **`eli:o:`**, **`eli:mn:`**, **`eli:mr:`**, and **`doc:`** callbacks; **`/answer-text`** and **`/answer-multi`** check the active session before **`PresenterIntent`** calls. **`telegram_session_control`** advances the queue after completion on select, Other follow-up, multi-select shortcuts, applicable document-review actions, and successful text/multi answers.
- **Observability:** Deep per-chat queues trigger a **warning** log at a fixed depth threshold.
- **Feature docs:** [telegram-session-control.md](telegram-session-control.md), [telegram-notifications.md](telegram-notifications.md). Package: [telegram-notifier.md](../../packages/tddy-daemon/docs/telegram-notifier.md), [changesets.md](../../packages/tddy-daemon/docs/changesets.md).

## 2026-04-06 — Telegram `/start-workflow`: branch/worktree intent step

- **`tddy-daemon`**: After a recipe is saved (excluding **More recipes** follow-up), the bot prompts for **branch/worktree intent** (**New branch + worktree** vs **Work on existing branch**). The choice is written to **`changeset.yaml`** under **`workflow.branch_worktree_intent`** (`new_branch_from_base` / `work_on_selected_branch`) before project selection. Inline **`callback_data`** uses compact **`intent:nb|s:<session_id>`** and **`intent:ws|s:<session_id>`** so payloads stay within Telegram’s 64-byte limit with a UUID session id.
- **Feature doc**: [telegram-session-control.md](telegram-session-control.md). Package history: [changesets.md](../../packages/tddy-daemon/docs/changesets.md).

## 2026-04-05 — Telegram: inbound session control, PresenterIntent, elicitation UX

- **Inbound control**: Daemon runs **`telegram_bot`** (teloxide long-polling) when Telegram is configured and **`sessions_base`** resolves. Commands include **`/start-workflow`**, **`/sessions`**, **`/delete`**, **`/submit-feature`**, **`/answer-text`**, **`/answer-multi`**; callbacks cover session list, recipe/project/agent picks, document review (**`doc:`**), and elicitation select (**`eli:s:`**). **`TelegramSessionControlHarness`** and integration tests exercise the library; production uses **`TeloxideSender`** with the same bot as outbound notifications.
- **PresenterIntent**: **`presenter_intent.proto`** and **`tddy-daemon::presenter_intent_client`** forward answers and document actions to the child **`tddy-coder`** on localhost gRPC.
- **Outbound notifications**: **`ModeChanged`** for document review / markdown viewer sends **full document body** (chunked), then **Approve** / **Reject** / **Refine** (and related) inline actions. **`Select`** clarification sends a **numbered option list** in the message body, **numeric** inline buttons, and a **post-tap confirmation** with the full chosen option text. Dedupe for identical **`ModeChanged`** payloads per session is unchanged.
- **Formatting**: Styled text must follow Telegram **[message entities](https://core.telegram.org/api/entities)** rules (UTF-16 code units for offsets and lengths where applicable).
- **Feature docs**: [telegram-session-control.md](telegram-session-control.md), [telegram-notifications.md](telegram-notifications.md).

## 2026-04-05 — Telegram extended recipe keyboard: `review`

- **`tddy-daemon`**: **`RECIPE_MORE_PAGE`** includes the **`review`** workflow recipe name (same normalization rules as other CLI recipe strings).
- **Cross-reference**: [workflow-recipes.md](../coder/workflow-recipes.md) (**Selecting a recipe**); package [changesets.md](../../packages/tddy-daemon/docs/changesets.md).

## 2026-04-04 — Session elicitation: Telegram `ModeChanged` + `ListSessions` flag

- **`connection.proto`**: **`SessionEntry.pending_elicitation`** (field **14**).
- **`tddy_core`**: **`SessionMetadata.pending_elicitation`** in **`.session.yaml`** (serde default **`false`**).
- **`tddy-daemon`**: Module **`elicitation`** — list flag from metadata; **`TelegramSessionWatcher::on_server_message`** handles **`ModeChanged`** with dedupe and generic approval/input Telegram lines; **`session_list_enrichment`** sets the proto field. Tests: **`telegram_notifier`** acceptance unit tests, **`list_sessions_enriched`**, **`session_list_enrichment`** unit test.
- **Feature docs**: [telegram-notifications.md](telegram-notifications.md) (Presenter stream: elicitation); [web-terminal.md](../web/web-terminal.md) (pending elicitation on rows). Package: [telegram-notifier.md](../../packages/tddy-daemon/docs/telegram-notifier.md), [changesets.md](../../packages/tddy-daemon/docs/changesets.md). Cross-package: **[docs/dev/changesets.md](../../dev/changesets.md)**.

## 2026-04-05 — Documentation wrap (telegram presenter PRD retired)

- **Docs**: WIP PRD for Telegram **PresenterObserver** stream removed from **`docs/ft/daemon/1-WIP/`**; product and integration remain in [telegram-notifications.md](telegram-notifications.md). **`docs/dev/1-WIP/daemon-telegram-validate/`** report bundle removed. Cross-package note: **[docs/dev/changesets.md](../../dev/changesets.md)**.

## 2026-04-04 — Projects: `main_branch_ref` (git integration base)

- **Registry**: Optional **`main_branch_ref`** on project rows; **`effective_integration_base_ref_for_project`**; **`add_project`** rejects invalid refs before **`projects.yaml`** writes (**`tddy_core::validate_integration_base_ref`**).
- **Docs**: [git-integration-base-ref.md](../coder/git-integration-base-ref.md), [project-concept.md](project-concept.md); package [connection-service.md](../../packages/tddy-daemon/docs/connection-service.md).
- **PRD retired**: Prior WIP PRD for the multi-user daemon was merged into [project-concept.md](project-concept.md) (**Multi-user daemon**) and this changelog; source file removed from **`docs/ft/daemon/1-WIP/`**.

## 2026-04-04 — Worktrees library + ConnectionService RPCs

- **`tddy_daemon::worktrees`**: Parses **`git worktree list`** output; **`WorktreeStatsCache`** persists per-project snapshots under **`TDDY_PROJECTS_STATS_ROOT`** (default **`~/.tddy/projects`**); **`validate_worktree_path_within_repo_root`** (lexical containment); **`remove_worktree_under_repo`** (membership in **`git worktree list`**, refuses primary worktree).
- **ConnectionService**: **`ListWorktreesForProject`** (optional **`refresh`** → **`refresh_stats_for_project`** in **`spawn_blocking`**), **`RemoveWorktree`** (invalidates cache on success). Project path via **`main_repo_path_for_host`** and local **`daemon_instance_id`** (remote daemon routing for these RPCs is out of scope). Tests: **`worktrees`**, **`worktrees_acceptance`**, **`worktrees_rpc`** (requires **`git`**, **`USER`** for registry tests).
- **Package doc**: [worktrees.md](../../packages/tddy-daemon/docs/worktrees.md), [connection-service.md](../../packages/tddy-daemon/docs/connection-service.md). Web feature: [worktrees.md](../web/worktrees.md).

## 2026-04-03 — Telegram session notifications (library)

- **Config**: Optional **`telegram`** block in **`daemon.yaml`** with **`enabled`**, **`bot_token`**, and **`chat_ids`** (integer chat targets); unknown keys on the block are rejected under **`deny_unknown_fields`**.
- **Behavior**: The **`tddy_daemon::telegram_notifier`** module provides **`TelegramSessionWatcher`** (baseline + one notification per status transition for active sessions), **`session_telegram_label`** (first two hyphen segments of **`session_id`**), **`mask_bot_token_for_logs`**, and **`send_telegram_via_teloxide`** (teloxide **`Bot::send_message`**). Tests use a mock **`TelegramSender`**; CI avoids the live Telegram API.
- **Docs**: Product reference **[telegram-notifications.md](telegram-notifications.md)**; technical reference **[telegram-notifier.md](../../packages/tddy-daemon/docs/telegram-notifier.md)**.

## 2026-04-03 — ConnectionService: workflow files, session base path, delete

- **`ListSessionWorkflowFiles`** / **`ReadSessionWorkflowFile`**: Allowlisted basenames (`changeset.yaml`, `.session.yaml`, `PRD.md`, `TODO.md`) under **`{sessions_base}/sessions/{session_id}/`** with canonical-path checks (**`session_workflow_files`**; tests **`session_workflow_files_rpc`**).
- **Sessions base**: **`sessions_base_for_user`** resolves the Tddy **data directory** (typically **`~/.tddy`**), matching **`tddy_core::output::tddy_data_dir_path`** when **`TDDY_SESSIONS_DIR`** is unset, so listing/connect/delete target the same trees as **`tddy-coder`**.
- **`DeleteSession`**: Terminates a live **`metadata.pid`** when needed (SIGTERM/SIGKILL; Linux zombie handling), then removes the directory; directories without readable **`.session.yaml`** are removed when the resolved path is valid.
- **Package**: [connection-service.md](../../packages/tddy-daemon/docs/connection-service.md). Web: [web-terminal.md](../web/web-terminal.md), [web changelog](../web/changelog.md).

## 2026-03-29 — ConnectionService: `ListAgents` and `allowed_agents`

- **Config**: Daemon YAML includes **`allowed_agents`**, a list of **`id`** (required) and optional **`label`** entries (same shape as tool allowlist entries; unknown keys on each entry are rejected when using **`deny_unknown_fields`**).
- **`ListAgents`**: Returns **`AgentInfo`** rows in config order; display labels use trimmed non-empty **`label`**, otherwise **`id`**.
- **`StartSession`**: When **`allowed_agents`** is non-empty, a non-empty **`agent`** must match an **`id`**; otherwise **`INVALID_ARGUMENT`**. An empty **`allowed_agents`** list does not apply this check.
- **Implementation**: Shared mapping lives in **`agent_list_mapping`**; integration tests cover config parse, RPC payloads, **`ListTools`** regression, and unknown agent rejection.
- **Package doc**: [connection-service.md](../../../packages/tddy-daemon/docs/connection-service.md). **Install / config**: [systemd-install.md](systemd-install.md).

## 2026-03-28 — Unified session tree and `session_id` validation

- **Filesystem**: Session directories use `{sessions_base}/sessions/{session_id}/` consistently for listing, connect, resume, signal, delete, and headless `GetSession` / `ListSessions`.
- **Validation**: `session_id` is validated as a single safe path segment on **ConnectSession**, **ResumeSession**, **SignalSession**, **DeleteSession**, and service-side **GetSession** before paths are built (aligned with `session_deletion` rules).
- **Feature reference**: [Session directory layout](../coder/session-layout.md) ([migration from non-unified trees](../coder/session-layout.md#migration-from-non-unified-trees)).

## 2026-03-28 — StartSession and spawn: `recipe`

- **`StartSession` / `StartSessionRequest`**: Optional **`recipe`** (`tdd` or `bugfix`); empty behaves like **`tdd`**. Session **`changeset.yaml`** persists **`recipe`** for the new session.
- **Spawn**: **`SpawnRequest`** includes **`recipe`**; the daemon passes **`--recipe`** to **`tddy-coder`** when set.
- **Package**: [connection-service.md](../../../packages/tddy-daemon/docs/connection-service.md). Coder feature: [workflow-recipes.md](../coder/workflow-recipes.md).

## 2026-03-28 — ConnectionService: multi-host selection + ListSessions workflow enrichment

- **`ListEligibleDaemons`**: Returns eligible daemon entries from **`EligibleDaemonSource`** (local instance; LiveKit peer discovery deferred).
- **`ListSessions`**: Each **`SessionEntry`** includes **`daemon_instance_id`** for the owning daemon, plus **`workflow_goal`**, **`workflow_state`**, **`elapsed_display`**, **`agent`**, and **`model`** from **`.session.yaml`** / **`changeset.yaml`** via **`session_list_enrichment`**. Blocking read and enrichment run on the thread pool via **`spawn_blocking_with_timeout`**. Enrichment failures are logged at **warn**; the RPC still returns base session fields from **`session_reader`**.
- **`StartSession`**: Accepts optional **`daemon_instance_id`**; local spawn when empty or matching the local instance; non-local targets return **unimplemented** until cross-daemon spawn routing exists.
- **Proto / service**: **`connection.proto`** defines **`SessionEntry`** fields; TypeScript and Rust stubs are generated from the proto.
- **Package doc**: [connection-service.md](../../packages/tddy-daemon/docs/connection-service.md). Web UX: [web-terminal.md](../web/web-terminal.md).

## 2026-03-24 — ConnectionService: DeleteSession

- **`DeleteSession`**: Removes the on-disk session directory under the authenticated user’s **`{sessions_base}/sessions/{session_id}/`** tree. Rejects invalid session ids with **`INVALID_ARGUMENT`**. Filesystem removal errors return a generic **`INTERNAL`** message to clients; full error detail is logged on the server.
- **Current behavior** (terminate live processes, metadata-less directories, **`sessions_base`** resolution): see **2026-04-03 — ConnectionService: workflow files, session base path, delete** above.

## 2026-03-23 — Root `./install --systemd`

- **Installer**: Repo **`./install --systemd`** (optional **`--build`** runs **`./release`** first) copies **`tddy-daemon`**, **`tddy-coder`**, **`tddy-tools`** from **`target/release/`**; installs **`daemon.yaml`** from **`daemon.yaml.production`** only when missing; writes **`tddy-daemon.service`**; copies **tddy-web** **`dist`** when present; runs **`systemctl`** unless **`INSTALL_NO_SYSTEMCTL=1`**.
- **Paths**: Overridable via **`INSTALL_PREFIX`**, **`INSTALL_BIN_DIR`**, **`INSTALL_CONFIG_DIR`**, **`INSTALL_SYSTEMD_DIR`**, **`INSTALL_WEB_BUNDLE_DIR`**.
- **Docs**: Feature summary in **[systemd-install.md](systemd-install.md)**. Example unit: **[docs/dev/tddy-daemon.service.example](../../dev/tddy-daemon.service.example)**.

## 2026-03-22 — LiveKit: `livekit.common_room` for spawns

- When **`livekit.common_room`** is set (non-empty), daemon-spawned **`tddy-*`** processes receive **`--livekit-room`** set to that value so all sessions share one room; **`--livekit-identity`** remains **`daemon-{session_id}`** per session. If unset or whitespace-only, the room name is **`daemon-{session_id}`** as before.

## 2026-03-21 — StartSession: `agent`

- **ConnectionService**: `StartSessionRequest` includes optional `agent`; forwarded to spawned `tddy-coder` as `--agent` when non-empty (skips interactive backend menu in the child).

## 2026-03-21 — Project concept

- **Projects**: Named `git_url` + `main_repo_path` per user; `~/.tddy/projects/projects.yaml`.
- **Config**: `repos_base_path` (default `repos` under user home).
- **ConnectionService**: `ListProjects`, `CreateProject` (optional `user_relative_path` for clone/adopt location under `~`); `StartSession` uses `project_id`; `SessionEntry` includes `project_id`.
- **Clone**: On create, clone into `{repos_base}/{name}/` unless path exists (then adopt).
- **Spawn**: `tddy-coder` receives `--project-id`; `.session.yaml` stores `project_id`.
- **PRD reference:** PRD-2026-03-21-project-concept.md (wrapped into [project-concept.md](project-concept.md)).
