# 2026-09-06 — The development TODO is a directory of one file per item

**Type:** Architecture

`docs/dev/TODO.md` had grown to 2,171 lines holding 98 items. It was the last shared append-point in
the repo's documentation: every branch recording a finding edited the same file, so two branches
landing in one merge window conflicted, and the union merge driver's repairs stranded blocks
mid-file — the exact failure that moved changesets and changelogs to one-file-per-entry.

It is now `docs/dev/todo/`, `YYYY-MM-DD-<slug>.md` per item, matching
[`docs/dev/changesets/`](../changesets/) and the product changelogs. See
[changelog-merge-hygiene.md](../guides/changelog-merge-hygiene.md).

- **98 items split**, each keeping its body verbatim. `**Category:**` preserves the old `##`
  grouping (`Known failing test`, `Future enhancement`, `Deferred from \`optional-livekit\` (#449)`),
  `**Status:** Resolved` the three struck-through entries, `**Source:**` the originating changeset.
- **Dates** come from each heading — `(source: …, YYYY-MM-DD)`, `(found …)`, `(deferred …)`. The one
  item sourced "as above" inherits its predecessor's date rather than defaulting to today.
- **Relative links rewritten** one level deeper (`../ft/…` → `../../ft/…`).
- **`docs/dev/TODO.md` remains as a stub** pointing at the directory. 17 already-wrapped changesets
  link to it, and wrapped changesets are history — rewriting them to fix a path would edit the record
  rather than the repo. Nothing routine touches the stub and no item is ever added to it.
- **No index**, deliberately — an index is a shared append-point, which is what this removes.

Planning now reads the directory as well as writing to it: Step 2b of
`.agents/skills/planning/references/planning-phase.md` scans it before a changeset is written, so an
item in the path of new work is found at planning time rather than during `/green`.
