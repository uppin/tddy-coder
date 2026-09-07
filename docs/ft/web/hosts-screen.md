# Hosts screen

The **Hosts** screen at `#/hosts` lists every host tddy knows about — including hosts that are not
currently connected — and says, per host, whether it is online now and when it was last seen. It is
reached from the hamburger menu, between **Projects** and **Models & Agents**.

## Motivation

Everywhere else in tddy, the set of hosts is the set of hosts that are *answering*: the live LiveKit
participant roster plus the daemon that served the page. A host that goes away simply vanishes — the
selector stops offering it, and nothing anywhere records that it ever existed. That is the correct
behaviour for choosing where to run a session, and it removes the row precisely when an operator
most wants to look at it: the machine is unreachable, and they want to know what it was and when it
was last alive.

The Hosts screen is the other half. Host existence is **durable** and daemon-side, so the list
survives a daemon restart and is the same list in every browser — deliberately not `localStorage`,
which is per-tab and would make one operator's history invisible to the next.

Every column the screen will grow — live resource telemetry, git identity, `gh` status, ssh-agent
keys, remote-desktop availability — is a fact about a host **row**. Without a durable row to hang
them on, none of them can be rendered.

## What is remembered

The daemon records a host the first time it sees one, from the same discovery path that maintains
the live peer roster, and it records **itself** at startup so the machine the operator is talking to
is always present.

Per host it keeps the label, the instance id, the repos base path, the attachment cap, when the host
was first seen and when it was last seen.

- **A host is never removed.** Going offline stamps the last-seen time and nothing else. There is no
  delete action; if one is ever wanted it is an explicit operator gesture, not a side effect of a
  machine being switched off.
- **First-seen is never re-stamped**, so "known since" keeps its meaning across restarts of both
  daemons.
- **Online is never stored.** A persisted online flag is wrong the moment a daemon exits without
  notice, so liveness is resolved each time the list is read, by intersecting the remembered hosts
  with the live roster.
- **One machine is one row**, across its own restarts. Hosts are remembered under an identity that
  survives a restart rather than the per-run identity used for routing.
- **A last-seen time cannot age past an hour while its host is visible.** A host that stays up
  indefinitely would otherwise never get a fresh stamp, and would read as "last seen a month ago"
  the moment it did go away.
- **A host that is answering right now is always listed**, whether or not it has been recorded yet.

## The screen

One row per host, sorted **online first, then alphabetically by label** — the hosts an operator can
act on right now belong at the top, and the rest stay listed rather than disappearing.

| Column | Content |
|---|---|
| Host | The host's label, with a `(local)` marker on the daemon serving the page |
| Status | Online / Offline, offline rendered muted |
| Last seen | A short relative phrase: "just now", "3 minutes ago", "2 hours ago", "5 days ago" |
| Instance ID | The id every host-addressed RPC is keyed on |
| Repos base path | Where that host keeps its checkouts |

With nothing recorded yet, the screen says so ("No hosts recorded yet.") rather than showing an empty
table.

The screen reads the list **once per visit** — registry membership changes rarely, so there is
nothing to stream — and re-reads it when the selected host changes, because the list belongs to the
daemon that was asked.

## Scope

- The screen does **not** change how a host is named, selected or reached. The host directory, the
  daemon selector and per-host connections are untouched; this screen opens no connection per row.
- Per-host live telemetry, host resources, git and `gh` identity, ssh-agent keys and remote-desktop
  availability are not part of it. They are the columns that build on this row — see
  [#454](https://github.com/uppin/tddy-coder/pull/454) through
  [#460](https://github.com/uppin/tddy-coder/pull/460).
- A host recorded by a peer daemon that publishes no durable identity of its own, and that restarts
  under a fresh per-run id, leaves one offline row behind per restart. Any daemon built from this
  code publishes one.

## Acceptance criteria

1. ✅ `#/hosts` renders the Hosts screen inside the app shell, reachable from the hamburger menu.
2. ✅ A host currently in the live roster renders as **online**.
3. ✅ A host that is remembered but absent from the live roster renders as **offline**, with its
   last-seen time — it is **not** omitted.
4. ✅ The list survives a daemon restart: a host recorded before the restart is still listed after
   it, as offline, with its original first-seen time.
5. ✅ Going offline updates the last-seen time and never removes the host.
6. ✅ The local daemon always appears, marked as the local host.
7. ✅ Listing known hosts rejects an invalid session token.
8. ✅ Rows sort online-first, then by label.
9. ✅ A corrupt or missing stored list yields an empty list, not a daemon failure.

## Related documentation

- [app-shell.md](./app-shell.md) — the shell and navigation menu this screen renders inside
- [url-state-routing.md](./url-state-routing.md) — the `#/hosts` path in the URL grammar
- [projects-screen-multi-host.md](./projects-screen-multi-host.md) — "a host is a daemon instance",
  which this screen extends from *currently connected* to *ever seen*
- [host-stats-footer.md](./host-stats-footer.md) — host telemetry for the selected daemon
- [hosts-screen-add-key.md](./hosts-screen-add-key.md) — loading a key into a host's ssh-agent
  from a row
- Technical: [hosts-screen.md](../../../packages/tddy-web/docs/hosts-screen.md),
  [host-registry.md](../../../packages/tddy-daemon/docs/host-registry.md)
