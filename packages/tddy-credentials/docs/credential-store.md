# Credential store

`tddy-credentials` holds `(provider, account)`-keyed credentials — a GitHub account's access token
today; a Cloudflare API key, a second GitHub account or a screen-sharing password later — in one
sealed file per user. At rest the file holds ciphertext and wrapped keys; nothing in it opens it.
What does is the user's **vault passphrase**, or the unlock key one of their browsers holds.

It depends on neither `tddy-daemon-auth` nor `tddy-livekit`, and not on `tddy-core` either, so a
later node can synchronise the file (`#keyring` 6/9) or store another provider's secret in it
(`#keyring` 7/9) without reaching through auth or pulling in the workflow stack.

## Why a passphrase, and not the login

The first design derived the vault's key from the GitHub access token a login returned. A GitHub
OAuth App mints a **new** token at every code or device exchange, so after a daemon restart every
fresh login — a new browser, a logged-out user, a refresh token past its window — derived a key
that did not open the vault, and was refused. Nothing a login carries is stable. The passphrase is
the one secret that is, so it is the one key-encryption key a user holds; the GitHub token is a
record *in* the vault, never key material.

## Record model

| Type | Meaning |
|---|---|
| `ProviderId` | which service — `github`, `cloudflare`, `screen-sharing`. A newtype, not an enum: adding a provider is data, not a breaking change |
| `AccountId` | which account *at* that provider. Today the GitHub login; distinct from the vault's owner, so one user can hold several |
| `CredentialRecord` | `{ provider, account, label, secret, metadata, updated_at }`. `label`, `metadata` **and** `secret` are one sealed unit |
| `SecretString` | the type of `secret`, and of a passphrase: prints `SecretString(<redacted>)`, is wiped on drop, has no `Serialize`/`Deserialize`, and is read through `expose()` |

`CredentialRecord` is not serialisable. It is sealed through a private mirror inside the vault, so
no response builder can serialise one by accident. `SessionVault::{put, get, list, remove}` read and
write records. `get` returns `None` only when no record is held; a record that will not
authenticate is `VaultError::Crypto`, never `None`.

## File and format

One file per **subject** (the user whose vault it is):
`CredentialStore::path_in(auth_storage, subject)` → `auth_storage/credentials-<hex subject>.vault`,
mode `0600` from its first byte, replaced by swap-then-rename (`src/atomic.rs`) so a crash or a full
disk never leaves a truncated vault. The subject is hex-encoded, so any login is a safe filename,
including on a case-insensitive filesystem.

```json
{
  "header": { "format_version": 2, "kdf": "argon2id", "kdf_version": 19,
              "m_cost_kib": 19456, "t_cost": 2, "p_cost": 1, "salt": "<hex 16>" },
  "wrapped_data_key": { "nonce": "<hex>", "ciphertext": "<hex>" },
  "unlock_slots": [ { "id": "<hex>", "nonce": "<hex>", "ciphertext": "<hex>" } ],
  "verifier": { "nonce": "<hex>", "ciphertext": "<hex>" },
  "records": [ { "id": "<hex>", "nonce": "<hex>", "ciphertext": "<hex>" } ]
}
```

- **Header.** It names the format and the whole passphrase derivation. Anything this build does
  not write — another format version, KDF, Argon2 version or any cost — is
  `VaultError::FormatMismatch { expected, found }`, checked **before** anything is derived: a lowered
  cost is a weakened derivation this build must not run, and a raised one could exhaust the daemon's
  memory. The header is also the passphrase slot's associated data, so an edited salt is a key that
  does not open the vault (`Locked`). Version 1 (the login-derived design) was never deployed
  outside tests and is not migrated; it is refused by name.
- **Records.** Each is ChaCha20-Poly1305 under the data key with a fresh 96-bit nonce. It is
  addressed by `id`, an HMAC of its length-prefixed provider and account under a subkey of the data
  key, so the file names no account in cleartext. The `id` is the record's associated data, and the
  identity inside the seal must hash back to it, so a record moved under another account's id does
  not open.
- **Verifier.** A known plaintext sealed under the data key. Every read and write re-checks it, so
  a session never writes records into a file its key no longer opens.

## Key derivation

```
DK             = 32 random bytes, generated once — seals every record
passphrase KEK = HKDF-Expand(PRK  = Argon2id(passphrase, header.salt, m=19456 KiB, t=2, p=1),
                             info = "tddy-credentials/v2/passphrase/" || subject)
unlock KEK     = HKDF-SHA256(salt = "", ikm = an unlock key U (32 random bytes),
                             info = "tddy-credentials/v2/unlock/" || slot id || "/" || subject)
wrapped DK     = AEAD(passphrase KEK, DK, aad = header)               — the passphrase slot
unlock slot    = AEAD(unlock KEK, DK, aad = "2/hkdf-sha256/" ‖ info)  — one per browser lineage
record id key  = HKDF-Expand(DK, "tddy-credentials/v2/record-id")
```

The Argon2id costs are OWASP's first recommended configuration and `argon2`'s default. They are
paid once per unlock, create or reset — never per request — and an offline guess costs the attacker
the same 19 MiB and two passes. Every HKDF `info` carries a label (`passphrase/`, `unlock/`,
`record-id`), so no subject or slot id can make one derivation's input read as another's. HKDF is
hand-rolled over `hmac` + `sha2` (checked against RFC 5869 A.1); `hkdf` would be tidier and needs
CLAUDE.md § ASK approval.

- **`CredentialStore::create`** creates only an absent file; one that exists is
  `AlreadyInitialized`. **`open_with_passphrase`** never creates: an absent file is
  `Uninitialized`, a wrong passphrase `Locked`, and nothing is changed either way.
  **`open_with_unlock_key`** opens through one slot.
- **`CredentialStore::reset`** is the forgotten-passphrase path. It renames the old file to
  `credentials-<hex>.locked-<unix seconds>.vault` beside it — **never deletes it**; whoever still
  knows the old passphrase can open it there — and creates a fresh vault under the new passphrase,
  both in one critical section. It does not parse the old file first, so it is also the way out of a
  vault this build cannot read. At most `MAX_SET_ASIDE_VAULTS` (5) of one subject's old vaults are
  kept: past that a reset is `TooManySetAside` and **nothing is deleted to make room** — an operator
  removes an old vault from the daemon's disk by hand first. Refusing was chosen over keeping the
  newest five because deleting an old vault destroys credentials its old passphrase still opens.
- **Zeroization.** Key material lives in `SecretBytes`, secret text in `SecretString`; both wipe
  themselves on drop with a volatile write plus a compiler fence. Transient plaintext buffers — an
  unwrapped key, a serialised record, Argon2's output — are wiped the same way. `zeroize` would be
  tidier and needs approval. ⚠ The cipher and HMAC instances hold their own copies of the key
  schedule, which are not wiped without `chacha20poly1305`'s `zeroize` feature (a TODO in
  `vault/crypto.rs`, recorded in
  [`2026-09-24-credential-vault-cipher-key-schedule-not-wiped.md`](../../../docs/dev/todo/2026-09-24-credential-vault-cipher-key-schedule-not-wiped.md)),
  and a passphrase that arrived in an RPC request also sits in that request's decode buffer, which
  this crate never sees. The two `auth` requests that carry one print it redacted (`tddy-service`
  generates them without prost's `Debug` derive).

**There is no daemon-held key.** A second way in that needs no user would let the daemon read
credentials with nobody present, which is the property this crate removes.

## The registry: states, logins, refreshes and logout

`SessionVaults` keeps each user's open handle by subject, because the reads that need a secret (PR
status) arrive later, with only a session token. For each user it answers `VaultState`:

| State | Meaning | What opens it |
|---|---|---|
| `Open` | the handle is in memory | — |
| `Locked` | a file exists and nothing has opened it since this daemon started | the passphrase, or a refresh presenting an unlock key |
| `Uninitialized` | no file | a first passphrase |

- **At login** (`retain`): the login's GitHub token is a record. With the vault open, it is sealed
  and the lineage is handed an unlock slot — no prompt. With it closed, the token is **held in
  memory**, never written in plaintext, and sealed by whichever of `unlock`, `create` or `reset`
  opens the vault next; the state tells the client to prompt. Pending tokens are held per user, not
  per lineage: session tokens are stateless, so the daemon cannot tell two lineages of one user
  apart, and a later token for the same account replaces an earlier one.
- **At unlock / create / reset**: the vault opens, everything pending is sealed (anything that fails
  to seal stays pending and the failure is reported), the lineage is handed an unlock slot, and the
  handle is kept.
- **Who may choose a passphrase.** Creating a vault or resetting one decides who controls it from
  then on, and a session token alone does not prove the caller holds the account: an access token
  crosses plain http and may have been copied. So `create` and `reset` need a credential **a fresh
  login left pending** for the vault (`NoFreshLogin` otherwise) — only a login proves possession of
  the GitHub account — and a reset is refused while the vault is open (`AlreadyOpen`: nothing was
  forgotten). A pending record is consumed by the create or reset that seals it, so each reset needs
  its own sign-in. ⚠ The gate is the pending record, not its age: a caller holding a copied access
  token for a user who signed in to a closed vault and has not unlocked it yet can still reset it.
- **One lock for every change of state** (`transitions`). A login's look-up-then-retain and an
  unlock's seal-then-register happen under it, so a login racing an unlock is either sealed into the
  vault it opened (and told `Open`, with a key) or held pending and told the vault is closed — never
  told `Open` with no key while its token waits unsealed. Key derivation happens outside it.
- **At a session refresh** (`reopen`): the client sends `U` back. The daemon opens the vault through
  that slot, which proves possession even when the vault is already open, registers it, and
  **rotates** the slot: the presented `U` opens nothing afterwards, and `U'` is returned. The proof
  is repeated under the write lock in the same critical section as the rotation, so two refreshes
  presenting one key cannot both rotate it.
- **Two tabs, one key.** Tabs of one browser share `localStorage` and refresh on load together, and
  a response can be lost. So for `ROTATION_GRACE` (30 s) after a rotation, the key it retired is
  answered with **the same** `U'` rather than refused, and both tabs end up holding the one key that
  opens the slot. A cross-tab lock in the page (`navigator.locks`) was considered and rejected: the
  Web Locks API exists only in secure contexts, and the dashboard is served over plain-http LAN
  origins. A key that fails outside the window is logged, and the refresh still succeeds, with an
  empty key and the vault's state — failing it would sign the operator out of everything for a
  credential-store problem.
- **An empty key means the key is dead, nothing else.** The auth service returns `""` only for a key
  that can never open this user's vault again: `Locked` (rotated, removed or evicted), another
  user's, or malformed. Any other failure — the file could not be read — says nothing about the key,
  so the presented key is handed back **unrotated** and the failure logged; the web client then
  keeps whatever key it has stored, which another tab may have rotated meanwhile.
- **At logout** (`forget`): the client sends `U`, and the daemon removes that lineage's slot. The key
  identifies the slot itself, so an expired access token does not keep a slot alive. When that was
  the **last** slot, no signed-in lineage can use the vault, and the open handle — its data key — is
  dropped. A lineage that signed in to a closed vault holds no key; its logout drops the credential
  its login left pending (`discard_pending`, for the user its access token names). Pending records
  are per user, so this also drops one another lineage of the same user left waiting — that lineage
  was told the vault is closed, and signs in again for its token to be kept.
- **A stale handle** — its file removed, or replaced by a reset from here or another process — is
  dropped the next time it is looked up, and the vault reads as whatever the disk now says. Any
  other failure to read is a real one and is reported where it happens.
- **Bound.** At most `MAX_UNLOCK_SLOTS` (16) slots per vault; past it the least recently used is
  evicted, and a rotation counts as a use.
- ⚠ TODO: a lineage that simply stops refreshing never logs out, so its vault stays open until the
  daemon exits, and a pending token that nobody unlocks lives as long —
  [`docs/dev/todo/2026-09-23-credential-vault-open-past-its-last-session.md`](../../../docs/dev/todo/2026-09-23-credential-vault-open-past-its-last-session.md).

**What guessing costs** (the auth service, `tddy-github`'s `auth_service/vault.rs`). Every
passphrase derivation — unlock, create, reset — runs on tokio's blocking pool behind a
service-wide semaphore of 2, so a flood of attempts queues rather than starving the RPC executor.
Wrong passphrases earn a per-user backoff: 3 are free, then each further attempt waits 2 s,
doubling to at most 15 min, refused as `RESOURCE_EXHAUSTED` with the retry-after in the message. The
count is in memory only, cleared by the right passphrase and by a daemon restart. A passphrase is
at most `MAX_PASSPHRASE_CHARS` (1024) characters, checked before anything is derived.

A stub login (`issues_usable_access_token() == false`) retains nothing, creates nothing, is handed no
unlock key and reports `VaultState::None` on the wire: its token is synthetic and a demo holds no
credential by construction.

## What the unlock key and the passphrase buy, and what they do not

This is the trade-off, stated plainly.

- **`U` plus a copy of the disk is the plaintext.** An unlock key opens its slot, and the slot is in
  the vault file. Anyone who holds both a copy of `U` and a copy of `auth_storage` — a backup, a
  snapshot, a stolen disk — can read every credential in that vault. The same is true of the
  passphrase: passphrase plus disk is the plaintext, at the cost of one Argon2id derivation.
- **`U` crosses plain http.** It is returned on every login, refresh and unlock, and sent back on
  every refresh and logout, over the same plain-http LAN origin the session token uses. Anyone who
  can read that traffic can take a copy. It sits in the browser's `localStorage` beside the refresh
  token.
- **The passphrase crosses it too.** `UnlockVault` and `ResetVault` carry it in the request body, so
  on a plain-http origin it is as exposed to the network as the session token. It is never logged,
  never stored and never returned.
- **Rotation bounds a copied `U` against the live file only.** A refresh rotates the slot, so a
  copied `U` stops opening the *current* file after the next refresh, and a logout removes the slot
  outright. **An old backup keeps the slots it was taken with**, and rotation cannot reach it: a `U`
  copied before the backup opens that backup for good. **A set-aside vault is the same**: a reset
  renames the old file with every unlock slot it had, so a `U` from before the reset plus that
  `.locked-*` file is still its plaintext, exactly as the old passphrase plus it is.
- **No rollback protection.** Someone who can write `auth_storage` can put back an older file — with
  a slot since removed, or an older record — and it opens. The vault authenticates content, not
  freshness.
- **One process.** The write lock is process-wide, not a file lock; two processes writing one
  `auth_storage` can lose each other's updates, as the token store this replaced could. `#keyring`
  6/9 (sync) has to decide this.

What it does buy: a disk **without** a copy of `U` or the passphrase is ciphertext, including every
backup of it; and a daemon at rest holds no key.

## Known limitations

Stated rather than fixed; each is bounded, and none lets a caller read a credential it could not
read otherwise.

- **Set-aside naming is checked, then renamed** (`vault/format.rs` `set_aside`). The free
  `.locked-<unix>[-n].vault` name is found with `exists()` and then `rename`d, and on unix a
  `rename` replaces an existing target. Within one daemon the process-wide write lock covers the
  gap; a second process writing the same `auth_storage` at the same second could overwrite a
  set-aside vault. The same one-process assumption as below.
- **Refreshes serialise daemon-wide.** `reopen` holds the `rotations` mutex — one for all users —
  across its file read, rotation write and fsync, inside an async handler. Every user's refreshes
  therefore queue behind each other, and each blocks a tokio worker for its I/O. Refreshes are rare
  (every few minutes per tab) and short, so this is a throughput limit, not a correctness one.
- **Two tabs that both unlock by passphrase each add a slot.** Both are handed their own `U`, and
  the second overwrites the first's in the shared `localStorage`, so the first slot is orphaned: it
  opens the vault for nobody, and lingers until `MAX_UNLOCK_SLOTS` eviction removes it.

## Two retention rules this crate carries

The trait this replaced (`GitHubTokenStore`, deleted with `github-tokens.json`) stated both in a doc
comment. They live here now.

1. **A secret never reaches an RPC response path.** The session token goes to a browser over a
   plain-http LAN origin, so a live `repo`-scoped GitHub credential is never carried in it or
   returned to the client. `SessionVault`'s reads exist for the daemon to *use*, and the types make
   the rule hold: `CredentialRecord` has no `Serialize` and its secret prints redacted. The unlock
   key is the one value from this crate that crosses the wire; see the trade-off above for what it
   is and is not.
2. **A login whose credential cannot be retained is reported, never silent.** A session minted
   without its credential is a half-login: the operator appears signed in while every
   credential-backed read reports itself unavailable, and nothing tells them why. So a login over a
   closed vault reports `Locked` or `Uninitialized`, and the client prompts for the passphrase; PR
   status says to unlock the credential vault. A **failed write** — `put` returns `Ok` only once the
   record is durably on disk — still fails the login. Only an `Io` failure, whose detail names
   server-side paths, is told to the client generically and logged in full.
