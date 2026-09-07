# Hosts screen

Every host tddy has a record of, reachable or not.

The [host directory](host-directory.md) answers "who can this page talk to right now", and every
host-selection surface reads it. The Hosts screen answers a different question: which machines does
the daemon *remember*. Its reason to exist is the **offline row** — a host that has left the common
room disappears from every other surface in tddy, which is exactly the moment an operator wants to
look at it.

The rows come from the daemon's durable registry
([host-registry.md](../../tddy-daemon/docs/host-registry.md)), not from the directory, so an offline
host is listed with its last-seen time rather than omitted.

## Route and entry point

| | |
|---|---|
| Route | `HOSTS_ROUTE = "/hosts"` and `isHostsPath` in `src/routing/appRoutes.ts` — an exact match, no sub-paths |
| Dispatch | one branch in the `src/index.tsx` route chain |
| Nav | `shell-menu-hosts` in `DaemonNavMenu`, between **Models & Agents** and **VMs** — beside the other machine-level screen, and clear of the Projects→Models adjacency the menu order pins |

## Components

`HostsAppPage` / `HostsScreen` follow the `VmsAppPage` / `VmsScreen` split: the page owns the data,
the screen renders what it is given.

**`HostsAppPage`** wraps `AppShell` (`title="Hosts"`, the default `scroll` variant,
`data-testid="hosts-app-page"`) and makes **one `ListKnownHosts` call per visit** against the
selected daemon's client. Registry membership changes rarely, so a stream would buy nothing and would
owe the `tx.closed()` teardown contract in `packages/tddy-codegen/docs/server-streaming.md`.

The effect depends on the memoised client and the session token, both stable while the screen is
looking at the same host under the same session, so it fires once on mount. It fires again only when
the selected host changes or the token is renewed — each of which genuinely invalidates the answer,
because the registry belongs to the daemon that was asked. A ref guard would suppress exactly those
legitimate re-reads, so the in-flight reply from a host just navigated away from is discarded by a
`current` flag instead. A failed call renders `hosts-error` above the table and leaves the previous
rows in place.

**`HostsScreen`** is presentational. `HostRow` is `{instanceId, label, online, lastSeenUnixMs,
reposBasePath, isLocal}` — the wire's `firstSeenUnixMs` is deliberately not mapped, because the
screen has no "known since" column and an unrendered field is surface every later change would have
to keep mapping for nothing.

**`HostRowTooling`** renders a host's tooling facts — the git identity its commits would carry, the
state of the GitHub CLI there, the ssh-agent it has, and whether a remote desktop on it is reachable.
It takes `{instanceId, git, githubCli, sshAgent, remoteDesktop}` and nothing else: the blocks arrive
as props rather than being fetched, so the section is a pure rendering of one `GetHostTooling` answer
and mounts wherever the row places it.

Its state machine is written **guard-first**: `unanswered()` runs before either block's own states
are consulted, and only `ProbeOutcome.OK` passes through to a finding. That direction is the point.
`ProbeOutcome` is a proto3 enum, therefore open, and this message grows — a newer daemon can send an
outcome this bundle's generated enum has never heard of. Listing the outcomes that mean "no finding"
would let an unknown value fall through and render "Not configured" with total confidence about a
probe whose result was not understood. Listing the one outcome that licenses a finding cannot.

An absent block (`undefined`) is the same admission as an unset outcome — nothing has answered for
this host yet — and renders a waiting marker rather than borrowing the shape of an answer.

**`HostRowSshAgent`** is the third section it mounts, taking `{instanceId, sshAgent}` — optional for
the same reason the git and `gh` blocks are, since a row renders before any probe has answered. It
repeats the outcome-first guard rather than sharing one, and for the same reason: only
`ProbeOutcome.OK` licenses a finding, so an outcome a newer daemon added and this bundle cannot name
renders "Could not check" instead of "No agent" — which would send an operator to start an agent
that is very possibly already running.

Past the guard it reads two more things in order. `reachable === false` is "No agent"; a reachable
agent with an empty `keys` is "No keys loaded". Both arrive with no keys, and only `reachable` tells
them apart, so the emptiness is never the thing consulted.

A held key renders its type, its **whole** fingerprint (monospaced, `break-all`) and its comment.
The fingerprint is not shortened because two keys can share any prefix of one, and a shortened one
matches nothing in an operator's own `ssh-add -l`. The comment is rendered in italics with no label
suggesting it locates anything: the agent does not know which file a key came from, and a comment is
free text.

**`HostRowRemoteDesktop`** is the fourth section, taking `{instanceId, readings}` — one reading per
probed protocol, and an absent block renders as an empty list rather than a fabricated one. Each
reading becomes one `hosts-row-<id>-<vnc|rdp>` span holding **two facts side by side**: the bridge
half (`Bridge ready` / `No bridge`) and the desktop half (`Desktop on :5900` / `No desktop on :5900`
/ `Could not check :5900`). They are never merged, because a host can serve a desktop tddy has no
bridge for and a host with the bridge installed can be serving nothing — and the two "no"s ask for
work on different things.

The desktop half repeats the same outcome-first guard the other sections use: anything other than
`ProbeOutcome.OK` reads "Could not check", with `failureReason` on the `title`. Keying off
`desktopReachable` alone would render "No desktop" for a host the daemon never reached, which is the
one thing this section exists not to say. The bridge half is rendered in every outcome, because it
is an existence check answered independently of the connect and a refused connect tells us nothing
new about it — where a reading never came back at all, the daemon sends the neutral `canBridge:
false` that renders as "No bridge".

`PROTOCOL_NAMES` maps `screen_sharing.proto`'s `Protocol` values (`1` → VNC, `2` → RDP), the same
values the daemon puts on the wire rather than a second enumeration of them. A reading for any other
value renders **nothing**: `protocol` is an open proto3 value, a newer daemon can probe a protocol
this bundle cannot name, and there is no label to put on it — inventing one, or rendering a nameless
row of facts, would be worse than omitting it.

The port is part of every desktop string rather than a separate cell. Only default ports are probed,
so "No desktop" on its own would read as authoritative about a host that simply serves elsewhere.

⚠ **Nothing mounts `HostRowTooling`.** No component in `src/` renders it, and nothing in `src/`
issues `GetHostTooling`; its only call sites are the Cypress component specs. The section's states
are covered, and the assembled path — row → RPC → probe → cells — has never run. Mounting it belongs
to the node that owns the row. Everything `HostRowTooling` mounts inherits that limit —
`HostRowSshAgent`, `HostRowRemoteDesktop`, and the add-key action below: each is wired, covered and
reachable over the wire, and an operator cannot yet open a screen that shows it.

## Adding a key to a host's agent

**`HostAddKeyAction`** is the surface that *starts* the add, and `HostRowSshAgent` mounts it beside
the agent summary. `AddHostKey` blocks for as long as the add takes — it raises a passphrase prompt,
waits for the ciphertext, unlocks the key and hands it to the agent — so this one component both
issues that call and, through `useHostPrompts`, renders the dialog the same call is waiting on.

**It is offered only where there is an agent to add to.** A host whose agent did not answer needs an
agent started, not a key loaded, and the control would do nothing there. `anAgentAnswered` reads the
same block `HostRowSshAgent` summarises and applies the same outcome-first guard: only
`ProbeOutcome.OK` with `reachable` licenses the action, so an outcome a newer daemon added and this
bundle cannot name never reads as a reachable agent. An agent already holding keys is still an agent
worth adding to — a host commonly needs a second key.

**The outcome is read from `AddHostKeyOutcome`, never from `added` alone.** `reportOf` is written per
arm rather than by echoing `failureReason`, because the enum exists precisely so that a daemon with
nothing to say still distinguishes the failures an operator would act on differently: a wrong
passphrase is worth retyping, an absent agent is not, an expired prompt means answering faster, and
an unreadable key means naming a different one. `failureReason` rides along as detail — except on
`UNSPECIFIED`, the daemon's own fallback for an answer it could not decrypt or an agent that refused,
where it is the only account there is.

**The prompt feed is read only while an add is in flight.** `useHostPrompts(adding ? instanceId :
null)` opens no stream for a screenful of hosts nobody is adding a key to, and the daemon replays
whatever is still outstanding to a subscriber that arrives late, so there is no window in which the
component can miss the question its own call raised.

**Cancel cancels.** It aborts the call client-side rather than leaving the row disabled for the
prompt's full 120 s TTL with nothing explaining why. The daemon has no withdraw path, so the prompt
simply expires unanswered.

`hostPromptsSubscription.ts` is the read loop, extracted from the hook as a plain function so that
closing it is observable: it holds the iterator by hand, aborts on unsubscribe, guards delivery on an
`unsubscribed` flag and releases in a `finally`. An `AbortError` from the call it cancelled itself is
not reported; a feed the daemon drops while the caller is still subscribed is.

## Host key trust

The answer is encrypted under the host's own public key, so which key that is decides everything.

**The fingerprint is derived in the browser from the key bytes, never taken from the wire.**
`hostKeyFingerprint.ts` computes `SHA256:<base64-no-pad>` over the received SPKI DER — the same digest
the daemon calls `spki_fingerprint` — and it is that value that is displayed and pinned.
`HostPromptEvent` carries an advertised fingerprint string beside the key, and both fields ride the
channel this feature exists to distrust, but only one of them encrypts anything. The advertised string
is public and non-secret, so pinning *it* makes the mechanism decorative: an active peer replays the
genuine fingerprint next to its own key, the check reports `unchanged`, the operator recognises the
value they verified out of band, and the passphrase is encrypted to the peer. A prompt whose two
halves disagree is refused outright.

**`hostKeyPinning.ts` is trust on first use, with six verdicts**, and the distinctions between them
are the design:

| Verdict | Means | Dialog |
|---|---|---|
| `pinned-now` | never seen this host; the derived key is now pinned | proceeds, says so |
| `unchanged` | the same key as last time | proceeds |
| `changed` | a different key from the pinned one | **blocks**, with an explicit accept path |
| `mismatched` | the key and the fingerprint it advertised are not the same key | **blocks**, no accept path |
| `unverified` | no conclusion available — no key presented, or storage unusable | warns, does **not** block |
| `unchecked` | this key's digest is not back yet | **blocks**, silently |

`mismatched` has **no accept path** because it is not a rotation and not a first sighting: a host
describing its own key gets it right, so two halves that disagree mean something rewrote one of them
in flight. An **empty** advertised field blocks for the same reason — "stripped in flight" and "an
older daemon" are indistinguishable from the browser.

`unverified` exists so that a missing conclusion is never reported as a positive one. Unusable storage
degrading to `pinned-now` would make a claim the dialog acts on ("first time seeing this host, key
recorded") when nothing was recorded — false on every later sighting too, leaving an active
substitution indistinguishable from ordinary first use *permanently*. An empty fingerprint takes the
same route rather than burning the one first-use trust slot.

`unchecked` is the caller's in-flight state, never returned by `verifyHostKey`, and it exists because
every alternative is a lie: `unverified` claims a check was attempted and reached nothing, and
`unchanged` claims continuity nobody established. A key whose digest is not back is a key nothing may
be encrypted to, because "which key is this?" has no answer yet.

**A checked key is held together with the prompt it arrived on.** `CheckedPromptKey` pairs the
verdict with the very prompt frame whose bytes were digested — compared by **object identity, not by
`prompt_id`**, because two frames can carry the same id and different keys. Only a pair whose prompt
is the one in hand is shown, so the fingerprint on screen is always the digest of the key that would
do the encrypting. Left as two pieces of state, the frame between a new key arriving and its digest
resolving shows the *previous* key's fingerprint and its reassuring `unchanged` verdict beside bytes
that would encrypt for somebody else — and a peer in the routing path widens that window at will by
emitting frames faster than SHA-256 resolves.

**`acceptChangedHostKey` is a deliberate two-step**: a checkbox confirming the operator verified the
new key with the host itself, gating the accept button. A rotated host key would otherwise lock an
operator out of their own host permanently — the daemon regenerating `host-prompt-key.pem` is enough
to cause it — and a one-click accept is a warning nobody reads.

⚠ This is the most arguable decision in the flow. Pinning makes an active key substitution *visible*,
not impossible, and gives nothing on a first-ever connection to an already-compromised host. The
alternative considered was to accept passive-only protection and disclose it in the dialog.

## The passphrase dialog

**`HostPassphraseDialog`** is server-initiated, unlike `ScreenSharingPassphraseDialog` and
`VncPassphraseDialog` — those are the UI deciding to ask before making a call; here the host raised
the question and is blocked until an answer comes back. It always names the host and shows the
derived fingerprint, so an operator can verify out of band before handing over a secret.

**The encryption happens inside the dialog, not in its caller.** `encryptForHost` runs there, so the
plaintext exists only inside this component's state and every path out of it carries ciphertext. A
caller handed the passphrase would be one `console.log` away from undoing the whole feature.

`keyContinuity` is the **only** thing the dialog is told about the pin, and both what it blocks on and
what it says. A `keyChanged: boolean` stood beside it while the verdict had no reader; two props
encoding the same fact can disagree, and the arm that would have gone unsaid — `unverified` — is
exactly the one worth saying.

## Encrypting on a plain-http origin

`encryptForHost.ts` and `hostKeyFingerprint.ts` both go through **`lib/subtleCrypto.ts`**, the single
audited entry point to `crypto.subtle`, which **refuses loudly on an insecure origin and names it as
the reason**. The refusal surfaces as the `underivable` verdict, so the dialog blocks with a stated
cause instead of throwing at submit.

There is deliberately **no fallback and must not grow one**: the only thing behind `subtle` here is a
passphrase being encrypted, so the only available fallback would hand that secret to every peer in
the room — the exact exposure the feature was built to remove.

⚠ **This bounds the feature, not just the code.** The daemon serves this bundle over `http://` on a
LAN address, which is not a secure context, so **add-key works only on a secure origin**. Making it
work over LAN needs TLS, which the daemon has nowhere today. Cypress cannot catch a regression here —
component tests run on `localhost`, which *is* a secure context — so the refusal is covered by units
that swap the `crypto` global. See
[insecure-origin-constraints.md](insecure-origin-constraints.md).

## Picking a key instead of recalling one

`HostAddKeyAction` calls `ListHostKeyCandidates` for the row's host — and only when there is an agent
to add to — and offers the returned keys by **type and fingerprint**, not by path alone: two paths can
hold the same key, and a path alone is what an operator was already failing to recall. A host that
will not list its keys is treated as one with no keys to offer, not as an error.

**The free-text path field stays beside the picker.** A key with no `.pub` file beside it is invisible
to a listing that never opens private keys, and it is still perfectly loadable — so the listing is a
convenience, not the boundary of what may be added.

**Both fields speak absolute paths, and `~` is expanded nowhere.** Not here, which does not know the
host's home; and not in the daemon, whose confinement is valuable precisely because it is a pure
function of the caller's input. Its refusal (`KEY_OUTSIDE_HOME`) names no path on purpose, so an
operator who sent `~/.ssh/id_ed25519` would be told only that their key must be inside a home it
already was inside. This side holds the context that makes that legible, so this side refuses it —
and the placeholder shows an absolute example rather than the tilde path the host is bound to reject.

## Rows

One `<tr>` per host, in a `hosts-table`, columns left to right:

| Column | Content |
|---|---|
| Host | `label`, plus a `(local)` marker when `isLocal` |
| Status | `Online` / `Offline` (offline is muted) |
| Last seen | `formatLastSeen(lastSeenUnixMs, nowUnixMs)` |
| Instance ID | `instanceId`, monospaced |
| Repos base path | `reposBasePath`, monospaced |

An empty list renders `hosts-empty` ("No hosts recorded yet.") instead of the table.

**Tooling cells.** `HostRowTooling` renders four sections under `hosts-row-<id>-tooling`, each
labelled with the tool it speaks for:

| Cell | Test id | Reading |
|---|---|---|
| git | `hosts-row-<id>-git` | `Name <email>` · `Not configured` · `Could not check` · `Not supported here` · `…` |
| gh | `hosts-row-<id>-gh` | the login · `Not authenticated` · `Not installed` · `Could not check` · `Not supported here` · `…` |
| ssh-agent | `hosts-row-<id>-ssh-agent` | one `hosts-row-<id>-ssh-key-<fingerprint>` per held key · `No keys loaded` · `No agent` · `Could not check` · `Not supported here` · `…` |
| remote desktop | `hosts-row-<id>-remote-desktop` | one `hosts-row-<id>-vnc` and one `hosts-row-<id>-rdp`, each `Bridge ready`/`No bridge` · `Desktop on :<port>`/`No desktop on :<port>`/`Could not check :<port>` |

"Could not check" and "Not configured" are deliberately different strings, because they send an
operator to two different places and only one of them is a host to go and fix. The desktop section
draws the same line twice over: `No bridge` against `No desktop`, and both against
`Could not check`.

The per-protocol test ids sit **beside** the section's own rather than under it
(`hosts-row-<id>-vnc`, not `hosts-row-<id>-remote-desktop-vnc`), which keeps a protocol assertion a
direct lookup instead of a nested one — and the section id stays available for asserting that the
block is there at all.

The `gh` label is static and present in **every** state, and that is what the `title` attribute
exists for: it names the login as *this host's*, distinguishing it from the tddy session user in
`UserAvatar` and from any `GITHUB_TOKEN`. Unlabelled, an authenticated `gh` renders as a bare login
beside the row's other identities and reads as whichever one the reader expected to see.

**Sort: online first, then by label** (`byLivenessThenLabel`, over a copy — the prop array is not
mutated). Ordering by liveness is the point: the hosts an operator can act on right now belong at
the top, and the rest stay listed rather than disappearing.

**The `(local)` marker can only come from the daemon.** Every daemon self-labels
`"<id> (this daemon)"` in its own advertisement, so in a list of hosts the label alone cannot say
which one is serving this page; `is_local` is the daemon's answer. Its test id
(`hosts-local-marker-<instanceId>`) sits **outside** the `hosts-row-` namespace on purpose: that
prefix belongs to the row elements, and a marker named under it would have to be excluded by hand
from any prefix match over rows.

`formatLastSeen` (`hostRowFormat.ts`) renders a short relative phrase — "just now" under a minute,
then whole minutes, hours and days, pluralised ("1 minute ago", never "1 minutes ago"). `nowUnixMs`
is passed in rather than read from the clock so a test pins a phrase instead of racing real time. A
stamp that is not in the past reads "just now": the daemon's clock and the browser's need not agree
to the second, and a host last seen "in 4 seconds" is noise. It accepts the wire's `bigint` as well
as a plain `number`; millisecond stamps are far below 2^53, so widening loses nothing.

## Testing

`cypress/component/HostsScreenAcceptance.cy.tsx` mounts
`mountWithRpc(withSelectedDaemon(<HostsAppPage />), backend)` against the in-memory backend, and
covers online, offline-with-last-seen, the sort, the local marker in both its positive and negative
case, reaching the screen from the nav menu, and the one-RPC-per-visit boundary.

`cypress/component/HostsScreenToolingAcceptance.cy.tsx` mounts `HostRowTooling` directly and covers
the six tooling states. One behaviour per test, and each state also **denies** the neighbouring state
it must not be confused with — a spec that only asserts its own state's string is present passes for
a component that collapses two states into one rendering, which is exactly the bug that sends an
operator to configure git on a host where git is not installed.

The page object `cypress/support/pages/hostsScreenPage.ts` selects rows **structurally**
(`[data-testid="hosts-table"] tbody tr`) rather than by a `hosts-row-` prefix. A prefix match over
that namespace also collects each row's own cells, and a `:not()` denylist patching around that would
silently over-match the moment a column is added.

`cypress/component/HostsScreenSshAgentAcceptance.cy.tsx` mounts `HostRowSshAgent` directly and
covers the key list, the three summary states told apart from one another, a comment rendered as a
comment rather than a path, an outcome this bundle cannot name, and a host that has not answered yet.

`hostSshAgentPage` on the same page object owns the ssh-agent section's selectors, including the
prefix match over `hosts-row-<id>-ssh-key-` that collects the held keys.

`cypress/component/HostsScreenRemoteDesktopAcceptance.cy.tsx` mounts `HostRowRemoteDesktop` directly
and covers four behaviours: both protocols reported for one host, a host that cannot bridge told
apart from one with nothing serving, the checked port named when reporting unreachable, and a failed
probe told apart from a negative finding.

Two of those tests assert a **denial** as well as a presence — `No desktop` must not also read
`No bridge`, and `Could not check` must not read `No desktop`. A spec asserting only that its own
string is present passes for a component that collapses the two facts into one verdict, which is the
exact bug the section exists to prevent. `aReading` builds a fully-populated reading and takes
overrides, so each test states only the field it is about.

`hostRemoteDesktopPage` on the page object exposes `section(id)` and `protocol(id, "vnc" | "rdp")`,
so a test names a protocol rather than a test id.

`hostToolingPage` on that same page object owns the tooling section's DOM contract, and
`expectGhLoginLabelledAsHosts` is why it has to. The cell renders a static `gh` label in every
state, so asserting its text contains `"gh"` proves nothing at all; what actually distinguishes this
host's login from the signed-in user is the `title`, and which attribute carries that is the page
object's business rather than a test body's.

`cypress/component/HostAddKeyAcceptance.cy.tsx` mounts the add-key flow against the in-memory
backend and carries the security claims. Three of them do the load-bearing work:

- **The submitted payload is decrypted and compared.** Asserting only that the ciphertext "does not
  contain the passphrase", with a loose length check, is satisfied by base64, a hash or random bytes.
  The spec decrypts the recorded `AnswerHostPrompt` payload with the test keypair's private half and
  asserts it equals the passphrase exactly, at 256 bytes. Unary calls *are* recorded by the in-memory
  backend interceptor, which is what makes this directly assertable.
- **The dialog never shows the previous key's fingerprint beside the key that would encrypt.** The
  spec holds `crypto.subtle.digest` open so it can stand inside the window a second prompt frame
  opens.
- **A replayed fingerprint beside a different key is caught**, which is the whole point of deriving
  the digest rather than trusting the advertised one.

`cypress/component/HostAddKeySelector.cy.tsx` covers the picker, including the two guards on
behaviour that must not regress: a typed path still works, and a host that refuses the listing is not
an error. `cypress/component/HostsScreenAddKeyAcceptance.cy.tsx` covers the flow from the row — where
the action is offered and where it is not, the dialog the host's question raises, the changed-key
block and its two-step accept, the accepted key becoming the next pin, the daemon's rejection reaching
the operator, and cancel freeing the row.

`src/rpc/hostPromptsSubscription.test.ts` pins the read loop directly: prompts delivered in order,
the call cancelled on unsubscribe, a feed cancelled having never raised a prompt (this feed's normal
state), delivery stopping after unsubscribe, the self-inflicted `AbortError` swallowed — and, so that
swallow is not vacuous, a feed the daemon drops while the caller is still subscribed being reported.

`src/lib/hostKeyPinning.test.ts`, `hostKeyFingerprint.test.ts` and `encryptForHost.test.ts` cover the
verdicts, the derivation and the insecure-origin refusal, the last by swapping the `crypto` global for
one with no `subtle`. Cypress cannot reach that path: component tests run on `localhost`.

`src/components/hosts/hostRowFormat.test.ts` pins the phrasing against a frozen clock;
`src/routing/appRoutes.test.ts` pins `isHostsPath` (positive, root, a sibling route, a sub-path).
`src/components/hosts` is listed in `package.json`'s `test:unit` directories, which is what makes
those unit tests run in CI.

## See also

- Daemon: [host-registry.md](../../tddy-daemon/docs/host-registry.md),
  [connection-service.md](../../tddy-daemon/docs/connection-service.md)
- Web: [host-directory.md](host-directory.md), [host-connections.md](host-connections.md)
- Daemon: [host-tooling-probe.md](../../tddy-daemon/docs/host-tooling-probe.md) — the probe behind
  `GetHostTooling`
- Daemon: [host-add-key.md](../../tddy-daemon/docs/host-add-key.md) — the prompt registry, the host
  keypair and the per-user key read behind the add
- Web: [insecure-origin-constraints.md](insecure-origin-constraints.md) — why `crypto.subtle` has no
  fallback here
- Feature: [docs/ft/web/hosts-screen-add-key.md](../../../docs/ft/web/hosts-screen-add-key.md)
- Feature: [docs/ft/web/hosts-screen.md](../../../docs/ft/web/hosts-screen.md)
- Feature: [docs/ft/web/hosts-screen-tooling.md](../../../docs/ft/web/hosts-screen-tooling.md)
