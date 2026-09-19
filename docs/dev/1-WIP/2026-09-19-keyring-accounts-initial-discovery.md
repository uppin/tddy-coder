# Initial discovery — `#keyring` 4/9 `accounts`

**Node**: `#keyring` 4/9 — accounts · branch `feature/keyring/accounts` · base `feature/keyring/store (#510)`
**Changeset**: [2026-09-19-keyring-accounts.md](./2026-09-19-keyring-accounts.md)

This node's companion. **Exploration 1 below is the whole-stack discovery dump copied verbatim** —
every pass that produced the nine-node decomposition, the Step 2b scans over `docs/dev/todo/` and
`packages/*/docs/code-issues/`, and the `/analyze-code-issues` run on the two previously
unanalyzed packages. It is reproduced in full rather than referenced, because the whole-stack source
is temporary and is deleted once every node has its own copy.

Node-specific follow-up appended as later `## Exploration` sections.

---

### Source: whole-stack dump, 2026-09-19

**Temporary.** Not a changeset companion. Copied into each node's own
`{slug}-initial-discovery.md` as Exploration 1 at Step 4b, then deleted at the wave-1 checkpoint.

**Stack:** `#keyring`, 9 nodes. **Trunk:** `master`.

---

## Exploration 1 — whole-stack discovery (verbatim copy)

### Combined conclusions

*(filled as passes land; see `## Exploration N` at the tail for the raw dumps)*

### C1 — The credential surface being replaced is small and has exactly one external reader

`GitHubTokenStore` (`packages/tddy-github/src/token_store.rs:15`) is a two-method trait —
`put(login, access_token)` / `get(login)`. Its only implementation is `FileGitHubTokenStore`
(`packages/tddy-daemon-auth/src/github_token_store.rs`), one `0600` **plaintext** JSON file holding
`HashMap<String, String>` login → token, serialised with `write_atomic_with_mode` and guarded by a
process-wide `static PUT_LOCK: Mutex<()>` because concurrent logins would otherwise clobber.

Its only reader outside the auth crate is
`packages/tddy-session-lifecycle/src/connection_service/svc_pr_status_for_caller.rs:93`. Everything
else is construction and plumbing (`auth.rs:83-152`, `runtime.rs:882`).

The trait's own doc comment states a **security rule that constrains the replacement**: the GitHub
token is deliberately kept *outside* the HMAC session token, because that token is handed to a
browser over a plain-http LAN origin, so a live `repo`-scoped credential must never travel in it or
be returned to the client. It also states that a failed `put` must **fail the login** — "a session
minted without its token is a half-login".

The 2-argument signature has no room for a refresh token, no provider dimension, and no account
identity beyond the login.

### C2 — `livekit.api_secret` doubles as the session-token signing key, at one line

`packages/tddy-daemon-auth/src/auth.rs:68`:

```rust
// The one secret every daemon in a deployment shares (it also signs LiveKit room JWTs).
let signing_secret = config.livekit.as_ref().and_then(|lk| lk.api_secret.clone());
let signer = signing_secret.as_deref().map(|s| SessionTokenSigner::new(s.as_bytes()));
```

The comment names both the abuse and the reason for it: it was chosen **because it is shared
deployment-wide**, which is what lets a token minted by daemon A verify on daemon B. Any replacement
must answer that requirement or deliberately drop it.

Token format today (`packages/tddy-github/src/session_token.rs`): `v1.<b64url(json)>.<b64url(hmac-sha256)>`,
`TAG_LEN = 32`, compared with `subtle::ConstantTimeEq`. Access TTL 5 min, Refresh TTL 7 days,
sliding. `iat` is never validated; there is no `jti` and no key id, so `logout` is a documented
server-side no-op (`auth_service.rs:214`) and a leaked refresh token is valid for its full 7 days.

### C3 — Desktop login is blocked by three independent barriers, not one

`desktop.yaml.production:66-92` states them itself, and each is a separate code site:

| # | Barrier | Code site |
|---|---|---|
| 1 | `github:` needs **both** `client_id` and `client_secret`, or no auth entry is registered at all | `auth.rs:109` — `else if let (Some(id), Some(secret)) = (…)` |
| 2 | `livekit.api_secret` is "the ONLY source of the session-token signer, required even with no common room" | `auth.rs:68` (C2) |
| 3 | `users: []` — a login with no entry is refused `permission_denied: user not mapped to OS user`, with "deliberately no fallback" | `packages/tddy-daemon-kernel/src/config.rs:1111` `os_user_for_github`, linear search |

`./install --desktop` renders all three unset, so a fresh install cannot sign in. Observed in
`~/.tddy/logs/daemon`: `auth.AuthService` is never registered, and Login returns `not_found` —
`MultiRpcService.handle_rpc: NO service 'auth.AuthService' (registered: ["daemon_config.DaemonConfigService", …])`.

There is **no device-flow code anywhere in the tree**: `packages/tddy-github/src/real.rs:68` has the
single OAuth call, the redirect-flow exchange, and it posts `client_secret`.

### C4 — The encryption pattern to reuse already exists, and so do all but one of its crates

`packages/tddy-screen-sharing/src/screen_sharing_vault.rs` (394 lines): Argon2id(passphrase, 32-byte
salt) → 32-byte key; ChaCha20-Poly1305 with a fresh nonce per item; a `VERIFIER_PLAINTEXT` ciphertext
that proves a passphrase without storing the key; `write_atomic_with_mode(path, …, 0o600)`.

Known limits of that implementation, to fix rather than copy: the passphrase crosses the wire in
plaintext (`req.passphrase`, :862); only the password field is AEAD'd, so target metadata has **no
integrity protection**; `argon2::Argon2::default()` parameters are not versioned on disk; and
`DerivedKey(pub [u8; 32])` is never zeroized, cached in
`ScreenSharingKeyCache = Arc<Mutex<HashMap<String, DerivedKey>>>` keyed by `session_id`.

`packages/tddy-host-service/src/host_keypair.rs` is the asymmetric prior art: RSA-2048 OAEP-SHA256,
`host-prompt-key.pem` at `0600`, `PublishedKey { spki_der, fingerprint }`, with client-side key
continuity in `packages/tddy-web/src/lib/hostKeyPinning.ts`. Its own docs state the LiveKit threat
model plainly — the room is "a trusted peer group, **not a cryptographically authenticated one**";
publishing a key defeats a *passive* relay but not an *active* peer, because the client learns the
public key over the same unauthenticated channel.

Workspace crypto dependencies — present: `rsa =0.9.10` (pinned), `chacha20poly1305 0.10`,
`argon2 0.5`, `hmac 0.12`, `sha2 0.10`, `subtle 2.6`, `rand`. **Absent: `ed25519-dalek`, `hkdf`,
`zeroize`.** The root `Cargo.toml` carries a tuning note that RSA-2048 keygen is slow enough to need
`num-bigint-dig` at opt-level 3 even in test builds.

### C5 — Projects already have an extensible row; assignments are additive

`packages/tddy-projects/src/project_storage.rs` — `ProjectData` in `~/.tddy/projects/projects.yaml`,
already carrying three `#[serde(default, skip_serializing_if = …)]` optional fields and a
`HashMap<String, String> host_repo_paths`. Adding `accounts: Vec<AccountId>` is additive and needs no
migration. `packages/tddy-service/proto/project.proto` has 5 RPCs and `ProjectEntry` with 7 fields.

### C6 — GitHub credentials reach remote work as an **environment variable**, not a Rust call

`packages/tddy-workflow-recipes/src/github_rest_common.rs:20` reads `GITHUB_TOKEN` then `GH_TOKEN`;
all REST goes through four `curl_github_*_json` helpers. `packages/tddy-tools/src/server.rs:1480`
exposes the GitHub PR tools only when one is set. **Nothing in the daemon injects either variable
today** — they arrive from the OS user's environment.

Git identity: `packages/tddy-daemon-livekit/src/session_room.rs:342` pins
`GIT_AUTHOR_*`/`GIT_COMMITTER_*` to a fixed `tddy-daemon` identity — *deliberately*, and only for
machine-made WIP snapshot commits ("this object is not the agent's work, it is a machine-made
snapshot of it"). Agent commits inside the jail inherit whatever the OS user's gitconfig says.
Session environment is assembled at `session_room.rs:1158` (`command.env(name, value)`).

**These are precisely the files PR #492 moves**, which is why node 9 is sequenced behind it.

### C7 — #492 is 6/10 of an all-draft stack; nothing in `#carve` has merged

```
498 draft  feature/carve/test-homes        -> master
491 draft  feature/carve/core-foundations  -> feature/carve/test-homes
492 draft  feature/carve/git-plumbing      -> feature/carve/core-foundations
493 draft  feature/carve/session-store     -> feature/carve/git-plumbing
494 draft  feature/carve/telegram          -> feature/carve/session-store
495 draft  feature/carve/presenter-split   -> feature/carve/telegram
496 draft  feature/carve/pr-stack-crate    -> feature/carve/presenter-split
```

#492 extracts a `tddy-git` crate (~1,200 lines out of `worktree.rs`) and moves
`github_rest_common.rs` (338), `github_pr.rs` (494) and `orchestrate_pr_stack/github.rs` (1,292) —
2,124 lines — into `tddy-github`. Its own changeset records "Greenable independently: **no**".

A node gated on #492 is gated on roughly half of `#carve` landing. Decision taken in the interview:
**keep node 9 in the line and record the block** rather than park it.

### C8 — Cross-daemon token verification is a load-bearing invariant, and n1 has to replace it, not just drop it

Today every daemon verifies every other daemon's session token **because the HMAC key is identical
on all of them**. `docs/ft/daemon/session-auth.md` opens on that as the whole point — "Authenticate a
web client against **any** daemon in a LiveKit deployment with a single GitHub login" — and
`docs/ft/daemon/auth-livekit-services.md` § *One secret signs two things* says the shared key is held
deliberately, warning that "a second signer would silently partition which tokens each half accepts,
and the partition would stay invisible until a cross-daemon call failed."

A per-daemon Ed25519 key **is** that second signer, many times over. So n1 owns a replacement for the
invariant, not only a replacement for the key:

- each daemon publishes its **public** key (SPKI + fingerprint, the `host_keypair.rs` shape), and a
  verifying daemon selects the key by a signer id carried in the `v2` token;
- distribution runs over the common room, which is where peers already exchange published keys —
  **but** `packages/tddy-daemon-livekit/tests/dependency_boundary_unit.rs` walks the manifest closure
  and fails if `tddy-daemon-auth` lands on `tddy-daemon-livekit`'s dependency path. Key distribution
  therefore crosses the same way minting already does: a **port**, owned by auth, implemented toward
  LiveKit — never a direct dependency.

**Exploration 3 confirms this is exercised, not merely documented.** `livekit_peer_discovery.rs:18`
forwards the *client's own* `session_token` to a peer daemon for `StartSession`, and `:620` does the
same for project aggregation; the peer authenticates it with its own resolver. Drop the shared key
without a replacement and peer forwarding, peer project aggregation and cross-host `StartSession`
all break — the three things `session-auth.md` was written to fix.

This is genuinely more than "swap the signer", and it is the one place n1 could be under-scoped.
Standalone and desktop deployments are unaffected either way: one daemon, one key, nothing to
distribute — which is why n2 can sit behind n1 without inheriting any of this.

### C9 — The daemon already mints a session for a locally-resolved OS user, with no GitHub round-trip

`packages/tddy-daemon-auth/src/local_token.rs` exposes `mint_local_token(resolved_user)`, which the
UDS tonic adapter calls after resolving `uid → login`. It builds a `GitHubUser { id: 0, login, .. }`
and signs it with the same `SessionTokenSigner`.

Two consequences for n2. It is **evidence the shape n2 wants already exists** — an identity the
machine established rather than one GitHub vouched for — so "resolve the OS user as the running
user" is not a new concept in this codebase. And it is **blocked by the same barrier**: no signer,
no local token either, so the desktop app cannot fall back to it today. Barrier 2 is upstream of
both login paths, which is the n1 → n2 edge stated in code rather than in reasoning.

### C10 — The signer has four consumers, and the acceptance suites hard-code a fleet secret

`auth.rs` (GitHub login), `local_token.rs` (UDS), `session_room.rs` through the `SessionTokenMinter`
port (LiveKit room tokens), plus `token_service_acceptance.rs` / `auth_service_acceptance.rs`, which
construct signers from `FLEET_SECRET` and from `b"some-other-fleets-secret"` to assert
cross-fleet rejection. Those two suites are n1's ready-made red tests: what they currently express as
"a different shared secret is rejected" becomes "a key this daemon does not trust is rejected".

`session_token.rs` is 370 lines with one `verify` at `:171` and 7 unit tests — the v1 → v2 change is
small and already covered.

### C11 — Product area and WIP conflicts (Step 3, Step 2 item 4)

Primary area **`daemon`**; `desktop` and `web` are secondary. The feature docs the stack lands in:

| Doc | Nodes |
|---|---|
| `docs/ft/daemon/session-auth.md` | n1 (token model, signing key), n2 (login), n3 (§ GitHub access-token retention) |
| `docs/ft/daemon/auth-livekit-services.md` | n1 (§ One secret signs two things) |
| `docs/ft/daemon/project-concept.md` | n5 |
| `docs/ft/daemon/livekit-peer-discovery.md`, `session-room.md` | n6, n9 |
| `docs/ft/desktop/tddy-desktop-tauri.md` | n2 |
| `docs/ft/web/projects-screen-multi-host.md` | n5 |
| `docs/ft/web/screen-sharing-sessions.md` | n7 |
| `docs/ft/coder/pr-stack-live-status.md` | n3, n9 (the consumer of the retained token) |
| **new** `docs/ft/web/accounts-screen.md` | n4 |

**No conflicting active changeset.** The other five documents in `docs/dev/1-WIP/` — two
sandbox-split, one LiveKit rooms panel, one restructure-refusal, one dev-launchers — score **zero**
on `api_secret`, `token_store`, `credential`, `session token`, `GitHubTokenStore` and
`screen_sharing_vault`. `docs/ft/daemon/1-WIP/` and `docs/ft/desktop/1-WIP/` do not exist;
`docs/ft/web/1-WIP/` holds only `archived/`.

### C12 — Step 2b is settled: one claimed record, zero blockers outside it, four entries this stack closes

248 records read across both registers — 102 code issues (Exploration 4) and 146 backlog entries
(Exploration 5). 22 are in this stack's path.

**One 🚧 Claimed record, and its fork is already answered.**
`packages/tddy-workflow-recipes/docs/code-issues/squatting-github-rest-client.md` carries
`**Claimed by:** #492 — #carve 6/10 git-plumbing · draft`, and relocates the exact files n9 edits.
The developer chose in Step 1 to keep n9 in the line and record the block. #492 now sits behind only
#498 and #491. The other 13 `Claimed by:` markers in the repo are all `#carve` and none names a file
any node edits — overlap is per-file, not per-package. `docs/dev/todo/` has no such field at all.

**No ⛔ Blocking verdict survives verification outside n9.** The nearest candidate was the
`auth_storage` permissions entry, and it fails the correctness test: a `0600` key file written
through `write_atomic_with_mode` is protected by its own mode regardless of the directory's.

**Four records close with this stack**, one per claiming node:

| Record | Claimed by | Why it closes |
|---|---|---|
| `2026-08-02-verify-rejects-…-flaky-1-in-64.md` | **n1** | the v2 format makes it ~4× worse unless n1 fixes it |
| `2026-09-10-ensure-owner-only-dir-….md` | **n1** | n1's private key settles the posture question it asks |
| `2026-09-18-desktop-install-configures-no-identity.md` | **n2** | n2 is the node that exists to fix it |
| `2026-08-16-the-daemon-s-secret-stores-still-truncate-in-place.md` | **n7** | already closed upstream; stale file, n7 deletes it |

**No restructure node.** Two independent backlog entries and seven ⚠ During complexity records
converge on the same advice: the oversized files this stack edits (`auth.rs` 1,200 lines,
`runtime.rs::build` 806, `session_room.rs` ×3) were deliberately moved unsplit, and the split belongs
on a follow-up branch after the stack lands. Each node's changeset carries that as a constraint.

**Six touched packages have never been analyzed** — `tddy-github` (n1, n2, n3, n8),
`tddy-daemon-kernel` (n1, n2), `tddy-projects` (n5), `tddy-screen-sharing` (n7), `tddy-desktop` (n2),
`tddy-web` (n2, n4, n5). Named in each affected changeset; not a blocker.

### C13 — Decision: n1 ships key distribution with the key (2026-09-19)

Put to the developer once C8 was confirmed exercised rather than documented, with the smaller
alternative (a dedicated fleet-wide HMAC secret, no v2, no distribution) stated at equal weight.
**Chosen: per-daemon Ed25519 + key distribution**, accepting roughly double the node:

```
n1  signing-key
 ├ Ed25519 keypair, first boot, auth_storage 0600
 ├ token v1 -> v2  { ..., kid: <fingerprint> }
 ├ KeyDirectory port (auth owns; livekit implements)
 ├ publish SPKI+fingerprint to common room
 ├ verify peer token by kid -> pinned pubkey
 ├ fix session_token.rs:232 flake (1-in-4 under v2)
 └ warn once if auth_storage looser than 0700
```

The trade taken knowingly: n1 is the largest node in the stack, and the alternative would have kept
a shared secret — contradicting the Step 1 answer "Ed25519 per daemon, **no shared secret**" and
leaving n6 to invent peer identity from scratch.

**Constraint on the wiring, not negotiable:**
`packages/tddy-daemon-livekit/tests/dependency_boundary_unit.rs` walks the manifest closure and
fails if `tddy-daemon-auth` appears on `tddy-daemon-livekit`'s dependency path. Key distribution
crosses the same way minting already does — a port owned by auth, implemented toward LiveKit. The
existing `SessionTokenMinter` is the precedent to copy.

~~**Still owed before n1's wave 2:** developer approval of `ed25519-dalek`~~ — **approved
2026-09-19.** `zeroize` optional. n6's wrap-to-recipient reuses the existing `rsa` OAEP, so it adds
nothing.

### C14 — the two unanalyzed packages are now analyzed; one finding constrains n1

`tddy-github` and `tddy-daemon-kernel` had no `docs/code-issues/` directory. Analyzed 2026-09-19
against a green baseline (`./test -p tddy-github -p tddy-daemon-kernel` → **112 passed, 0 failed**;
kernel 78 unit + 4 acceptance, github 24 unit + 6 acceptance). Five records opened, all hand-verified.
Full detail in Exploration 6.

**The one that changes a decision:** `peer_forwarding.rs` is the **sole** consumer of the LiveKit SDK
in `tddy-daemon-kernel` — 1 module of 7, 266 of 3,957 lines — and it forces `livekit = "0.7"` onto all
14 dependents, 6 of which use none of it. Two modules (`pty_runtime.rs:9-12`,
`login_shell.rs:9-12`) independently record that as a constraint they had to design around, because
the SDK cannot enter `tddy-tools`' `--no-default-features` in-jail build. `#unbundle` node 6 split a
module in half to route around it.

**Consequence for n1:** the keypair and the `KeyDirectory` port **must not** land in
`tddy-daemon-kernel`. They go in `tddy-daemon-auth`. The plan already said so; this makes it a
constraint rather than a preference, and it is the kind of thing that would otherwise have been
discovered during wave 2 with the code already written.

**A second thing this pass settled** (Exploration 6 § *The shortcut that looks free*): `tddy-daemon-auth`
already declares `livekit = "0.7"` and already drives a `Room` + `RpcClient` in
`oauth_loopback_tunnel.rs:11,21`. So auth *could* do key distribution against the room directly,
with no port — and the boundary test would stay green, because it constrains the other direction.
**Rejected, and the PRD says why**: the room is one transport, and a desktop or single-daemon
deployment has none. A port keeps "where do I get a peer's public key" answerable without a room,
which is the coupling n1 exists to remove; the shortcut re-creates it in a new place.

**None of the five records blocks any node, and n1 claims none of them.** One is a live hand-off to
n2: `missing-tests-real-exchange-code` records that `RealGitHubProvider::exchange_code` is untestable
by construction (two hardcoded absolute hosts, no injection point), and n2 adds the **device flow** to
that same provider — roughly another hundred lines of network code with the same property, plus a
polling state machine that is harder to get right than the two-call exchange. n2 should open the
base-URL seam *before* adding the flow, or it writes the seam twice.

---

### Exploration 1.1 — pre-interview grounding

Passes run before and during the Step 1 interview; findings folded into C1–C7 above. Commands used:

- `grep -rn "GitHubTokenStore|github_token_store" packages --include="*.rs"` → C1
- `grep -rn "api_secret" packages --include="*.rs"`, `sed -n '55,100p' auth.rs` → C2
- `cat desktop.yaml.production`, `grep -rn "client_secret"`, `grep -rn "login/oauth/access_token|login/device|grant_type"`, `~/.tddy/logs/daemon` → C3
- `grep -rhnE "^(rsa|ed25519-dalek|…) *=" --include=Cargo.toml packages Cargo.toml` → C4
- `sed -n '1,80p' project_storage.rs`, `sed -n '1,120p' project.proto` → C5
- `grep -rn "GITHUB_TOKEN|GH_TOKEN"`, `grep -rn "GIT_AUTHOR|GIT_COMMITTER"`, `sed -n '330,350p' session_room.rs` → C6
- `gh pr list --state open --json number,title,headRefName,baseRefName,isDraft` → C7

Interview decisions recorded (Step 1, user-approved):

1. **n1 signing key** — per-daemon **Ed25519**, generated at first boot into `auth_storage/` at
   `0600`, published as SPKI + fingerprint. Token `v2`. **No shared secret.**
2. **n9 gating** — keep in the line, record the block on #492.
3. **Screen-sharing vault** — **migrate** into the generic store (node 7); `ScreenSharingVault`,
   `ScreenSharingKeyCache` and the wire passphrase are deleted. Justified because n1+n2 *are* the
   passphrase redesign: once the vault is keyed to the daemon and gated on a valid session, the
   per-session passphrase has nothing left to do.
4. **n6 sync trust** — a pre-shared **group membership secret** authenticates *which Ed25519 public
   keys to accept*; it is **not** key material. Payloads stay wrapped to the authenticated
   recipient's key. Conflicts: monotonic vault version, last-writer-wins, every rejected merge
   journaled. Per-account versioning deferred.
5. **n2 desktop-login** — focus is the **local login**, LiveKit present or not. **OAuth App +
   device flow + public client ID**, matching `gh`. Non-expiring token, so "refresh on relogin"
   is retired and the protection relocates to n3's encryption at rest plus revocation from n4.
   Acceptance criterion is a **fresh `./install --desktop` with no config editing**.
6. **Dependency to approve before n1's wave 2**: `ed25519-dalek` (one new crate). `zeroize` optional
   but recommended for the vault key. n6's wrap-to-recipient reuses the existing `rsa` OAEP path.

### Exploration 1.2 — planning contract, signer consumers, product area — 2026-09-19

**Agent**: parent Grep/Read
**Scope**: the Step 2/2b/3 contract itself; every consumer of the session-token signer; the
product-area docs the stack lands in; conflicts with other active changesets.

#### Sequence

1. Read `.agents/skills/planning/references/planning-phase.md` — the Step 2 / 2b / 3 contract.
2. Read `.agents/skills/planning/references/initial-discovery.md` — the dump contract this file owes.
3. Read `.agents/skills/deferred-work/references/planning-cross-check.md` — the 🚧 Claimed fork.
4. `ls docs/ft/{daemon,desktop,web}/` and their `1-WIP/` — Step 3 product area.
5. `ls docs/dev/1-WIP/` then `head -18` + a grep over each of the 5 other active changesets —
   Step 2 item 4, conflicting active changesets.
6. Read `docs/ft/daemon/session-auth.md` head — the token model this stack's n1 changes.
7. Read `docs/ft/daemon/auth-livekit-services.md` § *One secret signs two things* — why the key is
   shared, and what structurally holds it that way.
8. Grep `SessionTokenMinter|SessionTokenSigner` across `packages/**/*.rs` — every signer consumer.
9. Read `packages/tddy-daemon-auth/src/local_token.rs` (72 lines, whole file).
10. `grep -n "fn verify"` + `wc -l` on `packages/tddy-github/src/session_token.rs`.
11. Read `/Users/mantasi/.tddy/logs/daemon` — the installed desktop app's own log.

#### Grep / glob

| Tool | Pattern | Path scope | Notable hits |
|---|---|---|---|
| Grep | `SessionTokenMinter\|SessionTokenSigner` | `packages/` `*.rs` | `auth.rs:71,325,451`; `local_token.rs:6,13,23,27,65`; `session_room.rs:2011,2029,2052`; `dependency_boundary_unit.rs:68-81`; `token_service_acceptance.rs:160`; `auth_service_acceptance.rs:96,116,188` |
| Grep | `fn verify` | `tddy-github/src/session_token.rs` | `:171` — the single verify entry point; 7 tests below it |
| Grep | `api_secret\|token_store\|credential\|session token\|GitHubTokenStore\|screen_sharing_vault` | each of the 5 other `docs/dev/1-WIP/` changesets | **0 hits in all five** |

#### Inspected files

##### `docs/ft/daemon/auth-livekit-services.md` § One secret signs two things

**Why**: n1 replaces this secret, so what holds it in place is n1's real scope.
**Excerpt**:

> `config.livekit.api_secret` signs **both** LiveKit room JWTs and session tokens […] a token minted
> by one daemon is verifiable by every daemon holding the same secret, with no session store and no
> propagation.
>
> Auth and LiveKit being separate crates does **not** separate that secret, and **neither crate
> derives its own**. A second signer would silently partition which tokens each half accepts, and the
> partition would stay invisible until a cross-daemon call failed. The rule is held structurally
> rather than by convention: `tddy-daemon-livekit` reaches minting through a `SessionTokenMinter`
> **port**, and a test walks its manifest closure to prove `tddy-daemon-auth` is not on its
> dependency path.

##### `packages/tddy-daemon-livekit/tests/dependency_boundary_unit.rs:68-81`

**Why**: this is the test that will fail if n1 wires key distribution the obvious (wrong) way.
**Excerpt**:

```rust
/// deliberately does not reach for it: [`SessionTokenMinter`] is a port, so the credential is
/// [`SessionTokenMinter`]: tddy_daemon_livekit::session_room::SessionTokenMinter
        "{THIS_CRATE} must mint nothing: a room token arrives through the SessionTokenMinter port, \
```

##### `packages/tddy-daemon-auth/src/local_token.rs` (whole file, 72 lines)

**Why**: grep showed a *fourth* signer consumer nobody had accounted for.
**Excerpt**:

```rust
static SIGNER: OnceLock<Arc<SessionTokenSigner>> = OnceLock::new();

/// Mint a session token for an identity the transport already resolved.
pub fn mint_local_token(resolved_user: &str) -> Result<String, LocalTokenError> {
    let signer = signer()?;
    let login = resolved_user.trim();
    if login.is_empty() { return Err(LocalTokenError::NoSigner); }
    let user = GitHubUser { id: 0, login: login.to_string(),
                            avatar_url: String::new(), name: login.to_string() };
    Ok(signer.mint_access(&user))
}
```

and, on the RPC arm:

```rust
// The UDS tonic adapter resolves uid → login and calls [`mint_local_token`] directly today;
// this RPC path exists for Connect-HTTP registration symmetry […]
Err(Status::unimplemented(
    "MintLocalToken over RpcService requires transport-resolved identity; use the UDS adapter",
))
```

##### `packages/tddy-daemon-auth/src/auth.rs:325`

**Why**: states the threat model the signing key sits in.
**Excerpt**: the doc comment warns that a client holding the secret `SessionTokenSigner` uses
"could sign an access token for any GitHub user".

##### `/Users/mantasi/.tddy/logs/daemon`

**Why**: direct evidence of the desktop barrier, from the installed app rather than from reading
the code that causes it.
**Excerpt**:

```
11:23:31.736 [WARN] [tddy_daemon::auth] serving daemon_config.DaemonConfigService with no way to
             verify a session token — every call will be refused. Configure `github:` to make it usable.
11:23:31.748 [INFO] [tddy_desktop] daemon assembled with 2 services
11:23:36.646 [WARN] [tddy_rpc::bridge] MultiRpcService.handle_rpc: NO service 'auth.AuthService'
             (registered: ["daemon_config.DaemonConfigService", "grpc.reflection.v1.ServerReflection"])
```

Two services, not the full set; `auth.AuthService` is **not registered at all**, so the client's
`GetAuthUrl` returns `not_found` rather than a configuration error.

#### Findings

1. **The signer has four consumers, not one.** `auth.rs` (GitHub login), `local_token.rs` (UDS
   local minting), `session_room.rs` via the `SessionTokenMinter` port (LiveKit room tokens), and
   the acceptance suites that construct one from a `FLEET_SECRET` constant. n1 touches all four.
2. **`mint_local_token` already does what n2 needs, minus the key.** It mints a valid session for an
   OS user the transport resolved, with `id: 0` and no GitHub round-trip. It is blocked by exactly
   the same missing signer as the GitHub path, which is barrier 2 again.
3. **Cross-daemon verification is a documented invariant with a structural test behind it**, not an
   incidental property — see C8.
4. `session_token.rs` is **370 lines** with a single `verify` entry point at `:171` and 7 unit
   tests. The v1→v2 change is small and well-covered.
5. **No conflicting active changeset.** All five others score zero on every credential, token,
   `api_secret` and vault term.
6. Neither `docs/ft/daemon/1-WIP/` nor `docs/ft/desktop/1-WIP/` exists; `docs/ft/web/1-WIP/`
   contains only `archived/`. Nothing to collide with when the node PRDs are written.

### Exploration 1.3 — is cross-daemon verification real, or only documented? — 2026-09-19

**Agent**: parent Grep/Read
**Scope**: whether any code path actually verifies a token another daemon minted — C8 is n1 scope
only if it does. Plus the live `#carve` PR state that gates n9.

#### Sequence

1. Grep `session_token` in `packages/tddy-daemon-livekit/src` — find the peer-forwarding path.
2. Read `packages/tddy-daemon-livekit/src/livekit_peer_discovery.rs:10-30` — its trust model.
3. `gh pr list --state open --json number,title,headRefName,baseRefName,isDraft` — the real chain
   under #492, since the interview's snapshot predated `#carve` 1–3 merging.

#### Grep / glob

| Tool | Pattern | Path scope | Notable hits |
|---|---|---|---|
| Grep | `session_token` | `tddy-daemon-livekit/src` | `livekit_peer_discovery.rs:18,620-633` (peer forward); `livekit_service.rs:96` (`user_resolver` gate); `session_room.rs:2076` (minted per poll) |

#### Inspected files

##### `packages/tddy-daemon-livekit/src/livekit_peer_discovery.rs:14-21` § Trust and security

**Why**: settles whether a peer verifies a token it did not mint.
**Excerpt**:

```rust
//! **Anyone who can join the configured LiveKit room** (same project URL, API key/secret, and
//! `livekit.common_room` name) can appear in **ListEligibleDaemons** and receive a forwarded
//! **StartSession** RPC. The forwarded request includes the client **`session_token`** and full
//! protobuf body. Treat the shared room as a **trusted peer group** (private LiveKit project,
//! network-restricted access); this is **not** a substitute for cryptographic proof that a
//! participant runs authentic `tddy-daemon` software.
```

`peer_project_entries(&self, session_token: &str)` at `:620` forwards the same client token for
project aggregation.

#### Findings

1. **Cross-daemon verification is exercised, not merely documented.** Daemon A forwards the
   *client's own* `session_token` to daemon B for `StartSession` and for project aggregation, and B
   authenticates it with its local `user_resolver` (`livekit_service.rs:96` shows the same resolver
   shape). That only works because the HMAC key is identical fleet-wide. **C8 is n1 scope.**
2. **n1 answers a weakness this module states about itself.** The room is explicitly "not a
   substitute for cryptographic proof that a participant runs authentic `tddy-daemon` software".
   Per-daemon Ed25519 identity keys are that proof, and they are what n6's group-secret
   authentication is built on — so n1 is not only a key swap, it closes a named gap.
3. **The `#carve` chain under #492 is two PRs, not six.** `#carve` 1–3 have merged (`b6e70f52`,
   `60360a17`, and one below them). Open and beneath #492: **#498** (`carve/test-homes` → `master`)
   and **#491** (`carve/core-foundations` → #498). #492 itself is `carve/git-plumbing` → #491.
   n9's block is materially nearer than the interview assumed.
4. **#504** (`feature/install-desktop`, **ready**, → `master`) touches `./release --desktop`, which
   is n2's territory. It is expected to merge well before this stack reaches n2; noted so n2's
   rebase is not surprised by it.

### Exploration 1.4 — Step 2b, `packages/*/docs/code-issues/` — 2026-09-19

**Agent**: Explore subagent (very thorough)
**Scope**: all 102 records across 28 `packages/*/docs/code-issues/` directories, read in full
(body, not heading), classified against the nine nodes' touch paths.

#### Result: 102 read · 94 unrelated · 8 relevant · 1 blocking

| # | Record | Symbol / site | Nodes | Verdict |
|---|---|---|---|---|
| 1 | `tddy-daemon-auth/…/complexity-auth-build-auth-entries.md` | `build_auth_entries`, `auth.rs:51` (104 lines — spans both `:68` and `:109`) | n1, n2 | ⚠ During |
| 2 | `tddy-daemon/…/complexity-runtime-build.md` | `build`, `runtime.rs:498` (806 lines — covers `:882`, the token-store wiring) | n3 | ⚠ During |
| 3 | `tddy-daemon-livekit/…/complexity-session-room-git-output.md` | `git_output`, `session_room.rs:1144` (84 lines — contains `:1158`, the `command.env` git-identity injection) | n9, n6 | ⚠ During |
| 4 | `tddy-daemon-livekit/…/complexity-session-room-run.md` | `run`, `session_room.rs:2404` (79 lines, the poll loop) | n9, n6 | ⚠ During |
| 5 | `tddy-daemon-livekit/…/complexity-session-room-take-c-quoted.md` | `take_c_quoted`, `session_room.rs:834` (nesting 8) | n9, n6 | ⚠ During |
| 6 | `tddy-livekit/…/complexity-participant-run.md` | `run`, `participant.rs:530` (198 lines) | n6 | ⚠ During |
| 7 | `tddy-workflow-recipes/…/squatting-github-rest-client.md` | `github_rest_common.rs` + `github_pr.rs` + `orchestrate_pr_stack/github.rs`, 2,124 lines | **n9** | ⛔ **Blocking · 🚧 Claimed** |
| 8 | `tddy-coder/…/complexity-run-build-auth-service-entry.md` | `build_auth_service_entry`, `run.rs:1131` — CLI-side, gated on `(Some(id), Some(secret))` | n1, n2 | ⚠ During |

#### The one 🚧 Claimed record — already decided in the interview

```
**Claimed by:** #492 — `#carve` 6/10 `git-plumbing` · draft · `feature/carve/git-plumbing`
```

`squatting-github-rest-client.md` relocates the exact two files n9 edits. The wait-or-proceed fork
was put to the developer during Step 1 and answered: **keep n9 in the line and record the block**.
Exploration 3 measured the chain — #492 now sits behind only **#498** and **#491**, both open.

**No second fork.** The other 13 `Claimed by:` markers in the repo all belong to `#carve` too, and
none names a file any of the nine nodes edits — `worktree.rs`, `presenter_impl.rs`, the changeset
parser, the session catalog, the Telegram control plane, the PR-stack data model. Overlap is
per-file, not per-package, so `#carve` 5/10 claiming records inside `tddy-core` does not constrain
this stack.

#### ✗ Six of this stack's packages have never been analyzed

A missing `docs/code-issues/` directory is **not** a clean bill of health — it means
`/analyze-code-issues` has never run there. Of the sixteen packages `#keyring` touches:

| Unanalyzed | Nodes that land in it |
|---|---|
| `tddy-github` | n1, n2, n3, n8 — the OAuth provider, the token store trait, the session token |
| `tddy-daemon-kernel` | n1, n2 — `GitHubConfig`, `UserMapping`, `os_user_for_github` |
| `tddy-projects` | n5 — `ProjectData`, `projects.yaml` |
| `tddy-screen-sharing` | n7 — the vault this stack deletes |
| `tddy-desktop` | n2 — the device-flow host |
| `tddy-web` | n2, n4, n5 — the Accounts screen and the Projects assignment UI |

`tddy-github` is the notable one: four nodes rewrite it and nothing is recorded about its shape.
Analyzed for contrast: `tddy-session-lifecycle` (22), `tddy-workflow-recipes` (6), `tddy-coder` (3),
`tddy-daemon` (3), `tddy-daemon-livekit` (3), `tddy-daemon-auth` (1), `tddy-livekit` (1),
`tddy-service` (1), `tddy-host-service` (1), `tddy-tools` (1).

#### Findings

1. **Nothing blocks nodes 1–8.** Every record in their path is a ⚠ During complexity finding on a
   function the node already has to touch — a constraint on *how*, not a prerequisite.
2. **The ⚠ During records cluster into exactly two functions**, and both are ones this stack is
   already rewriting: `build_auth_entries` (n1 + n2) and `runtime::build` (n3). Neither needs a
   restructure node — the nodes shrink them as a side effect of removing the secret-gated branches.
3. **`session_room.rs` carries three separate complexity records** and n9 edits it. With n9 already
   gated on #492, this is worth stating in n9's `## Prerequisites` as a reason its scope stays
   narrow: inject resolved env, do not refactor the file.
4. **The only ⛔ is n9's, and it was already decided.** No new developer fork is owed.

### Exploration 1.5 — Step 2b, `docs/dev/todo/` — 2026-09-19

**Agent**: Explore subagent (very thorough), then parent verification of the three ✅/⛔ candidates
**Scope**: all 146 entries in `docs/dev/todo/` screened; 36 opened in full by the agent; 3 re-read by
the parent because their verdicts carry consequences (a ✅ deletes a shared backlog file).

#### Result: 146 screened · 132 unrelated · 14 relevant · **0 claimed** · **0 blocking**

**No entry anywhere in `docs/dev/todo/` carries a `Claimed by:` marker.** That field is a
code-issues convention; the backlog README documents only `**Status:** Resolved`. So the entire
🚧 Claimed fork for this stack rests on the single code-issue record in Exploration 4.

| # | Entry | Nodes | Verdict | Shape |
|---|---|---|---|---|
| 1 | `2026-08-02-verify-rejects-a-token-with-a-tampered-signature-is-flaky-1-in-64.md` | **n1** | ✅ **Resolved here** | in-node — and *must* be, see below |
| 2 | `2026-08-16-the-daemon-s-secret-stores-still-truncate-in-place.md` | n7 | ✅ **Resolved here** (stale — already closed) | delete at n7's wrap |
| 3 | `2026-09-18-desktop-install-configures-no-identity.md` | n1, **n2** | ✅ **Resolved here** (n2 claims) | n2 *is* the node |
| 4 | `2026-09-10-ensure-owner-only-dir-no-longer-re-tightens-an-existing-auth-storage.md` | **n1** | ⚠ During → ℹ Answered | small, in-node |
| 5 | `2026-07-04-tddy-github-tddy-daemon.md` | n1, n6 | ⚠ During | recorded, not fixed |
| 6 | `2026-08-13-tokengenerator-generate-for-performs-no-authorization.md` | n1 | ⚠ During | recorded — see `api_secret` on a command line |
| 7 | `2026-08-15-session-worktree-sync-deliberate-gaps.md` | n1, n6 | ℹ Answered (in part) | recorded |
| 8 | `2026-08-15-remote-git-repo-over-livekit-deliberate-gaps.md` | n1, n6 | ℹ Answered (in part) | recorded |
| 9 | `2026-09-10-the-auth-and-livekit-modules-are-over-budget-and-were-moved-unsplit.md` | n1, n6 | ⚠ During | **explicitly not this stack's work** |
| 10 | `2026-08-16-models-agents-open-items-at-wrap.md` | n1, n2 | ⚠ During | recorded |
| 11 | `2026-09-05-from-2026-09-05-tauri-desktop-single-process-daemon.md` | n6, n1, n2 | ⚠ During | recorded |
| 12 | `2026-08-14-a-split-agent-s-join-token-carries-can-update-own-metadata-it-never-us.md` | n1 | ⚠ During | recorded |
| 13 | `2026-07-26-pr-stack-status-polling-and-stack-hygiene.md` | n1, n3 | ⚠ During | recorded |
| 14 | `2026-07-30-pr-stack-full-control-follow-ups.md` | n9 | ⚠ During | recorded |

#### Parent verification — three verdicts the agent got partly wrong

**#1, the flaky token test — the agent said ⚠ During; it is ✅, and n1 cannot decline it.**
The entry diagnoses the flake as a base64url canonicality property of the tag length. An HMAC-SHA256
tag is 32 bytes → 43 chars, whose final character carries a must-be-zero bit; the helper at `:232`
flips the last character `'A' ↔ 'B'`, and `'B'` is non-canonical, so the token fails to *decode*
about **1 run in 64**.

An **Ed25519 signature is 64 bytes**. 64 = 21·3 + 1, so the tail group is one byte → two characters,
and the final character carries 2 significant bits plus **4** that must be zero. Only four final
characters are then legal — `A`, `Q`, `g`, `w` — so the untampered tag ends in `'A'` roughly **1 time
in 4**, not 1 in 64.

> **n1 escalates this flake from ~1.5% to ~25% if it ships the v2 format without touching the
> helper.** It is in n1's own file (`session_token.rs:232`), the fix the entry already names is two
> lines (flip a middle character, or assert on the tampered bytes), and n1 rewrites the surrounding
> tests anyway. ✅ Resolved here, claimed by n1.

**#2, the secret stores — the agent said n7 closes it; it is already closed.**
Both call sites in the entry's own table are struck through, and it ends
`**Status:** Closed on node 10 for the remaining live vault (tddy-screen-sharing).` This is an entry
whose wrap missed it, which the classification table covers explicitly ("also the verdict for an
entry the tree shows already fixed, whose wrap missed it"). n7 deletes the very file it names, so n7
is the tidiest claimant — but the deletion is bookkeeping, not work, and n7's changeset must say so
rather than claiming credit for a fix that landed on `#unbundle` node 10 (#482).

**#4, `auth_storage` permissions — the agent said "runs straight into it"; it does not block n1.**
The entry's cost is that an `auth_storage` directory which has *drifted* group- or world-readable is
no longer re-tightened. n1 writes a **private signing key** there, so the question is live — but a
directory mode governs listing and traversal, not the contents of a `0600` file, and
`write_atomic_with_mode` sets the swap file's mode *before* writing rather than copying it from the
target. So n1's key is protected by its own mode even inside a loose directory. **Not blocking.**

What n1 *does* change is the entry's open decision. The entry lists three options and says the cost
"deserves a decision rather than a silent accept" — and it was written when the directory held a
plaintext token file. A private signing key is the new fact that settles it: **warn once at startup
when `auth_storage` is more permissive than `0700`**. Small, inside n1's own files, and it turns a
⚠ During into a ℹ Answered that n1 can close.

#### The three entries that argue *against* widening the stack

#7 and #8 both record that the LiveKit room-ownership model "does not exist today" and that closing
it is "a much larger change". #8 additionally documents that `build_auth_service_entry` builds an
**unsigned** `AuthServiceImpl` for `tddy-coder` — which Exploration 4's record #8 found from the
other direction.

n1 **partially answers** both, and that is worth stating precisely rather than overclaiming. #7's
sharpest line is that a client wanting to join `session-{id}` "has to hold `LIVEKIT_API_SECRET` and
mint for itself — which is the fleet's session-token signing key, and therefore a real widening of
the client trust surface." **n1 severs exactly that coupling**: after n1, `api_secret` is a LiveKit
room credential and nothing more, so holding it stops being equivalent to holding the session-token
signing key. The room-ownership model itself remains unbuilt, and neither entry is closed.

#6 sharpens the same point from a third angle: `spawner.rs` passes the raw `--livekit-api-secret` on
a child's command line, visible in `/proc/<pid>/cmdline`. Today that leaks the fleet signing key to
any local process; after n1 it leaks a room credential. A real reduction, still not a fix.

#9 is the one that tells this stack what **not** to do: `auth.rs` (1,200 lines) and
`session_room.rs` / `livekit_peer_discovery.rs` / `common_room_supervisor.rs` are all over the
500-line budget and were deliberately moved unsplit, with the entry recommending the split happen
"on a follow-up branch after the stack lands, not inside it." Read together with Exploration 4's
seven ⚠ During complexity records against those same files, the record is consistent and recent:
**no restructure node belongs in `#keyring`.**

#### Findings

1. **Nothing in the backlog blocks any node.** 14 relevant entries, zero ⛔, zero 🚧.
2. **Three entries close with this stack** — n1 takes two (the flake it would otherwise make four
   times worse, and the `auth_storage` posture decision its private key settles), n2 takes the
   desktop-identity entry it exists to fix, n7 takes one stale bookkeeping deletion.
3. **n1 is the centre of gravity of the whole scan**: 9 of the 14 backlog entries and 3 of the 8
   code issues name it or its files. That is consistent with C8 — n1 is the largest node, not the
   smallest, and the plan should stop describing it as a swap.
4. **Two independent records advise against splitting the oversized files this stack edits.** No
   restructure node; each node's changeset carries the constraint instead.

---

### Exploration 1.6 — `/analyze-code-issues` on the two unanalyzed packages — 2026-09-19

**Why this pass ran.** Step 2b found that 6 of the 16 packages this stack touches have never been
analyzed. Two of them — `tddy-github` and `tddy-daemon-kernel` — are in **n1's** path, and the
developer asked for them to be analyzed and the result carried on n1's PR.

**Green baseline first**, as the skill requires — never analyze a red tree:

```
./test -p tddy-github -p tddy-daemon-kernel
  tddy_daemon_kernel  unittests          78 passed  0 failed
  kernel_surface_acceptance.rs            4 passed  0 failed
  tddy_github         unittests          24 passed  0 failed
  github_token_retention_acceptance.rs    6 passed  0 failed
  ────────────────────────────────────  112 passed  0 failed
```

Scoped to these two packages, per the repo's verification policy. Whole-workspace health is CI's
answer, not this pass's.

**Coverage was not collected.** `tddy-tools analyze coverage` needs an instrumented rebuild, and the
scoped debug build above was already a cold compile. So **no record here carries a CRAP score**, and
each says so in its own `Coverage:` field. Per the record format, "not measured" is not "clean" —
every "0 tests enter it" below is a call-graph check across all packages, which is a different and
weaker claim than a coverage tier. A later pass with coverage should reconcile rather than re-open.

#### Production line counts (measured before the first `#[cfg(test)]`)

| Prod | Total | File |
|---:|---:|---|
| **1,447** | 2,511 | `tddy-daemon-kernel/src/config.rs` ⚠ 2.9× the 500 budget |
| 426 | 426 | `tddy-daemon-kernel/src/spawn_as_user.rs` |
| 266 | 266 | `tddy-daemon-kernel/src/peer_forwarding.rs` |
| 224 | 560 | `tddy-github/src/auth_service.rs` |
| 217 | 406 | `tddy-daemon-kernel/src/lib.rs` |
| 216 | 370 | `tddy-github/src/session_token.rs` |
| 131 | 131 | `tddy-daemon-kernel/src/user_paths.rs` |
| 129 | 129 | `tddy-github/src/real.rs` |
| 113 | 113 | `tddy-daemon-kernel/src/daemon_identity.rs` |
| 104 | 104 | `tddy-daemon-kernel/src/privilege_drop.rs` |
| 103 | 207 | `tddy-github/src/stub.rs` |
| 33 | 33 | `tddy-github/src/provider.rs` |
| 25 | 25 | `tddy-github/src/token_store.rs` |
| 16 | 16 | `tddy-github/src/lib.rs` |

Exactly one file is over budget. `auth.rs` in `tddy-daemon-auth` (1,200 lines) is the stack's other
oversized file and already has its own records; it is not this pass's.

#### The five records opened

| Record | Category | Metric |
|---|---|---|
| `tddy-daemon-kernel/.../heavy-dependency-livekit-peer-forwarding.md` | heavy-dependency | 1 of 7 modules pulls the SDK onto 14 dependents; 6 gain nothing |
| `tddy-daemon-kernel/.../oversized-file-config.md` | oversized-file | 1,447 prod lines; one contiguous 426-line seam at 545–970 |
| `tddy-daemon-kernel/.../missing-tests-privilege-drop-resolve-pty-os-user.md` | missing-tests | 0 tests, 3 production call sites, decides a uid |
| `tddy-daemon-kernel/.../misplaced-tests-privilege-drop.md` | misplaced-tests | 5 of 5 tests live in `tddy-session-lifecycle` |
| `tddy-github/.../missing-tests-real-exchange-code.md` | missing-tests | 71 lines, 2 HTTP calls, 0 of 6 error paths exercised |

#### The finding that changed a plan decision

`tddy-daemon-kernel/Cargo.toml` declares `livekit = "0.7"` with a comment naming the reason:
"`peer_forwarding`: one daemon serving another's RPC over the LiveKit common room." That reason is
the whole of it — every SDK reference in the crate is in that one file:

```
$ grep -rn "^use livekit\|^use tddy_livekit\|livekit::" packages/tddy-daemon-kernel/src/
peer_forwarding.rs:18   use livekit::Room;
peer_forwarding.rs:132  -> Result<tddy_livekit::RpcClient, tddy_rpc::Status>
peer_forwarding.rs:150  tddy_livekit::LiveKitRpcClientFactory::for_room(room_arc)
peer_forwarding.rs:201  (doc comment)
```

`config.rs` and `daemon_identity.rs` match a `livekit` grep only on `LiveKitConfig` — the crate's own
serde struct — and on a doc comment. Neither imports the SDK. Checked by reading the lines.

14 crates depend on the kernel. Six declare no LiveKit of their own and use none of its API:
`tddy-daemon-sandbox`, `tddy-model-registry`, `tddy-session-activity`, `tddy-worktree-service`,
`tddy-spawn`, `tddy-telegram`.

**This is not a build-time complaint. The cost already landed.** Two modules say so independently:

> depending on that crate from a subsystem crate would put the whole LiveKit SDK inside
> `tddy-tools`' `--no-default-features` in-jail build, which carries none.
> — `tddy-session-lifecycle/src/pty_runtime.rs:9-12`, and again at
>   `tddy-terminal-rpc/src/login_shell.rs:9-12`

`#unbundle` node 6 split `pty_runtime.rs` in half along that line — `login_shell` left, the
impersonation logic stayed — and the seam was chosen by what would drag the SDK rather than by what
belongs together. **n1's key material and `KeyDirectory` port therefore must not land in
`tddy-daemon-kernel`.** → **C14**.

#### The shortcut that looks free, and why the PRD rejects it

`tddy-daemon-auth`'s own manifest declares `livekit = "0.7"` **and** `tddy-livekit`, and
`oauth_loopback_tunnel.rs:11,21` already drives `livekit::prelude::{ParticipantIdentity, Room,
RoomEvent}` and `tddy_livekit::RpcClient` from inside the auth crate.

So the `dependency_boundary_unit.rs` constraint is narrower than it first reads: it forbids
`tddy-daemon-auth` on **`tddy-daemon-livekit`'s** path. It says nothing about auth reaching a Room
directly, which auth already does. n1 could skip the port entirely and keep the test green.

**Rejected.** The room is one transport; a desktop or single-daemon deployment has none. A port keeps
"where do I get a peer's public key" answerable without a room — which is exactly the coupling n1
exists to remove. The shortcut would re-create it one layer over.

#### A hand-off to n2

`RealGitHubProvider::exchange_code` (`real.rs:59`, 71 lines) posts to a hardcoded
`https://github.com/login/oauth/access_token` and gets a hardcoded `https://api.github.com/user`.
No base-URL field on the struct, none in the constructor. It is **untestable by construction**, and
all six of its error returns — the messages an operator reads while locked out — are unexercised.

n2 adds the **device authorization flow** to this same provider: `POST /login/device/code` plus a
polling `POST /login/oauth/access_token` with `authorization_pending`, `slow_down`, `expired_token`
and `access_denied`. Written against today's shape, that lands ~100 more lines with the same
property, and a polling state machine is harder to get right than a two-call exchange. **n2 opens
the base-URL seam before adding the flow, or it writes the seam twice.**

#### One record deliberately not opened

`DaemonConfig` carries **33 fields**, which meets a naive `god-object` threshold. Not opened: it is a
configuration aggregate root deserialized from one YAML document, and its fields are unrelated to
each other *by design*, because the YAML's top-level keys are. A true metric and a false finding. If
a later pass disagrees it should argue from the type's **methods**, not its field count.

#### A grep artifact, recorded so a re-run does not repeat it

A first pass filtered symbol references by lines matching `test|assert` and concluded
`wrap_argv_for_privilege_drop` was untested. **It is tested** — the call sits on a plain
`let wrapped = …` line inside a test body, with the `#[test]` three lines up. Deciding coverage from
the matched line's own text is wrong for any symbol whose result is bound before it is asserted,
which is most of them. Decide from the enclosing item. Caught before it reached a record; the
corrected finding (`resolve_pty_os_user` is the only untested symbol, 1 of 5) is what shipped.

#### Findings

1. **One finding constrains n1's wiring** (heavy-dependency → key material stays out of the kernel).
   Everything else is ⚠ During or unrelated. **Zero blockers, and n1 claims none of the five.**
2. **The two crates are not thinly tested** — 112 passing tests, and `config.rs`'s 1,063 test lines
   are most of the kernel's. The findings are about *specific* untested and misplaced things, not
   about a neglected package. Do not let the size of `config.rs` imply a CRAP problem it does not have.
3. **`#keyring` 2/9 gained a concrete prerequisite** it did not have before this pass.
4. **Four packages this stack touches remain unanalyzed**: `tddy-projects` (n5),
   `tddy-screen-sharing` (n7), `tddy-desktop` (n2), `tddy-web` (n2, n4, n5). Each named in its node's
   changeset; still not a blocker.

---

## Exploration 2 — the screen pattern and the service registration this node follows — 2026-09-19

**Question**: what does adding a ninth daemon-scoped screen and a new RPC service actually cost in
this codebase, and where would a new secret most plausibly leak?

### The web screen pattern — four pieces, all small

- `packages/tddy-web/src/routing/appRoutes.ts:88-130` — one route constant plus one `is*Path`
  predicate per screen. Pure string rules, no DOM, no React, which is why they carry their own unit
  tests in `appRoutes.test.ts`. The predicates are deliberately strict: `isTasksPath` matches
  `/tasks` and `/tasks/:id` but *not* `/tasks-archive`, via a `singleSegmentAfter` helper. A new
  predicate must follow that, and its test must pin the negative case.
- `packages/tddy-web/src/components/shell/DaemonNavMenu.tsx:55-140` — a flat list of `Button`s, each
  with a `role="menuitem"`, a `data-testid` (`shell-menu-projects`, `shell-menu-hosts`, …) and
  `onClick={() => go("/projects")}`. Eight entries today: Sessions, Worktrees, Tasks, Projects,
  Models & Agents, Hosts, VMs, LiveKit.
- `packages/tddy-web/src/index.tsx:483-499` — a ternary ladder selecting one `*AppPage`.
- `packages/tddy-web/src/components/<area>/<Area>AppPage.tsx` — the screen itself, taking
  `onNavigate`.

**Conclusion**: the plumbing is four lines plus a component. The work in this node is the service and
the three list states, not the route.

### Capability gating does *not* apply

`packages/tddy-web/docs/capability-gating.md` is explicit: `useHasCapability` is "the only place a
`capabilities` set is read", and that singularity is deliberate — capability information already
arrives from three directions, and "a fourth place that answered 'can I show video here' its own way
is precisely the drift that would make the wire-neutral model decorative". Gating exists for **media
and presence**, because a wire either carries a video track or does not.

Accounts is plain RPC. So the screen is **not** gated and this node adds **no** new reader. Worth
recording because gating it would look superficially consistent with the other daemon-scoped screens.

### The service registration pattern

`packages/tddy-daemon/src/runtime.rs:1021-1279` — every service is one `rpc_entries.push(...)`:
`terminal_session_entry()`, `session_files_entry()`, `project_entry()`, `build_livekit_entry(...)`,
`build_screen_sharing_entry(...)`, and so on, ending with the reflection entry. Adding one is a
single line — and one more line on a function already recorded at 806 lines
(`complexity-runtime-build`). Recorded, not claimed.

### The proto convention

`packages/tddy-service/proto/project.proto` is the model: a header comment explaining what the
service owns and what it imports, then `service X { rpc … }`, then its messages. **Every request's
first field is `string session_token = 1;`** — that is how a caller is identified, and after 3/9 it
is also what scopes the opened vault. There are 30 proto files; `accounts.proto` joins them, and
`tddy-service` has **no `docs/` directory**, so nothing there has been measured.

### Where a secret would leak, and the shape that prevents it

The plausible leak is a convenience RPC — `GetAccount` returning the credential "so the UI can show
the last four characters", or a `secret` field left on the summary for a later node to use. 3/9 made
"no store API returns a secret to an RPC response path" a boundary; this node is the first place with
an RPC that could.

The prevention is structural rather than procedural: `AccountSummary` has **no secret field** and
there is **no** get-one-credential RPC. `has_secret` is a single bit, which is what a UI needs to
distinguish "linked" from "linked but needs re-authorising". The test asserts over the **serialised**
response, so adding a field later fails the test rather than passing it silently.

### The three list outcomes

`VaultError::Locked` (3/9) is the deliberate no-fallback outcome when the login credential changed.
Above the store it has nowhere to be reported today. Collapsing it into an empty list would present a
recoverable, explainable failure as a normal empty state — a fallback in the sense CLAUDE.md forbids,
and the person would re-link accounts they already have rather than understand what happened. Empty,
locked and errored therefore stay distinct from `VaultError` all the way to the DOM.
