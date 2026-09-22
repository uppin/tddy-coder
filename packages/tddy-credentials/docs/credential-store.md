# Credential store

`tddy-credentials` holds `(provider, account)`-keyed credentials — a GitHub account's access token
today; a Cloudflare API key, a second GitHub account or a screen-sharing password later — in one
sealed file per user. At rest the file holds ciphertext and wrapped keys; nothing in it opens it.
The key that does comes from the user: their login, or the unlock key their browser holds.

It depends on neither `tddy-daemon-auth` nor `tddy-livekit`, so a later node can synchronise the
file (`#keyring` 6/9) or store another provider's secret in it (`#keyring` 7/9) without reaching
through auth.

## Record model

| Type | Meaning |
|---|---|
| `ProviderId` | which service — `github`, `cloudflare`, `screen-sharing`. A newtype, not an enum: adding a provider is data, not a breaking change |
| `AccountId` | which account *at* that provider. Today the GitHub login; distinct from the vault's owner, so one user can hold several |
| `CredentialRecord` | `{ provider, account, label, secret, metadata, updated_at }`. `label`, `metadata` **and** `secret` are one sealed unit |

`SessionVault::{put, get, list, remove}` read and write records. `get` returns `None` only when no
record is held; a record that will not authenticate is `VaultError::Crypto`, never `None`.

## File and format

One file per **subject** (the user whose vault it is):
`CredentialStore::path_in(auth_storage, subject)` → `auth_storage/credentials-<hex subject>.vault`,
mode `0600`, replaced only through `tddy_core::atomic_file::write_atomic_with_mode`, so a crash or
a full disk never leaves a truncated vault. The subject is hex-encoded, so any login is a safe
filename, including on a case-insensitive filesystem. One file per user is required, not
cosmetic: a vault opens for one subject only, so a shared file would lock every user after the
first out of theirs.

```json
{
  "header": { "format_version": 1, "kdf": "hkdf-sha256", "kdf_version": 1,
              "salt": "<hex 32>", "info": "tddy-credentials/v1/<subject>" },
  "wrapped_data_key": { "nonce": "<hex>", "ciphertext": "<hex>" },
  "unlock_slots": [ { "id": "<hex>", "nonce": "<hex>", "ciphertext": "<hex>" } ],
  "verifier": { "nonce": "<hex>", "ciphertext": "<hex>" },
  "records": [ { "id": "<hex>", "nonce": "<hex>", "ciphertext": "<hex>" } ]
}
```

- **Header.** It names the format and the key derivation. A version or KDF this build does not
  write is `VaultError::FormatMismatch { expected, found }`, checked before any decrypt: an
  operator told "wrong key" would re-link accounts for nothing, when the remedy is an upgrade. The
  header is also the login slot's associated data, so a salt or parameter edited on disk is a key
  that does not open the vault. It never becomes a vault that opens under a derivation someone
  else chose.
- **Records.** Each is ChaCha20-Poly1305 under the data key with a fresh nonce. It is addressed by
  `id`, an HMAC of its length-prefixed provider and account under a subkey of the data key, so the
  file names no account in cleartext. The `id` is the record's associated data, and the identity
  inside the seal must hash back to it, so a record cannot be moved into another account's slot
  and still open. Records are the last thing in the file.
- **Verifier.** A known plaintext sealed under the data key. Every read and write re-checks it, so
  a session never writes records into a file its key no longer opens.

## Key derivation

```
DK            = 32 random bytes, generated once — seals every record
login KEK     = HKDF-SHA256(salt = header.salt, ikm = the login credential,
                            info = "tddy-credentials/v1/" || subject)
unlock KEK    = HKDF-SHA256(salt = "", ikm = an unlock key U (32 random bytes),
                            info = "tddy-credentials/v1/unlock/" || slot id || "/" || subject)
wrapped DK    = AEAD(login KEK, DK, aad = header)            — the login slot
unlock slot   = AEAD(unlock KEK, DK, aad = format ‖ info)    — one per browser session lineage
```

HKDF is hand-rolled over `hmac` + `sha2` (checked against RFC 5869 A.1); `hkdf` would be tidier
and needs CLAUDE.md § ASK approval. Because the subject is in `info`, the same credential presented
for a different user does not open the vault.

- **`VaultError::Locked`** — the key does not unwrap its slot. That happens when the credential
  rotated, when a slot was rotated or removed since, or when the file belongs to another subject.
  **Nothing is changed and nothing is re-initialised.** `open_or_create` creates only an absent
  file; `open_existing` and `open_with_unlock_key` never create one.
- **`rewrap(ikm)`** — re-wraps the login slot under a fresh salt. Called on **every** successful
  login. Records and unlock slots are untouched. `SessionVaults::unlock` rewraps the handle already
  open for a user who is signed in elsewhere, which is how a rotated credential (a device login
  after a callback login, a re-approval) carries the vault over instead of locking it.
- **Zeroization.** Key material lives in `SecretBytes`, which zeroes itself on drop with a
  volatile write plus a compiler fence. Transient plaintext buffers — an unwrapped key, a
  serialised record — are wiped the same way. `zeroize` would be tidier and needs approval.
  ⚠ The cipher and HMAC instances hold their own copies of the key schedule, which are not wiped
  without `chacha20poly1305`'s `zeroize` feature (a TODO in `vault.rs`).

**There is no daemon-held key.** A second way in that needs no user would let the daemon read
credentials with nobody present, which is the property this crate removes.

## The session-scoped handle, and restarts

`SessionVault` is the opened vault: the only thing that can read a record. `SessionVaults` keeps
each user's open handle by subject, because the reads that need a secret (PR status) arrive later,
with only a session token.

- **At login** (`SessionVaults::unlock`, then `SessionVault::add_unlock_slot`): the daemon opens the
  vault or creates it if absent, rewraps it, and seals the GitHub token into it. It then adds an
  unlock slot and returns its key `U` to the client as `vault_unlock_key`. The daemon stores only
  the wrapped slot, never `U`.
- **At a session refresh** (`SessionVaults::reopen`): the client sends `U` back. The daemon opens
  the vault through that slot, which proves possession even when the vault is already open, and
  registers it if a restart had emptied the registry. It then **rotates** the slot: the presented
  `U` opens nothing afterwards, and the new `U'` is returned. A `U` that does not open its slot is
  logged, and the refresh still succeeds with an empty key. Failing the refresh would sign the
  operator out of everything for a credential-store problem. Credential-backed reads then stay
  unavailable until the next login.
- **At logout** (`SessionVaults::forget`): the client sends `U`, and the daemon removes that
  lineage's slot. The key identifies the slot itself, so an expired access token does not keep a
  slot alive.
- **Bound.** At most `MAX_UNLOCK_SLOTS` (16) slots per vault. Adding one past that evicts the least
  recently used; a rotation counts as a use. An evicted lineage's refresh after a restart cannot
  reopen the vault, the same as a lineage that never had a slot.
- **Between a restart and the first refresh**, a still-valid access token can reach PR status while
  the vault is not open. That resolves to *unavailable*, with a reason that says the credential is
  unlocked again at the next session refresh, not that the user must sign in. The web client
  refreshes on page load when it holds an unlock key, so a reload reopens the vault at once.
- ⚠ TODO: a registry entry is not evicted when a session ends. Session tokens are stateless, so the
  daemon learns of no ending; an entry lives until the daemon exits.

**The trade-off of `U`.** It crosses the same plain-http LAN origin the session token does, and it
sits in the browser's `localStorage` beside the refresh token. It is a wrap key, not a stored
credential: alone it opens nothing. It is useful only together with the vault file, which never
leaves the daemon's `auth_storage`. Someone holding both a copy of `U` and a copy of the disk can
open that vault, but they already hold the disk. Rotation on every refresh and removal on logout
bound how long a copied `U` stays useful.

A stub login (`issues_usable_access_token() == false`) is handed no unlock key and creates no vault,
because its token is synthetic and differs on every exchange. It still opens a vault that already
exists, through `open_existing`, and is refused `Locked` when it cannot. A login a vault refuses is
not let through for being a demo.

## Two retention rules this crate carries

The trait this replaced (`GitHubTokenStore`, deleted with `github-tokens.json`) stated both in a doc
comment. They live here now.

1. **A secret never reaches an RPC response path.** The session token goes to a browser over a
   plain-http LAN origin, so a live `repo`-scoped GitHub credential is never carried in it or
   returned to the client. `SessionVault`'s reads exist for the daemon to *use*. The unlock key
   is the one value from this crate that crosses the wire, and it is not a stored credential (see
   above).
2. **A failed write fails the login.** A session minted without its credential is a half-login:
   the operator appears signed in while every credential-backed read reports itself unavailable,
   and re-authenticating, the one remedy, is the one action they have no reason to attempt. So
   `put` returns `Ok` only once the record is durably on disk, and every failure is reported. A
   vault that cannot be opened (`Locked`, `FormatMismatch`, `Crypto`) fails the login too, and
   distinctly: its message names the lock rather than a generic refusal. Only an `Io` failure,
   whose detail names server-side paths, is told to the client generically and logged in full.
