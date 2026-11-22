# PR-stack documents — per-PR PRD and changeset, attached to the child session

A planned PR can carry **two authored documents** on the session that owns its stack, and a child
session started for that PR can receive them as attachments:

| Document | Answers |
|---|---|
| `PRD.md` | What this PR delivers — behaviour, API surface, acceptance criteria |
| `changeset.md` | How, and **where the edges are** — responsibility, boundaries, what each dependency delivers, and the draft-PR contract |

The shared stack-level document is `pr-stack-plan.md` (`tddy_pr_stack::docs::PR_STACK_PLAN_MD_BASENAME`),
beside them under the orchestrator session's `artifacts/`.

**What is shipped and what is not.** The layout, the listing, the attachment builder
(`tddy_session_files::stack_doc_attachments::stack_doc_attachments`) and the daemon's prompt rule are
shipped. No workflow goal authors the documents, and neither the stack view's Start-session dialog nor
`pr_spawn_child` attaches them on its own: a child receives them when its start names them as
attachments. Wiring an attaching spawn path is a backlog item.

## Why boundaries belong in a per-PR document

The [PR boundary contract](pr-stacking.md#pr-boundary-contract-every-node-is-self-contained) tells
whoever plans a stack to cut vertical slices. It shapes *planning*. Nothing carries it into
*building* unless a document does: the child agent implementing `n3` has never seen the contract, does not know that `n2` is already adding
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
(`tddy-github/src/pr_api.rs:97`) has no `draft` parameter and gains none here. Draft PRs are
already *read* correctly — `pr_state_from_github` maps them to `PrState::Draft` (`pr_api.rs:68`)
and `pr_status.phase` deliberately records a draft as `open`
(`tddy-pr-stack/src/stack_ops/adopt.rs:180`). Opening a PR as a draft stays a human act; the
document says what should be in it.

## On-disk layout

Documents live on the **orchestrator session**, under its artifacts root:

```
{orchestrator_session_dir}/artifacts/
  stack-plan.yaml
  pr-stack-plan.md        <- the shared stack-level document
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
(`tddy-pr-stack/src/stack_ops/nodes.rs:20-25`). A helper resolves the pair of paths and reports
which exist; if a node ever needs to point somewhere else, the field is additive then.

## Required headings in `changeset.md`

Every per-PR changeset carries four headings (`tddy_pr_stack::docs`):

```
## Responsibility        what this PR owns
## Boundaries            what it explicitly does not do
## Dependencies          per parent node: what that PR delivers that this one consumes
## Draft PR contract     what lands first (API + failing tests) to unblock dependents
```

They are a content convention: the [PR boundary
contract](pr-stacking.md#pr-boundary-contract-every-node-is-self-contained) is not validated, because
distinguishing a vertical slice from a layer split needs the diff the node has yet to produce.

## Attaching documents to the child session

`stack_doc_attachments(orchestrator_session_dir, orchestrator_session_id, daemon_instance_id, node_id)`
builds the four attachments a child of `node_id` is started with, materialized into its
`artifacts/attachments/` by the `materialize_session_attachments` path:

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

**A missing document is skipped, not fatal.** A node started before its documents exist attaches
whatever does. Starting early is sometimes correct, and failing the spawn would make the documents a
hard prerequisite for all work.

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

Every seam that launches an agent after materializing attachments applies it: the claude-cli and
cursor-cli branches of `start_session_core`, and `spawn_split_agent`. The split seam is **not
covered by a test**, since exercising it needs a peer daemon.

Workspace and tool sessions ignore `initial_prompt` entirely and are untouched.

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

## Out of scope

- **Creating GitHub draft PRs.** The draft-PR contract is document content; `GithubPrApi` is unchanged.
- **A wire RPC to read a context document's contents.** `read_session_context_doc_utf8` still has no
  RPC and no Docs tab consumes it. This feature improves what the *child agent* reads off disk.
- **Attaching to an already-running session.** Attachments are supplied only at `StartSession`;
  there is no attach-after-start RPC. A child spawned before this ships does not retroactively
  receive documents.
- **Authoring the documents.** No goal or tool writes `prs/<node_id>/`; a `pr_write_node_docs` tool is a
  natural follow-up.

## Related

- [PR stacking](pr-stacking.md) — the stack data model, the PR-management tools, the PR boundary contract
- [Session attachments](session-attachments.md) — the `artifacts/attachments/` store and its guards
- [Exploration artifact](exploration-artifact.md) — the code-discovery map, one of the attached documents
- [Session layout](session-layout.md) — session directory structure and artifact paths
