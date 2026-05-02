# Wrap Context Documentation

Transfer knowledge from changesets and PRDs into permanent documentation, then clean up working documents.

## Core Principle

**"Wrapping"** means:
1. Extract the final state (State B) from the working document
2. Update the actual permanent docs with that knowledge
3. Add a changelog or changeset **index** entry (audit trail)—see merge hygiene below
4. Delete the working document

Wrapping is **NOT** just adding changelog entries. It is a full knowledge transfer.

## Changelog / changeset index format (merge hygiene)

Follow [changelog-merge-hygiene.md](../../docs/dev/guides/changelog-merge-hygiene.md):

- **Product changelog** (`docs/ft/{coder,daemon,web}/changelog.md`): prepend a **new** top `## YYYY-MM-DD — Distinct title` section; single-line bullets; do not edit older `##` sections for unrelated work.
- **Package index** (`packages/*/docs/changesets.md`) and **cross-package index** (`docs/dev/changesets.md`): prepend **one new single-line bullet** under the intro block.
- **Optional** long cross-package narrative: add `docs/dev/changesets.d/YYYY-MM-DD-slug.md` and one index line linking it—see [changesets.d/README.md](../../docs/dev/changesets.d/README.md).

Repo `.gitattributes` uses **union** merge on these paths; **same-line** edits on two branches still conflict.

## Decision Logic

Before wrapping, check if all scope items are complete:

**If all checkboxes are `[x]`** -> Proceed with wrapping.

**If any checkboxes are NOT `[x]`** -> Display the CRITICAL DISCLAIMER:

```
+------------------------------------------------------------------------+
|                                                                        |
|   !! WRAPPING BLOCKED - INCOMPLETE ITEMS DETECTED !!                   |
|                                                                        |
|   The following items are not marked complete:                          |
|                                                                        |
|   - [ ] Item 1 description                                             |
|   - [~] Item 2 description                                             |
|                                                                        |
|   Wrapping incomplete work will permanently lose tracking of           |
|   unfinished items.                                                    |
|                                                                        |
+------------------------------------------------------------------------+
```

Then present three options to the user:

1. **Complete All Work** - Finish the remaining items before wrapping
2. **Accept Current State** - Wrap anyway, documenting incomplete items as known limitations
3. **Keep Working** - Abort the wrap and continue development

Do not proceed without the user choosing an option.

## Wrapping Changesets

For changesets in `docs/dev/1-WIP/`:

1. **Extract State B** from the changeset document
2. **Apply to dev docs**: Update `packages/*/docs/` with the final state descriptions
3. **Update change history**:
   - Add **one** release-note-style bullet line to each affected `packages/{package}/docs/changesets.md` (prepend, single line).
   - If the work is cross-package, add **one** bullet line to `docs/dev/changesets.md` (and optionally create `docs/dev/changesets.d/YYYY-MM-DD-slug.md` for a long narrative—see merge hygiene above).
4. **Delete** the changeset file from `docs/dev/1-WIP/` (not archived)

## Wrapping PRDs

For PRDs in `docs/ft/*/1-WIP/`:

1. **Extract State B** - the final feature specification
2. **Apply to feature docs**: Update `docs/ft/{area}/` with the completed feature documentation
3. **Add changelog entry** to `docs/ft/{area}/changelog.md`: **new** top `## YYYY-MM-DD — Title` section with single-line bullets (see merge hygiene above). Do not reference deleted PRD filenames.
4. **Delete** the PRD file from `docs/ft/*/1-WIP/` (not archived)

## Process

1. Identify documents to wrap (check `docs/dev/1-WIP/` and `docs/ft/*/1-WIP/`)
2. For each document, check completion status
3. Apply decision logic (complete vs incomplete)
4. Execute the wrap: extract -> update docs -> prepend index/changelog lines -> delete source
5. Report what was wrapped and where knowledge was transferred
