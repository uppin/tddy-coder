# A credential vault stays open after its last session lapses without a logout

**Category:** Key lifetime
**Source:** `#keyring` 3/9 validation finding V3 (recorded in `docs/dev/changesets/2026-09-24-keyring-store.md`)
**Package:** `tddy-credentials`
**File:** `packages/tddy-credentials/src/sessions.rs`

## What happens

`SessionVaults` keeps each user's opened vault — its data key — in memory, so PR-status reads can
use the stored GitHub token. It drops that handle when the vault's **last unlock slot is removed**,
which is what the last lineage's logout does. A lineage that never logs out — a browser closed for
good, a refresh token that lapses after its seven-day window — removes nothing, so the vault stays
open until the daemon exits, and the daemon can keep acting on the credential with nobody signed in.

## Why it is not fixed in 3/9

Session tokens are stateless: the daemon learns of no session ending except a logout. The candidates
each need a decision:

- **A TTL tied to the refresh-token window** — drop a handle no refresh or unlock has touched for
  seven days. Simple; the bound is a week, not "nobody is present".
- **Touch on every authenticated RPC** — a shorter idle timeout, at the cost of plumbing a
  last-seen time through the RPC gate.
- **Evict at slot eviction too** — the LRU bound (`MAX_UNLOCK_SLOTS`) already retires lineages; a
  vault whose every slot was evicted is as unused as one whose every lineage logged out.

## Also held

A login that finds the vault closed holds its GitHub token in memory (`pending`) until the vault is
unlocked, reset or created. That, too, lives until the daemon exits when nobody unlocks it.

**Narrowed by #510's wrap (S5).** A lineage that signed in to a closed vault holds no unlock key,
so its logout used to remove nothing and its token stayed pending. A logout with no unlock key now
drops that user's pending records (`SessionVaults::discard_pending`, verified by the logout's access
token). What remains is the same as for an open vault: a lineage that never logs out, or whose
access token has expired by the time it does, leaves its pending token until the next unlock or
restart.

