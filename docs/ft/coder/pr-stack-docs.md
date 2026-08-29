# PR-stack documents — per-PR PRD and changeset, attached to the child session

A `pr-stack` orchestrator today plans a stack and hands each child session two strings: the node's
`title` and its `description` (`PrStackScreen.tsx:254`, `connection_service.rs:6913`). Everything the
orchestrator learned while planning — the code-discovery map in `exploration.md`, the shape of the
stack in `pr-stack-plan.md`, and above all *where this PR stops and the next one starts* — stays on
the orchestrator and never reaches the agent that has to build the thing.

This feature gives every planned PR **two authored documents** on the orchestrator, and **attaches
them to the child session** at spawn:

| Document | Answers |
|---|---|
| `PRD.md` | What this PR delivers — behaviour, API surface, acceptance criteria |
| `changeset.md` | How, and **where the edges are** — responsibility, boundaries, what each dependency delivers, and the draft-PR contract |

The shared stack-level document is the **existing** `pr-stack-plan.md`. It is generated from
`stack-plan.yaml` on every plan write and is not replaced or supplemented by a new epic document:
the plan *is* the stack-level doc, and a hand-authored twin of it could only drift.

## Why boundaries belong in a per-PR document

The [PR boundary contract](pr-stacking.md#pr-boundary-contract-every-node-is-self-contained) already
tells the planning agent to cut vertical slices. It is carried in the `analyze-stack` and
`write-stack-plan` system prompts and shapes *planning*. Nothing carries it into *building*: the
child agent implementing `n3` has never seen the contract, does not know that `n2` is already adding
the trait it is about to add, and has no way to find out. Two children building the same abstraction
in parallel is the failure this feature exists to prevent, and it is not a planning failure — the
plan can be perfect and the duplication still happens.

So `changeset.md` states, for each parent node, **what that PR delivers that this one consumes**.
The child reads it before writing code and knows which surfaces are somebody else's to create.

## Draft-PR contract

A stacked PR blocks its dependents for as long as it is unfinished. The usual mitigation is to
publish the *interface* early, and `changeset.md` carries a section naming exactly that: the API
surface plus its failing tests, enough to open a draft PR against, so dependents can branch off a
real ref and code against a real signature while the implementation continues in the same PR.

This is a **content contract, not automation**. `GithubPrApi::create_pr(head, base, title, body)`
(`orchestrate_pr_stack/github.rs:97`) has no `draft` parameter and gains none here. Draft PRs are
already *read* correctly — `pr_state_from_github` maps them to `PrState::Draft` (`github.rs:68`) and
`pr_status.phase` deliberately records a draft as `open` (`pr_stack/mod.rs:1397`). Opening a PR as a
draft stays a human act; the document says what should be in it.

## On-disk layout

Documents live on the **orchestrator session**, under its artifacts root:

```
{orchestrator_session_dir}/artifacts/
  stack-plan.yaml         <- moved here (see "Artifact path correction")
  pr-stack-plan.md        <- moved here; the shared stack-level document
  exploration.md
  stack-status.md
  stack-status.json
  prs/
    n1/
      PRD.md
      changeset.md
    n2/
      PRD.md
      changeset.md
```

**Nesting under `prs/<node_id>/` is already supported by the read path.** `resolve_host_document`
joins the full `relative_path` against the scope root (`host_documents.rs:211`) behind a
canonicalize-and-contain guard on the parent (`:223-229`); `validate_relative_path` (`:40`) rejects
`..`, `.` and absolute paths but permits separators. Only the final filename is segment-validated
(`:200-204`). No new read path is required — the narrower wording at `connection.proto:2119` ("the
basename of a `SessionContextDoc`") describes intent, not the implementation.

**Paths are derived from `node_id` by convention**, not recorded on the node. `StackNode` gains no
field, and neither do `PlannedPr`, `AddPlannedPrRequest`, `PrAddPlannedInput` or the web wire type.
The precedent against widening that surface speculatively is `AddPlannedPrInput.child_recipe`, which
is accepted and silently dropped because `StackNode` has nowhere to put it
(`pr_stack/mod.rs:600-610`). A helper resolves the pair of paths and reports which exist; if a node
ever needs to point somewhere else, the field is additive then.

## The `write-stack-docs` goal

The pipeline gains a fourth goal:

```
analyze-stack --GoTo--> write-stack-plan --GoTo--> write-stack-docs --GoTo--> orchestrate
```

Docs are authored in **their own goal**, not folded into the `write-stack-plan` submit. The plan is
cheap, re-runnable and refined constantly through chat; the documents are expensive and mostly
stable. Sharing one submit would mean every "actually, split `n2` in two" rewrote every document in
the stack, and would put a plan the host can validate structurally behind a payload it cannot.

- **`write-stack-docs`** — `PermissionHint::ReadOnly`, `goal_requires_tddy_tools_submit` returns
  `true` for it. Like `write-stack-plan`, its submit is **raw YAML parsed by the hook**, with no
  entry in `goals.json` and no generated JSON schema: `generated/` holds only `tdd/`, and the
  pr-stack goals have never had schemas.

### Submit payload

```yaml
version: 1
docs:
  - node_id: n1
    prd: |
      # n1 — Token store
      ...
    changeset: |
      # Changeset: n1 — Token store
      ## Responsibility
      ...
```

### Validation (`validate_stack_docs`)

A rejected submit writes **nothing**, matching every other stack writer.

| Rule | Why |
|---|---|
| Every `node_id` resolves to a node in the persisted stack | a document for a node that does not exist is a planning error surfacing late |
| Every node in the stack has an entry | a node with no boundaries document is precisely the duplicate-development hazard; a partial pass is refused rather than half-written |
| `prd` and `changeset` are non-blank | an empty file reads as "no boundaries" rather than "not written yet" |
| `changeset` carries all four required headings | see below |

The four required headings in `changeset.md` are checked **structurally** — the host asserts the
headings are present, not that their contents are correct:

```
## Responsibility        what this PR owns
## Boundaries            what it explicitly does not do
## Dependencies          per parent node: what that PR delivers that this one consumes
## Draft PR contract     what lands first (API + failing tests) to unblock dependents
```

This is a deliberate middle ground and the line is worth stating. The [PR boundary
contract](pr-stacking.md#pr-boundary-contract-every-node-is-self-contained) is **not** validated,
because distinguishing a vertical slice from a layer split needs the diff the node has yet to
produce, and any check reduces to a keyword heuristic. "Does this document have a `## Boundaries`
heading" needs no semantics at all. Presence is mechanically checkable and worth enforcing; content
stays prompt-carried and human-reviewed.

### State table

| State | Goal |
|---|---|
| `Init` \| `AnalyzeStack` | `analyze-stack` |
| `WriteStackPlan` | `write-stack-plan` |
| `StackPlanned` | `write-stack-docs` *(was `orchestrate`)* |
| `StackDocsWritten` *(new)* | `orchestrate` |
| `failed` | `None` |

`status_for_session`: `StackPlanned | StackDocsWritten | orchestrate → "Active"`.

**A seeded stack starts at `StackDocsWritten`, not `StackPlanned`.** Seeding
(`seed_stack_with_base_session`) binds one node to a session whose work already exists and may
already be underway; a retroactive PRD for it would document a decision nobody is about to make.
`tddy-coder`'s seeding path therefore writes `STATE_STACK_DOCS_WRITTEN` where it previously wrote
`STATE_STACK_PLANNED`, preserving the documented behaviour that a seeded orchestrator comes up
straight in `orchestrate` ([pr-stacking.md § Seeding the
stack](pr-stacking.md#seeding-the-stack-from-an-existing-session-added-2026-08-13)). The recipes
crate owns both constants; the coder writes them.

**A plan refinement re-runs the docs pass.** `after_write_stack_plan` re-seeds `Changeset.stack` on
every refinement turn and then sets state to `StackPlanned` — which now routes to
`write-stack-docs`, so the operator's next turn regenerates the documents before returning to
`orchestrate`. This interrupts the chat deliberately: a refined plan carrying stale boundary
documents is exactly the hazard the feature exists to prevent, and documents that silently describe
a previous plan are worse than none. `write-stack-docs` is idempotent and rewrites every node.

## Attaching documents to the child session

At spawn a child receives four attachments, materialized into its
`artifacts/attachments/` by the existing `materialize_session_attachments` path:

| Source on the orchestrator | Destination basename |
|---|---|
| `prs/<node_id>/PRD.md` | `PRD.md` |
| `prs/<node_id>/changeset.md` | `changeset.md` |
| `pr-stack-plan.md` | `pr-stack-plan.md` |
| `exploration.md` | `exploration.md` |

Each is a `SessionAttachment` carrying a `HostDocumentRef { daemon_instance_id, scope:
SESSION_ARTIFACT, session_id: <orchestrator>, relative_path }`. The source path is nested; the
destination is a flat basename — separate fields, so the flat one-level attachment store
([session-attachments.md](session-attachments.md)) is respected without flattening the source.

**Cross-host works unchanged.** `daemon_instance_id` on the ref names the orchestrator's host, and
`materialize_host_document_attachment` already performs a streaming fetch when that is not the local
daemon. Documents are capped by `MAX_HOST_DOCUMENT_BYTES` (4 MiB, `host_documents.rs:30`).

**A child's own `artifacts/PRD.md` is untouched.** The `tdd` recipe's manifest owns
`artifacts/PRD.md`; the attached copy lands at `artifacts/attachments/PRD.md`. These are distinct
files by design — "a recipe artifact and an attachment may share a basename"
([session-attachments.md](session-attachments.md)) — and attachment writes cannot escape the
`attachments/` subdirectory.

**A missing document is skipped, not fatal.** A node started before its docs pass ran attaches
whatever exists. Starting early is sometimes correct, and failing the spawn would make the docs pass
a hard prerequisite for all work. The operator is told which documents were unavailable.

### Both spawn paths attach

A child must not differ by how it was started, so both paths build the same attachment list from one
helper:

- **Agent** — `StackChildSpawnHandler::spawn_child` (`connection_service.rs:6893`), reached by the
  `pr_spawn_child` tool. It already reads the orchestrator's changeset and session metadata, so it
  has everything the helper needs.
- **Web** — the Start-session dialog opened from a planned-PR row (`PrStackScreen.tsx:233-256`).

### The child is told the documents are there

`initial_prompt` gains a line naming the attached changeset by path, so the agent reads its
boundaries before writing code rather than discovering the file by chance. This mirrors the grill-me
hand-off, which names `{session_dir}/artifacts/grill-me-brief.md` in the spawned conversation's
prompt (`grill_me/prompt.rs:99-107`).

**The rule lives in the daemon, once, and every launching seam applies it** —
`prompt_with_attached_changeset(initial_prompt, materialized)`. It is deliberately *not* duplicated
in the web client: "a child must not differ by how it was started" is not a rule a client can be
trusted to remember, and the daemon is the only place that knows what actually landed.

It is derived from the **materialized** attachments, not the offered ones, so a line can never name a
document that is not there. And it matches on the attachment's **source**, not its destination
basename: a `SESSION_ARTIFACT` ref whose `relative_path` is exactly `prs/<node_id>/changeset.md`. An
operator who attaches a file of their own called `changeset.md` gets no line — matching on the
destination would have fired for them the moment the dialog path was wired.

Four seams launch an agent after materializing attachments, and all four apply it: the agent path
(`StackChildSpawnHandler::spawn_child`), the claude-cli and cursor-cli branches of
`start_session_core` (the dialog path), and `spawn_split_agent`. The last is reachable because the
Start-session dialog offers a codebase host (`create-session-codebase-host-select`), so a
split-placement pr-stack child would otherwise have kept exactly the inconsistency this closes — but
it is **not covered by a test**, since exercising it needs a peer daemon. The other three are, by
tests verified through mutation: removing the call, or the attachments, fails them.

Workspace and tool sessions ignore `initial_prompt` entirely and are untouched.

### Auto-attachment in the Start-session dialog

The dialog shows the four documents as **pre-populated attachment rows, which the operator can
remove**. `CreateSessionInitialValues` (`CreateSessionPane.tsx:67-116`) gains an `attachments` entry
and `useSessionAttachments` accepts initial rows; removal already exists (`removeAttachment`).

Pre-populated-and-removable rather than fixed, because the rows are a *default*, not an invariant:
an operator restarting an orphaned node whose child already holds the documents should not be forced
to re-attach them. Rendering them as ordinary rows also means the existing list, rename and
drop-zone behaviour applies unchanged.

## Listing per-PR documents

`context_docs_for_session` builds `SessionEntry.context_docs` from two sources: the recipe manifest's
**static** basenames, and a **dynamic** scan of `artifacts/attachments/`. Per-PR documents are a
third, dynamic source — `artifacts/prs/*/` — listed with `kind = MANIFEST` (they are recipe-owned,
not user-attached).

`SessionArtifactManifest::known_artifacts()` returns `&[(&'static str, &'static str)]` and therefore
cannot enumerate N per-node files. It is **not** changed: the per-PR rows are discovered by scanning,
exactly as attachment rows already are, and the static manifest keeps describing the stack-level
artifacts alone.

`SessionContextDoc` gains **`relative_path` (field 8)**, populated for every row. `HostDocumentPicker`
currently reconstructs the path from the kind:

```ts
relativePath:
  doc.kind === SessionContextDocKind.ATTACHMENT
    ? `attachments/${doc.basename}`
    : doc.basename,
```

That derivation cannot express `prs/n1/PRD.md`, and it duplicates knowledge the server already has.
The picker reads `relative_path` instead. The field is additive and the derivation stays as the
fallback for a server that does not send it.

## Artifact path correction

`stack-plan.yaml` and `pr-stack-plan.md` are written to the **session root**
(`pr_stack/hooks.rs:333,337`), while `exploration.md` and the two `stack-status.*` files go under
`artifacts/`. Every reader but one resolves manifest basenames under `artifacts/` only —
`context_docs_for_session` (`session_context_docs.rs:76`), `read_session_context_doc_utf8` (`:167`)
and `resolve_host_document`'s `SESSION_ARTIFACT` root (`host_documents.rs:105`) — so both files
report `exists: false` in `context_docs` today and cannot be referenced as host documents at all.
The exception is `build_context_header` (`tddy-core/src/workflow/mod.rs:262-276`), which probes
`artifacts/<name>` and falls back to the session root as `"legacy session root"` — which is why the
orchestrator agent's own context header finds them and nothing else does.

Both move under `artifacts/`. This is a prerequisite: attaching `pr-stack-plan.md` to a child
requires a `SESSION_ARTIFACT` ref that resolves. `build_context_header`'s existing fallback means
orchestrators created before this change keep working with no migration.

## Out of scope

- **Creating GitHub draft PRs.** The draft-PR contract is document content; `GithubPrApi` is unchanged.
- **A wire RPC to read a context document's contents.** `read_session_context_doc_utf8` still has no
  RPC and no Docs tab consumes it. This feature improves what the *child agent* reads off disk.
- **Attaching to an already-running session.** Attachments are supplied only at `StartSession`;
  there is no attach-after-start RPC. A child spawned before this ships does not retroactively
  receive documents.
- **Per-node document regeneration on demand.** `write-stack-docs` rewrites the whole stack. A
  `pr_write_node_docs` tool for one node is a natural follow-up.

## Related

- [PR stacking](pr-stacking.md) — the recipe, the stack data model, the PR-management tools, the PR boundary contract
- [Session attachments](session-attachments.md) — the `artifacts/attachments/` store and its guards
- [Exploration artifact](exploration-artifact.md) — the code-discovery map, one of the attached documents
- [Session layout](session-layout.md) — session directory structure and artifact paths
