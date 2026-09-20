# 2026-09-20 — Remove the `specialized_agents` tombstone from `SessionMetadata`

**Category:** Future enhancement
**Source:** sandboxed-codebase-managed-workflow changeset, 2026-09-20

`SessionMetadata::legacy_specialized_agents`
(`packages/tddy-core/src/session_metadata.rs:79`) is a read-and-discard tombstone for the
`specialized_agents` key that `agents` + `agents_rev` superseded on 2026-08-17. Nothing reads it —
a workspace-wide grep finds no consumer other than its own declaration, and it is
`skip_serializing`, so it is never written back.

## Why it cannot simply be deleted

`SessionMetadata` is `#[serde(deny_unknown_fields)]`. Drop the field and any `.session.yaml` still
carrying the key fails to parse — which the daemon and tddy-web read as **"not a session"**,
dropping a session whose agent process may still be alive. The failure is silent and costs the
operator a running session, so this is a data question, not a code one.

## Measured, 2026-09-20

On one developer machine:

| | |
|---|---|
| `.session.yaml` files found | 61 |
| still carrying `specialized_agents:` | **2** |

So the key is not yet extinct even locally, a month after it was superseded. The one-line deletion
is not safe today.

## What would make it safe

Either of:

- **Let them age out.** Re-measure; delete the field once no `.session.yaml` in any deployment
  carries the key. Cheap, but needs someone to actually re-measure rather than assume.
- **Migrate on read.** Rewrite the file without the key the first time a session carrying it is
  loaded, so the population drains itself. More code, but it terminates on its own and does not
  depend on a deployment survey.

Note the tombstone pattern will recur — `deny_unknown_fields` makes every removed key a
compatibility event — so whichever route is taken is worth writing down as the house rule for
retiring a persisted field.

## Why it was not done in the originating PR

The originating changeset asked whether this was low effort. It is not: the edit is one line, the
safety argument is a survey of deployed state that a single machine cannot settle.
