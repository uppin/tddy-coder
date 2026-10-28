# Changeset: context-sync-symlinks

PRD (target): [`docs/ft/daemon/agent-context-sync.md`](../../ft/daemon/agent-context-sync.md)
(amended 2026-10-28 — Design § *Two streaming RPCs* point 3, AC19–AC19c)

## Problem

Found live in split session `01a180c8-ff8c-7e42-8843-70d7ffeff84e` (agent on the Mac, codebase on
`mantas-udoo-ros`): the agent's context dir held `.agents/**` but no `.claude/skills`,
`.claude/commands` or `.claude/agents` — in this repository those three are symlinks into
`.agents/`, and Claude Code reads them only under `.claude/`. The agent started with the project's
rules and none of its skills, commands or subagents.

Cause: `walk_context_files` (`tddy-sandbox/src/context_manifest.rs`) followed a directory link only
when no other spelling of that directory had been visited ("listed once, under whichever spelling
the walk reached first"). `.agents/skills` came first in `read_dir` order, so `.claude/skills` was
dropped, and nothing about the link itself was ever carried — so the consumer could not recreate it.

## State B

- **Links travel as links.** `ContextEntry.symlink_target` / proto `ContextManifestEntry.symlink_target`
  (field 4, empty = ordinary file) name the root-relative target. A link entry carries no bytes:
  `size_bytes = 0`, `sha256 = sha256_hex(target)`, so retargeting is a diff like any edit.
- **An allow-listed link pulls its target in**, even when no glob names the target — one hop only.
  A target is withheld when it escapes the root, is excluded (`CONTEXT_EXCLUDE_GLOBS`) or matches the
  new `CONTEXT_SECRET_GLOBS` (`**/.env`, `**/.env.*`, `**/*.pem`, `**/*.key`, `**/.git`, `**/.git/**`)
  — the target **or any directory above it** (`config/.env/prod`).
- **Git gate on pulled-in paths.** A path that travels only because a link reaches it (the target,
  each file pulled in under it, a followed preamble link's target) travels only if
  `git check-ignore` does not report it ignored. One `git check-ignore --stdin -z` per walk stage;
  not a git repo / git fails → nothing is pulled in (`warn`). Glob-named paths are not asked.
- **Read-as exclusion across all links** (`is_withheld_through_a_link`): a file readable through
  any travelling link under an excluded or secret name is withheld, however a glob names it, and so
  is a link whose target or own path is one.
- **Reader** (`context_files.rs`) asks the walk's own answer (`context_reach`): it serves a path no
  glob names only when the walk lists it, so the git gate and the read-as exclusion agree with the
  manifest by construction. It also serves a glob-named path *through* a directory link that the
  manifest lists only under the target's path — a superset, the target still checked.
- **Consumer** (`context_sync.rs`, `context_dir.rs`) recreates links relative to their parent,
  re-points a retargeted one, removes a dropped link without following it, never writes a file or
  a link through a symlinked parent (`prepare_context_write`), and re-checks every served link
  (escape / secret / read-as-withheld → `PERMISSION_DENIED`). A served entry beneath a served link
  is refused outright — an honest walk never produces one.
- **Split fetch** (`svc_split_context_from_codebase_host.rs`) maps the proto field and excludes link
  entries from the batch read and the size/hash checks.

## Decisions (2026-10-28)

- ✅ **A directory-root link counts as allow-listed.** A link whose path is the root of a `dir/**`
  glob (`.claude` for `.claude/**`) travels as a link, so `.cursor -> .claude` and
  `.claude -> shared/claude` keep syncing. A file pulled in through it is withheld when the name it is
  *read by* through the link is excluded (`.claude/settings.local.json`).
- ✅ **A preamble file is never a link.** `CLAUDE.md` / `AGENTS.md` symlinks are followed and listed
  as ordinary files carrying their target's bytes, so the managed-codebase preamble always leads the
  file the agent reads. Found because this repository's own `CLAUDE.md -> AGENTS.md` was being
  recreated as a link. A peer serving a preamble path as a link is refused.
- ✅ **A link pulls in only what git does not ignore** (developer decision, 2026-10-28; resolves
  Validation 🟡1, option (b)). Untracked-but-not-ignored still travels; glob-named paths keep their
  AC16 behaviour (gitignored agent config named by a glob is served on purpose); the secrets list
  stays as a second filter. Ignore status comes from `git check-ignore --stdin -z` **without**
  `--no-index` — rules applied to untracked paths, a tracked file never "ignored" — batched per walk
  stage. **Fails closed**: a worktree that is not a git repo, or a git that cannot answer, pulls
  nothing in. This is a narrowing, not a fallback. The followed preamble target (`CLAUDE.md ->
  <unnamed>`) is held to the same gate, because its bytes are those of a path no glob names.
- ⏭️ **Pulled-in trees have no aggregate bound** — deferred:
  [`2026-10-28-context-sync-link-pulled-in-trees-are-unbounded.md`](../todo/2026-10-28-context-sync-link-pulled-in-trees-are-unbounded.md).
  The split fetch's total-size refusal already caps the split path.

## Technical debt

- `TODO(agent-context-sync)` in `create_context_link` (`context_manifest.rs`): non-unix hosts return
  `Unsupported` rather than creating a link.

## Package docs to update at wrap

- `packages/tddy-sandbox/docs/architecture.md` — the symlink paragraph: links are carried as link
  entries, one-hop target pull-in, `CONTEXT_SECRET_GLOBS` checked on every ancestor of a target,
  the git gate on pulled-in paths (`git check-ignore`, fail-closed), `is_withheld_through_a_link`,
  `context_reach`, and `prepare_context_write` (no write through a symlinked parent).
- `packages/tddy-session-files/docs/agent-context-sync.md` — reader asks `context_reach` (git gate +
  read-as exclusion); the reader's superset (a glob-named path through a directory link);
  syncer link recreation / deletion / receiving-side re-check; the two new manifest refusals (entry
  beneath a served link, read-as-withheld); `SyncStateEntry.symlink_target`;
  `context_entry_to_wire` / `context_entry_from_wire` as the one wire mapping.
- `docs/ft/daemon/agent-context-sync.md` — AC19 family: the git gate on pulled-in paths and the
  read-as exclusion across all links.

## Acceptance tests

Scoped runs, 2026-10-28, all ✅:

- [x] `tddy-sandbox/tests/context_manifest_acceptance.rs` — 24 passed (8 new link tests + 2 secret guards)
- [x] `tddy-session-files/tests/context_files_acceptance.rs` — 29 passed (2 new + 1 guard)
- [x] `tddy-session-lifecycle/tests/context_sync_acceptance.rs` — 26 passed (8 new)
- [x] `tddy-daemon/tests/remote_managed_worktree_cross_host_acceptance.rs` — 11 passed, incl.
  `a_split_sessions_context_dir_recreates_the_repos_claude_skills_link` (LiveKit)
- [x] `-p tddy-sandbox -p tddy-session-files -p tddy-session-split` — every suite 0 failed

Re-run after the Validation fixes, 2026-10-28, all ✅ (scoped):

- [x] `tddy-sandbox/tests/context_manifest_acceptance.rs` — 34 passed (+5: git gate ×2, secret
  ancestor, read-as ×2)
- [x] `tddy-session-files/tests/context_files_acceptance.rs` — 30 passed (+1: gitignored via link)
- [x] `tddy-session-lifecycle/tests/context_sync_acceptance.rs` — 30 passed (+2: beneath a link,
  nested-link escape)
- [x] `tddy-daemon/tests/remote_managed_worktree_cross_host_acceptance.rs` — 12 passed (one hit a
  LiveKit container startup timeout in the parallel run and passed re-run alone)
- [x] `-p tddy-sandbox -p tddy-session-files -p tddy-session-split` — every suite 0 failed; clippy
  `--all-targets -D warnings` clean

## TODO

- [x] Directory-root links + preamble-file rule
- [x] Validation findings 🔴1, 🟡1 (git gate), 🟡2, 🟡3 and the reader/duplication/`pub(crate)` notes
- [ ] Refactor pass over `context_manifest.rs` (~420 changed lines)
- [ ] Update the package docs listed above at wrap

## Files

- `packages/tddy-sandbox/src/{context_manifest.rs,context_dir.rs,lib.rs}`
- `packages/tddy-service/proto/session_files.proto`
- `packages/tddy-session-files/src/{context_files.rs,context_sync.rs}`
- `packages/tddy-session-split/src/svc_split_context_from_codebase_host.rs`
- Tests: the four suites above, plus `tddy-sandbox/tests/context_manifest_unit.rs` (field added)

## Validation Results

**Last run:** 2026-10-28. This was a read-only review of `git diff origin/master...HEAD` plus the uncommitted diff, using the four checklists `validate-changes`, `validate-tests`, `validate-prod-ready` and `analyze-clean-code`. Nothing was built or tested in this pass; the scoped test runs are recorded above. Items already planned are not repeated here: the three files over 500 lines, the unbounded pulled-in tree, the non-unix TODO and the name `LinkTargets`.

**Status:** ❌ one blocker, ⚠️ three should-fix items, then notes.

**Resolution (2026-10-28):** 🔴1, 🟡1, 🟡2, 🟡3 fixed, each pinned by a new failing-then-passing
test; the reader superset is documented in `context_files.rs`; the wire mapping is deduplicated;
`relative_link_target` (and `followed_preamble_target`, now unused outside the crate) are
`pub(crate)`. Still open from the notes: the function-length refactors, the canonicalize/read race
(`docs/dev/todo/` entry not yet written), the `held_manifest` repoint check and the link-entry hash
check.

### 🔴 Must fix

1. ✅ **Fixed** — `refuse_unlisted_paths` refuses any entry beneath a served link, and both writers
   and the copier go through `prepare_context_write` (each parent component a real directory, then
   canonical parent == `canonical(ctx)/logical parent`). Tests:
   `a_served_file_beneath_a_served_link_is_refused`,
   `nested_served_links_cannot_write_outside_the_context_dir`.
   **A peer can write outside the context dir by nesting one link inside another.** Files: `tddy-session-files/src/context_sync.rs` `refuse_unlisted_paths` (~:336), `write_context_link` (~:620), `write_context_file` (~:575).
   - **What `allow_listed_link` checks.** It checks only the strings: the link name is allow-listed and the target is relative with no `..`. `relative_link_target` then counts the `..` steps from the link's *logical* parent.
   - **What goes wrong.** `create_dir_all` and `symlink` work on the *physical* parent. When an earlier served link sits on that parent path, the physical parent is shallower than the logical one. The `..` steps then climb past the context dir.
   - **Example.** The peer serves three entries, which sort in this order:
     - `.claude/a/b -> .claude`. This is the ancestor-loop layout the walk itself allows.
     - `.claude/a/b/a/b/a/b/q -> t`. It is written as `../../../../../../../t`, but it physically sits in `ctx/.claude`, so it resolves six levels above `ctx`.
     - an ordinary file `.claude/a/b/a/b/a/b/q/<name>`. Every string gate passes, and `std::fs::write` writes it, with the daemon's privileges, into whatever directory `t` names outside the context dir.
   - **Gaps.** `write_context_file` never checks containment on the canonical path; only the delete path does. The changeset's claim that the consumer "never writes a file through a link" holds only for the final path component.
   - **Fix, receiving side.** Refuse any served entry, link or file, that has a strict ancestor which is a served link path. An honest walk can never produce one: it does not descend links, and a pulled-in tree drops the links inside it. So the rule costs nothing for honest peers.
   - **Fix, write time, in both writers.** Check that the parent's canonical path equals `canonical(ctx).join(logical parent)`, so no part of the parent is a symlink.
   - **Missing test.** Add a `context_sync_acceptance` test that serves the nested-link triple above and expects `PERMISSION_DENIED`, with nothing written outside the context dir.

### 🟡 Should fix

1. ✅ **Fixed, option (b)** — see *Decisions*: a path pulled in only through a link travels only if
   git does not ignore it. Tests: `a_link_to_a_gitignored_target_does_not_pull_it_in`,
   `a_gitignored_file_inside_a_pulled_in_tree_is_left_out`,
   `a_gitignored_file_reached_through_an_allow_listed_link_is_refused`.
   **The rule for link targets changed from an allow-list to a deny-list, and no recorded decision covers that.** Files: `context_manifest.rs:219` (`allow_listed_link`) and `:135` (`CONTEXT_SECRET_GLOBS`), `context_files.rs:530-537`.
   - **Before.** On master, the reader required the resolved target to be named by a glob ("both ends allow-listed").
   - **Now.** Any target inside the root that misses six secret shapes travels, and so does `CLAUDE.md -> <anything>`. These would cross hosts:
     - `.claude/x -> ../id_rsa`, `.npmrc`, `.netrc`, `.aws/credentials`, `*.p12` or `*.pfx`, `terraform.tfstate`, `config/credentials.json`, `secrets.yaml`, `.envrc`;
     - gitignored build output.
   - **Why it matters.** The reader's module doc gives the git gate as what kept exactly these files unreadable. Anyone who can create a link in the codebase worktree can now publish them into the agent's directory. That includes the agent itself, if its tools on the codebase host can run `ln -s`.
   - **The widening was not needed for the bug.** The reported layout, `.claude/skills -> ../.agents/skills`, has a target that every agent row already names (`.agents/**`). Carrying links as links fixes it under the old both-ends rule.
   - **Options:**
     - (a) keep the both-ends rule and recreate links only;
     - (b) pull a target in only when it is git-tracked, or at least not gitignored, for example through `worktree_files`' listing;
     - (c) keep the deny-list, and record it under *Decisions* with explicit developer consent and a much larger list.
   - **Recommendation:** (a) or (b).
2. ✅ **Fixed** — one predicate, `is_withheld_through_a_link`, applied after the walk, in the
   reader and in the receiving syncer. Tests:
   `a_directory_link_onto_an_allow_listed_tree_does_not_expose_the_hooks_file`,
   `a_file_readable_as_the_hooks_file_through_one_link_is_withheld_from_every_link`.
   **A link that stands in for a whole directory still exposes the daemon-owned hooks file whenever its target is glob-named too.** Files: `context_manifest.rs:615` (ordinary-file insert) and `:661` (pull-in withholding).
   - **What happens.** The read-as-name withholding is applied only inside `pull_in_link_target`, and only to entries that link pulls in itself. Take `.claude -> .agents/claude`, or `.claude -> .cursor` for Cursor. The file `.agents/claude/settings.local.json` has already been listed as an ordinary file under `.agents/**`, and the `continue` at :661 does not remove it.
   - **Result.** The agent reads it as `.claude/settings.local.json`. The daemon's hooks writer then lands through the `.claude` link, and the next sync tick overwrites it. That is the race `CONTEXT_EXCLUDE_GLOBS` exists to prevent.
   - **Second route.** A second allow-listed link straight to the file (`.agents/alias.json -> ../shared/claude/settings.local.json`) gets past the per-link check in `is_reached_through_link`. That check is `any` over links, so one link's read-as refusal is overridden by another link's pull-in.
   - **The layers agree, but all of them are wrong.** The reader (`named` branch) and the receiving syncer (`is_syncable_rel_path`) give the same wrong answer.
   - **Fix.** Add one predicate, *is this path withheld under its read-as name through **any** travelling directory link*, and apply it as a post-pass over `found` in the walk, in the reader's `named` and link branches, and in the syncer's acceptance check.
   - **Missing test.** Add one for `.claude -> .agents/claude` with `.agents/claude/settings.local.json` present.
3. ✅ **Fixed** — `is_withheld_at_any_depth` in `allow_listed_link` and
   `followed_preamble_target`. Test: `a_link_to_a_file_inside_a_secret_shaped_directory_is_not_listed`.
   **A secret-shaped directory above a link's target is never checked.** Files: `context_manifest.rs:225` and `:249`.
   - **The gap.** The withheld check is applied to the target's full path and to each segment *below* it, but not to the target's own parent directories. `.claude/x -> config/.env/prod` (or `keys.key/…`) travels: `**/.env` does not match `config/.env/prod`. Linking to `config` instead withholds the same file, because the walk checks it segment by segment.
   - **Fix.** Ask `is_withheld_from_links` of every prefix of `target`, in `allow_listed_link` and in `followed_preamble_target`.

### 🟢 Notes

- ✅ *Documented in the `context_files.rs` module doc; listed for the package doc above.* **Reads through a directory link (reader).** `context_files.rs:530`. The reader serves a named path *through* a directory link (`.claude/skills/tdd/SKILL.md`) that the manifest never lists, because the manifest lists the link and `.agents/skills/…`. The reader serves more than the manifest lists, but the resolved target is still checked. It is harmless; mention it in the package doc.
- **Race between resolving and reading.** The gap between `canonicalize` and `std::fs::read` in the reader, and between the walk's `symlink_metadata` and `File::open` in the manifest, predates this change. An actor who can write the worktree (a co-located agent) could swap a component for a symlink inside that window. Not introduced here; worth a `docs/dev/todo/` entry (open with `O_NOFOLLOW` or `openat`, or re-check the opened fd's path).
- **Recorded links on disk.** `held_manifest` (`context_sync.rs`) treats a recorded link as held when *any* symlink sits at the path, without comparing what `read_link` returns to the record. That is fine while the context dir is mounted read-only in the jail. If the dir is ever agent-writable, a repointed link would never be repaired.
- **Link-entry hash.** The peer's link-entry `sha256` and `size_bytes` are not checked against `sha256_hex(target)` and `0`. A peer that gets them wrong only causes churn, not a safety problem.
- **Deletes are safe.** Every delete re-checks the path's canonical parent, the parent cleanup cannot remove a directory reached through a link, and a link is unlinked rather than followed.
- **Preamble rule is consistent.** All four layers apply it the same way: the walk follows the link, the reader serves it through `followed_preamble_target`, the syncer refuses a served preamble link and `unlink_if_symlink` replaces an old one.
- **Production readiness:** ✅ no mocks or test-only branches. The only TODO is the known non-unix one. No `println!` or `dbg!`. The `#[serde(default)]` on `SyncStateEntry.symlink_target` is a documented back-compatibility default that reads an old record as files only, and it can only narrow what is held. `relative_link_target` is exported but used only inside `tddy-sandbox`; make it `pub(crate)` unless a consumer is planned. ✅ *Done.*
- **Clean code:**
  - Functions too long: `synchronize_directory` (`context_sync.rs:238`) is about 72 lines (must refactor; it grew by about 25 here). Extracting the fetch loop as `apply_fetches` and the delete loop would bring it under 40. `collect_allow_listed` (`context_manifest.rs:558`) is about 70 lines with nesting depth 5 in the symlink arm (must refactor); extracting `list_symlink_entry` would fix both. `resolve_allow_listed_context_file` (`context_files.rs:473`) is about 77 lines (it was about 60 before this change). `write_context_link` and `pull_in_link_target` are each about 45 lines (needs attention).
  - ✅ *Fixed: `context_entry_to_wire` / `context_entry_from_wire` in `context_files.rs`.* Duplication: the "empty string means an ordinary file" mapping between the wire and the entry is written in two places (`context_files.rs:156` and `svc_split_context_from_codebase_host.rs:271`). Put a single `From` or helper next to the proto conversion. The canonicalize-then-`log::debug!` skip pattern appears three times in `resolve_within_root`, `resolve_link` and `resolve_preamble_link`; that is acceptable.
- **Tests:** the new tests follow fluent-tests (Given/When/Then, names that describe behaviour, meaningful data) and are deterministic (temp dirs, no sleeps). Two tests assert two behaviours each: `a_split_sessions_context_dir_recreates_the_repos_claude_skills_link` checks the link and the read-through, and `a_split_sessions_linked_claude_md_still_leads_with_the_managed_codebase_preamble` checks that `CLAUDE.md` is not a symlink and checks its text. Both are acceptable for an expensive LiveKit end-to-end test. Missing coverage: the 🔴 nested-link escape, 🟡2 (a directory link onto a glob-named tree) and 🟡3 (a secret ancestor of the target).
