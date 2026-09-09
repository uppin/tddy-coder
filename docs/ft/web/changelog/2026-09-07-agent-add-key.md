# 2026-09-07 — Load a key into a host's ssh-agent from the browser

A Hosts row whose ssh-agent answered now offers to **add a key to it**. The operator picks one of the
keys that host reports for their own OS user — by type and fingerprint, not by path alone — or types an
absolute path for a key no listing can see. The host then asks for the passphrase, a dialog appears
naming the host, the key and the host's public-key fingerprint, and the answer travels back
**encrypted under that host's own public key**.

Until now the row could say that a host's agent was reachable and holding nothing — precisely the state
in which a session there cannot clone or push — and offer nothing to do about it but open a terminal on
the machine.

**The passphrase is never persisted, never logged, never written to disk**, and is dropped as soon as
the key is unlocked. There is no "remember this passphrase", by decision. The plaintext exists in two
places only: the operator's own dialog, and the addressed host's process for the duration of the
unlock — so the LiveKit common room and any forwarding daemon carry ciphertext.

**The fingerprint shown is computed from the host's key, not read off the wire.** The question carries
both a key and a string describing it, and only the key encrypts anything; the advertised string is
public, so an active peer could replay the genuine one beside its own key and an operator comparing it
against what they verified out of band would see exactly what they expected. A question whose two halves
describe different keys is refused outright.

**A host's key is pinned on first sight, and a changed key blocks the flow** — released only by an
operator who confirms, in two deliberate steps, that they checked the new key with the host itself. It
is SSH's own model and the same warning an operator recognises from
`REMOTE HOST IDENTIFICATION HAS CHANGED`; a legitimate rotation is possible, a silent substitution is
not. Where no conclusion is available at all — no key presented, or a browser that will not store a pin
— the dialog says so and does not block, because a missing conclusion rendered as a first sighting
would stay false on every later sighting too.

⚠ **This makes an active key substitution visible, not impossible**, and gives no protection on a
first-ever connection to an already compromised host. The weaker alternative — passive-only protection,
disclosed in the dialog — was rejected rather than overlooked, and is worth arguing with.

**A question belongs to the operator who raised it**: shown to nobody else, answerable by nobody else,
answerable once, and expiring if unanswered. The identity is the GitHub user rather than the host OS
user, because two GitHub users mapped to one OS user would otherwise see and burn each other's
questions — including the private-key path one of them named.

**The key is read as the operator's own OS user, from inside their own home directory.** A session
mapped to one user cannot name another's key and have the daemon open it. "That key could not be read"
is one message for absent, unreadable and not-a-key alike, and names no path: told apart, the endpoint
becomes a file-existence probe over the operator's home. "That passphrase did not unlock the key" and
"this host could not decrypt your answer" are identical for a sharper reason — told apart they are a
decryption oracle against the host's long-lived key.

Removing a key from an agent, and generating one, are not offered.

⚠ **Add-key requires a secure browser origin.** The daemon serves tddy-web over plain `http://` on a
LAN address, where browsers withhold the Web Crypto API, so on such an origin the dialog blocks and
names it as the reason. There is deliberately no fallback: the only thing behind that API here is a
passphrase, and the only fallback available would send it in the clear to every peer in the room.
Making this work over a LAN address needs TLS on the daemon.

⚠ **Not yet visible in the app.** The action lives in the ssh-agent section of the tooling row, and
nothing in `packages/tddy-web/src` mounts that row — the limit the tooling work already recorded. All
of the above is implemented, covered and reachable over the wire, and no operator can open a screen
that shows it.

See [`hosts-screen-add-key.md`](../hosts-screen-add-key.md) and
[`hosts-screen-tooling.md`](../hosts-screen-tooling.md).
