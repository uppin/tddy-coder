# 2026-09-06 — Each host's ssh-agent and the keys it holds

Every Hosts row reports whether an **ssh-agent is reachable** for that host's OS user and, when one
is, **which keys it holds** — key type, `SHA256:` fingerprint and comment. Until now a session that
could not clone or push for want of a usable key produced a generic git failure and nothing about
why.

Four states, kept distinct because each sends an operator somewhere different: keys listed · an
agent holding none · no agent reachable · the probe could not run. **"No keys loaded" is only ever
said by an agent that answered** — every other emptiness reads "No agent" or "Could not check". An
empty agent wants a key; an absent one wants an agent started; a failed probe wants looking at on the
daemon side.

**A key's originating file is never shown, because the agent does not know it.** What a row can
honestly say is the type, the whole fingerprint — never shortened, since two keys can share any
prefix of one — and the comment, which is free text set when the key was generated and is rendered as
free text rather than as a path. A certificate is listed under its own type with the fingerprint of
the key it certifies, so the row matches what the operator's own `ssh-add -l` prints.

Read-only. No key is added, removed or unlocked from this screen, no passphrase is prompted for, and
the daemon still runs three fixed probes rather than any "run this on host X" primitive.

⚠ **Not yet visible in the app.** Nothing in `packages/tddy-web/src` mounts the tooling row or issues
`GetHostTooling`; the section exists, is covered, and is reachable over the wire, but no operator can
open a screen that shows it. Mounting it belongs to the node that owns the Hosts row.

⚠ **On a host installed with `./install --systemd`** the daemon runs as an unprivileged service
account, and a user-session agent's socket is reachable only by its owner and by root — so those rows
read "could not check" with the permission error, not "No agent" and not an empty key list.

See [`hosts-screen-tooling.md`](../hosts-screen-tooling.md) and
[`hosts-screen.md`](../hosts-screen.md).
