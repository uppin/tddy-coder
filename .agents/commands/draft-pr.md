---
description: Create a reviewable DRAFT PR from current changes, skipping all checks
---
User wants a fast, reviewable **draft** pull request from the current changes. This command
**intentionally skips all validation** — no `cargo fmt`, no `cargo clippy -- -D warnings`, no
`cargo build`, no `./test`, no `/validate-changes`, no `/pr-wrap` or any wrap/validation workflow. The
point is to get eyes on work-in-progress quickly; the draft state signals the branch is not
merge-ready.

**Do NOT** run formatting, linting, compilation, tests, or validation steps, and do not block on
failing or unfinished checks. Skipping them is the purpose of this command — do not silently run them
anyway. (Pre-commit hooks still run: **never** `--no-verify`. If a hook blocks the commit, fix what it
flags and say that you did — that is the hook's gate, not a validation step you chose to add.)

## Why a draft PR matters in a stack

A stacked PR blocks its dependents for as long as it is unfinished. Publishing the *interface* early is
the mitigation, and it is written down: each PR's changeset carries a
**`## Draft PR contract`** heading naming exactly what lands first — the API surface plus its failing
tests, enough to open a draft PR against — so dependents can branch off a **real ref** and code against
a real signature while the implementation continues **in the same PR**. See
the `pr-stack` skill § *Per-PR documents*.

Two things follow, and both are load-bearing:

- **A draft PR is not a stubs-only PR.** The node still ships its own implementation and tests before
  it merges. Splitting a node into "surface now, behaviour later" is forbidden by the
  the `pr-stack` skill § *The PR boundary contract* —
  every node must be independently reviewable and independently mergeable. The draft is *early
  publication inside* one node, not a layer of its own.
- **Opening a PR as a draft is a human act.** `GithubPrApi::create_pr` has no `draft` parameter and the
  nothing else sets one. Drafts are read correctly everywhere (a draft counts as `open`
  records a draft as `open`, so a draft node is a live node), but this command — or `gh pr create
  --draft` — is what creates one.

If this branch is part of a stack and its changeset has a `## Draft PR contract`
section, mention in the PR body which part of that contract this draft publishes.

You should:

1. Review the current changes (`git status`, `git diff --stat`) and stage the files relevant to this
   work. Only include relevant files; if unrelated changes are present, ask the user before including
   them.
2. **Bring the context documentation up to date** before committing, by conducting the
   `update-context-docs` workflow (`.agents/commands/update-context-docs.md`): update the PRD (in
   `docs/ft/{product-area}/1-WIP/`) if one exists, otherwise the feature doc in `docs/ft/{product-area}/`;
   update the active changeset in `docs/dev/1-WIP/` (Scope / milestone / acceptance-test checkboxes,
   status sections, test-result counts, implementation evidence) to reflect the current implementation
   state. Never edit `packages/*/docs/` directly — that goes through the changeset workflow. This is
   documentation sync, **not** a validation gate — do not run tests, clippy or a build to satisfy it;
   just make the docs match reality (including honestly noting anything WIP or not-yet-passing, marked
   with a visible indicator). Stage the updated docs so the draft PR ships with current context
   documentation.
3. Commit with a clear Markdown summary of what changed and why, following the repo's commit
   conventions.
4. Push to a remote branch (create one if needed — see branch selection below).
5. Create a **draft** PR with `gh pr create --draft --base <base>`, giving it a concise title and a
   Markdown body that summarizes the scope and explicitly notes this is a WIP draft with checks
   skipped.
   - **Detect the base; do not assume `master`.** If an open PR's `headRefName` is an ancestor of
     `HEAD`, this branch is stacked on it and that branch is the base — the same detection `/pr` uses
     (`gh pr list --state open --json number,headRefName,baseRefName` plus
     `git merge-base --is-ancestor origin/<headRefName> HEAD`). Confirm a non-`master` base with the
     user before creating.
6. Return the PR URL to the user.

## Working branch selection

1. **If on main/master**: create a new branch first (`git switch -c <feature-branch-name>`), commit
   there, and push with tracking (`git push -u origin <feature-branch-name>`). Never push master to a
   differently-named remote branch.
2. **If on a different, unrelated branch**: ask whether to use it or create a new one.
3. **If on a branch the user created for this work**: use it as-is.

## Guardrails

- Never use `--no-verify`.
- This is a **draft** — never mark it ready-for-review and never merge it. Never change any *other*
  PR's draft/ready state either; a predecessor's state is not yours to flip.
- Do not comment `#automerge` or `#forcemerge` on a draft
  ([`docs/dev/guides/ci.md` § Automerge](../../docs/dev/guides/ci.md#automerge)).
- Keep the commit scoped to relevant files only. Mark anything temporary or non-production with
  `TODO` / `FIXME`.

## Related

**Commands**: `/pr` (full, validated PR), `/update-pr`, `/pr-wrap` (the validation workflow this
command skips), `/add-to-pr-stack` (new stacked node **and** its draft PR), `/follow-up-branch`,
`/update-context-docs`
**Product docs**: the `pr-stack` skill (`.agents/skills/pr-stack/SKILL.md`)
