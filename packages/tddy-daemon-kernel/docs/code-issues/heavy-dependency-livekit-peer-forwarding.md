# heavy-dependency: the LiveKit SDK, pulled in by `peer_forwarding`

**Location:** `packages/tddy-daemon-kernel/src/peer_forwarding.rs:18` — `use livekit::Room`
**Category:** heavy-dependency
**Detected:** 2026-09-19 by structural audit
**Metrics:** **1 module of 7** (266 of 3,957 lines, **6.7%**) pulls `livekit = "0.7"` + `tddy-livekit` · **14 dependent crates** · **6 of them inherit the SDK and use none of it** · sole SDK consumer in the crate
**Restructure:** required — `extract_module` to a sibling crate, `/code-restructuring` territory
**Status:** Open — **unclaimed**
**Verified:** ✅ hand-verified 2026-09-19 — see *Verified by hand*

## Measurement history

| Run | SDK consumers in crate | Dependents | Inheriting for nothing | Note |
|---|---|---|---|---|
| 2026-09-19 | 1 of 7 modules | 14 | 6 | first detection |

## What the tool found

`tddy-daemon-kernel` declares the LiveKit SDK directly:

```toml
# packages/tddy-daemon-kernel/Cargo.toml
# `peer_forwarding`: one daemon serving another's RPC over the LiveKit common room.
tddy-livekit = { path = "../tddy-livekit" }
livekit = "0.7"
```

The manifest comment names the reason, and the reason is the whole of it. Every reference to the SDK
in the crate is in one file:

```
$ grep -rn "^use livekit\|^use tddy_livekit\|livekit::" packages/tddy-daemon-kernel/src/
peer_forwarding.rs:18    use livekit::Room;
peer_forwarding.rs:132   -> Result<tddy_livekit::RpcClient, tddy_rpc::Status>
peer_forwarding.rs:150   tddy_livekit::LiveKitRpcClientFactory::for_room(room_arc)
peer_forwarding.rs:201   (doc comment)
```

`config.rs` and `daemon_identity.rs` match a `livekit` grep, but only on `LiveKitConfig` — the
crate's own serde struct — and on a doc comment. Neither imports the SDK.

**Who pays.** 14 crates depend on `tddy-daemon-kernel`. Six of them declare no LiveKit dependency of
their own and use none of its API:

| Inherits the SDK for nothing | Depends on the kernel for |
|---|---|
| `tddy-daemon-sandbox` | config types |
| `tddy-model-registry` | config types |
| `tddy-session-activity` | config types |
| `tddy-worktree-service` | `privilege_drop` (104 lines) |
| `tddy-spawn` | config types |
| `tddy-telegram` | config types |

## Why it matters here

**Two modules say this is already constraining them, independently.** Neither is speculation about
future cost; both describe a structure chosen to route around this dependency:

> It stays in `tddy-daemon` for that reason: the impersonation it exists for is
> `tddy_daemon_kernel::privilege_drop`'s, and **depending on that crate from a subsystem crate would
> put the whole LiveKit SDK inside `tddy-tools`' `--no-default-features` in-jail build, which
> carries none.**
> — `packages/tddy-session-lifecycle/src/pty_runtime.rs:9-12`

> The rest of `pty_runtime.rs` stayed behind: [it] exists to front-load a `setpriv` privilege drop
> through `tddy_daemon_kernel::privilege_drop`, and **depending on that crate from here would put
> the whole LiveKit SDK inside `tddy-tools`' `--no-default-features` in-jail build, which carries
> none today.**
> — `packages/tddy-terminal-rpc/src/login_shell.rs:9-12`

So the cost is not a slower build. It is that **`#unbundle` node 6 had to split one module in half**
— `login_shell` left, the rest of `pty_runtime` stayed — and the seam was chosen by what would drag
the SDK rather than by what belongs together. A 104-line passwd lookup cannot be reached from a
subsystem crate because a 266-line RPC-forwarding module sits in the same crate.

The in-jail build is the sharp edge: `tddy-tools --no-default-features` is what runs *inside* every
sandbox the daemon spawns, and it carries no LiveKit. Any subsystem that wants a kernel helper and is
reachable from that build is blocked outright, not merely slowed.

**For `#keyring`.** `#keyring` 1/9 adds per-daemon key material and a key-directory port. If either
lands in `tddy-daemon-kernel`, it inherits this constraint and becomes unreachable from the in-jail
build for the same reason. That is an argument for the port living in `tddy-daemon-auth` — which is
where the node already plans to put it — and it is worth re-checking rather than assuming, because
`tddy-daemon-auth` declares `livekit = "0.7"` itself and so would not notice the problem.

## What would close it

Move `peer_forwarding` into its own crate — `tddy-daemon-peer-forwarding`, or into the existing
`tddy-daemon-livekit`, which already owns peer discovery and re-exports every name in this module
(`peer_forwarding.rs:13`). Then drop `livekit` and `tddy-livekit` from the kernel manifest.

This is `/code-restructuring` work (`extract_module` / `move_module_to_crate`), not ordinary work.
Two cautions from prior `#carve` and `#unbundle` nodes, recorded in this repo's history:

- `move_module_to_crate` has been effectively unusable on comparable targets — plan for `git mv` plus
  a manifest edit, and budget for the import rewrite.
- `livekit_peer_discovery` re-exports every symbol here, so the move can keep all caller paths
  unchanged. Verify that re-export list first; it is what makes this cheaper than it looks.

The prize is concrete and checkable: after the move, `tddy-terminal-rpc` and `tddy-session-lifecycle`
can reach `privilege_drop` directly, and the `pty_runtime` / `login_shell` split can be revisited on
its merits.

## Verified by hand

**2026-09-19.** Checked, and the checks are re-derivable:

- Read `packages/tddy-daemon-kernel/Cargo.toml` in full and confirmed `livekit = "0.7"` is a direct
  dependency with a comment naming `peer_forwarding` as the reason.
- Grepped every SDK import across `packages/tddy-daemon-kernel/src/` — four hits, all in
  `peer_forwarding.rs`. Confirmed `config.rs`'s and `daemon_identity.rs`'s matches are `LiveKitConfig`
  and a doc comment, not imports, by reading the lines.
- Enumerated the 14 dependents with
  `grep -ln "tddy-daemon-kernel" packages/*/Cargo.toml`, then checked each one's own manifest for a
  LiveKit dependency to separate "inherits it for nothing" from "has it anyway".
- Read both module comments quoted above at their cited lines rather than trusting a search snippet.

**What an automated pass would have got wrong.** A dependency-graph tool reports 14 crates reaching
`livekit` through this edge and stops there. It cannot see that the cost already landed — that a
module was split in `#unbundle` node 6 to avoid this edge, and that the split is documented in two
places as a workaround rather than as a design. The two comments are the evidence that this is a real
constraint rather than a tidy-up, and they are not in the graph.
