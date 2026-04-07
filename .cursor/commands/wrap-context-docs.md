---
description: Updates and wraps documentation including changesets and PRDs. Tracks progress, updates checkboxes, and transfers knowledge from changesets/PRDs into documentation. Use after implementation work is done.
---

## Wrap Documentation

This command updates and wraps documentation by transferring knowledge from changesets and PRDs into the actual documentation, tracking progress and maintaining up-to-date project documentation.

## Core Principle: Transfer Knowledge, Not Just References

**CRITICAL**: "Wrapping" means:
1. ✅ **Extract final state** from changeset/PRD
2. ✅ **Update actual documentation** with new content (feature docs, dev docs)
3. ✅ **Add changelog entry** as audit trail

**NOT**:
- ❌ Only adding a changelog/changesets entry
- ❌ Leaving feature/dev docs unchanged
- ❌ Creating links to archived PRDs/changesets

The changeset/PRD knowledge must be **transferred into** the documentation before the source is deleted.

## When Invoked

1. **Find relevant documentation**:
   - Changesets: `docs/dev/1-WIP/`
   - PRDs: `docs/ft/*/1-WIP/`

2. **Update or wrap** based on status.

## Update Documentation (`/update-context-docs`)

### Scope Checkbox Updates

Track progress by updating checkboxes:

| Status | Meaning |
|--------|---------|
| `[ ]` | Not started |
| `[~]` | In progress |
| `[x]` | Complete |

### What to Update

- **Package Documentation**: `[x]` after planning complete
- **Implementation**: `[~]` during → `[x]` when complete
- **Testing**: `[x]` after acceptance tests pass
- **Integration**: `[x]` after verification
- **Technical Debt**: `[x]` after production readiness
- **Code Quality**: `[x]` after linting/review

### Milestone Tracking

Update milestone checkboxes as work completes:
```markdown
## Milestones
- [x] Phase 1: Setup
- [x] Phase 2: Implementation
- [ ] Phase 3: Testing  ← Update when tests pass
```

## Wrap Documentation (`/wrap-context-docs`)

### Prerequisites for Wrapping

**ALL must be true:**
- All Scope checkboxes marked `[x]`
- All acceptance criteria met
- Status changed to ✅ Complete

### Wrapping Process

**CRITICAL**: Follow the `/wrap-context-docs` command for detailed wrapping instructions. The key steps are:

1. **For Changesets**:
   - **Extract final state (State B)** from changeset (ignore deltas, process descriptions)
   - **Identify merge points** in dev documentation
   - **Apply content transformations** to package READMEs and dev docs
   - **Transform language**: "Changed to X" → "Uses X", "Now supports Y" → "Supports Y"
   - **Merge operations**: Replacement, addition, enhancement, or removal of sections
   - **Update change history**: Add release note-style entry to `packages/{package}/docs/changesets.md`
   - **Delete source**: Remove changeset from `docs/dev/1-WIP/` (not archived)

2. **For PRDs**:
   - **Extract final state (State B)** from PRD (ignore change rationale, deltas)
   - **Identify merge points** in feature documentation
   - **Apply content transformations** to feature documents
   - **Transform language**: Remove "changed", "updated", "new" language → state final state cleanly
   - **Merge operations**: Replacement, addition, enhancement, or removal of sections
   - **Update change history**: Add release note-style entry to `docs/ft/{product-area}/changelog.md`
   - **Delete source**: Remove PRD from `docs/ft/1-WIP/` (not archived)

**Changelog / changeset index format (merge hygiene)** — follow [changelog-merge-hygiene.md](../../docs/dev/guides/changelog-merge-hygiene.md): **prepend** one new **`##`** (product changelog) or **one new bullet line** (indexes); **single-line** bullets; **do not** rewrite existing shipped lines in the same PR as unrelated work; optional **`docs/dev/changesets.d/YYYY-MM-DD-slug.md`** for long cross-package notes plus one line in `docs/dev/changesets.md`.

**State B, Not Delta**: Documentation must read as cohesive, unified documents without traces of change process. No "previously", "now", "changed from", or temporal language.

### Decision Logic

```
IF all checkboxes [x] AND all criteria met:
    → Extract final state (State B) from changeset/PRD
    → Apply content transformations to target docs
    → Update changelog/changesets history files
    → Delete source document (not archived)
    → Report success
ELSE IF incomplete:
    → Report what's missing
    → Display HUGE DISCLAIMER (see below)
    → Skip wrapping
    → Keep for future work
ELSE:
    → No documents found
```

---

## ⚠️ CRITICAL: When Wrapping is Blocked

**IF prerequisites are not met, you MUST display this prominent disclaimer:**

```
╔════════════════════════════════════════════════════════════════════╗
║                                                                    ║
║  ⚠️  WRAPPING BLOCKED - ACTION REQUIRED  ⚠️                        ║
║                                                                    ║
║  The following documentation cannot be wrapped:                   ║
║  • [Document name]                                                ║
║                                                                    ║
║  STATUS: 🚧 In Progress (Required: ✅ Complete)                    ║
║                                                                    ║
║  INCOMPLETE ITEMS:                                                ║
║  ❌ [Item 1 name] - [Status: Not Started/In Progress]            ║
║  ❌ [Item 2 name] - [Status: Not Started/In Progress]            ║
║  ❌ [Item 3 name] - [Status: Not Started/In Progress]            ║
║                                                                    ║
║  IMPACT:                                                          ║
║  • Documentation remains in WIP state (not transferred)           ║
║  • Knowledge not transferred to permanent docs                    ║
║  • Changeset/PRD file stays in 1-WIP/ directory                  ║
║  • Technical debt accumulates                                     ║
║                                                                    ║
║  ACTION REQUIRED:                                                 ║
║  You have THREE OPTIONS:                                          ║
║                                                                    ║
║  Option 1: Complete All Work (Recommended)                        ║
║  → Finish all incomplete items                                    ║
║  → Update status to ✅ Complete                                   ║
║  → Call /wrap-context-docs again                                  ║
║  → Estimated effort: [X hours based on gaps]                      ║
║                                                                    ║
║  Option 2: Accept Current State & Wrap (If Justified)             ║
║  → Mark incomplete items as "Deferred" or "Out of Scope"         ║
║  → Document WHY items are being deferred                          ║
║  → Update status to ✅ Complete (Phase N)                         ║
║  → Call /wrap-context-docs again                                  ║
║  → Use when: Current work is cohesive and deployable             ║
║                                                                    ║
║  Option 3: Keep Working (Continue Implementation)                 ║
║  → Continue development on incomplete items                       ║
║  → Call /update-context-docs as you make progress                ║
║  → Wrap when all work is truly complete                          ║
║  → Use when: More work is definitely needed                      ║
║                                                                    ║
║  CHOOSE AN OPTION - Do not leave this unresolved!                ║
║                                                                    ║
╚════════════════════════════════════════════════════════════════════╝
```

**Format the disclaimer as**:
- Use box drawing characters for visual prominence
- List ALL incomplete items with their current status
- Show concrete impact of not wrapping
- Provide THREE clear action options with guidance
- Include effort estimates when possible
- Make it IMPOSSIBLE to miss

**Example of well-formatted disclaimer:**

```
╔════════════════════════════════════════════════════════════════════╗
║                                                                    ║
║  ⚠️  WRAPPING BLOCKED - ACTION REQUIRED  ⚠️                        ║
║                                                                    ║
║  Document: 2026-02-06-unified-grid-cell-state-implementation.md   ║
║                                                                    ║
║  STATUS: 🚧 In Progress (Required: ✅ Complete)                    ║
║                                                                    ║
║  INCOMPLETE ITEMS (4):                                            ║
║  ❌ Remove legacy PlaceholderState types - Not Started            ║
║  ❌ Remove old useIconsDuplexerEvents hook - Not Started          ║
║  ❌ Update all Cypress tests - Not Started                        ║
║  ❌ Update documentation - Not Started                            ║
║                                                                    ║
║  IMPACT:                                                          ║
║  • Hybrid architecture (old + new code coexisting)               ║
║  • Technical debt: 560-line legacy hook remains                   ║
║  • Documentation incomplete (migration plan not executed)         ║
║  • Future confusion for developers                                ║
║                                                                    ║
║  ACTION REQUIRED:                                                 ║
║                                                                    ║
║  Option 1: Complete Migration (~6-8 hours)                        ║
║  → Extract business logic from useIconsDuplexerEvents            ║
║  → Remove legacy hook and types                                   ║
║  → Update Cypress tests and documentation                         ║
║                                                                    ║
║  Option 2: Accept Hybrid Architecture                            ║
║  → Mark items as "Deferred - Out of Scope"                       ║
║  → Justify: "Separation of concerns is intentional"              ║
║  → Document hybrid architecture as final state                    ║
║  → Update status to ✅ Complete (Phase 1)                         ║
║                                                                    ║
║  Option 3: Continue Development                                   ║
║  → Work on migration tasks incrementally                          ║
║  → Call /update-context-docs as you progress                     ║
║  → Wrap when truly complete                                      ║
║                                                                    ║
║  CHOOSE NOW - This blocks documentation closure!                  ║
║                                                                    ║
╚════════════════════════════════════════════════════════════════════╝
```

---

### What "Apply to Target Docs" Means

**NOT just adding changelog entries!** You must:

1. **Read and understand** the changeset/PRD completely
2. **Extract final state** (State B) - what the docs SHOULD say after wrapping
3. **Find merge points** - which sections in target docs need updates
4. **Transform content** - convert changeset/PRD language to final-state language
5. **Merge intelligently** - replace, add, enhance, or remove sections
6. **Update the actual feature/dev docs** with the new content
7. **THEN** add changelog/changesets history entry as audit trail

See `/wrap-context-docs` command for detailed merge strategies and transformation rules.

## Output Format

### Update Report
```markdown
## Documentation Update Report

### Changesets Updated
| Document | Progress | Status |
|----------|----------|--------|
| 2026-01-22-feature.md | 4/6 complete | 🔄 In Progress |

### Updates Made
- [x] Implementation: Marked complete
- [x] Milestone 2: Marked complete
- [ ] Testing: Still pending

### Next Steps
- Complete acceptance tests
- Run production readiness check
```

### Wrap Report
```markdown
## Documentation Wrap Report

### Wrapped Documents
| Source | Target | Status |
|--------|--------|--------|
| 1-WIP/feature.md | packages/{package-name}/docs/ | ✅ Applied |
| 1-WIP/fix.md | ft/feature/1-OVERVIEW.md | ✅ Applied |

### Archived
- `docs/dev/1-WIP/archived/2026-01-22-feature.md`
- `docs/ft/feature/1-WIP/archived/fix.md`

### Skipped (Incomplete)
| Document | Missing |
|----------|---------|
| 1-WIP/other.md | Testing, Code Quality |

Reason: Complete remaining work before wrapping.
```

## Common Mistakes to Avoid

❌ **WRONG**: Only adding changelog entry without updating docs
❌ **WRONG**: Creating links to deleted PRD/changeset files
❌ **WRONG**: Keeping delta language ("changed from X to Y") in final docs
❌ **WRONG**: Leaving feature/dev docs unchanged

✅ **CORRECT**: Extract content from PRD/changeset → Update feature/dev docs → Add changelog entry → Delete source

## Reference

- **Commands**: `/update-context-docs`, `/wrap-context-docs`
- **Rules**: `@changeset-doc`, `@prd-doc`

---

## AI Implementation Guide: Constructing the Disclaimer

**When prerequisites are NOT met, you MUST**:

### Step 1: Collect Gap Information

```rust
interface GapInfo {
  documentName: string;
  currentStatus: string; // e.g., "🚧 In Progress"
  requiredStatus: string; // e.g., "✅ Complete"
  incompleteItems: Array<{
    name: string;
    status: 'Not Started' | 'In Progress';
    details?: string;
  }>;
  impactDescription: string[];
  estimatedEffort?: string;
}
```

### Step 2: Format the Disclaimer Box

**Use exact box drawing characters**:
- Top border: `╔════...════╗`
- Side borders: `║ ... ║`
- Bottom border: `╚════...════╝`
- Width: 72 characters (including borders)
- Internal padding: 2 spaces after `║` and before `║`

**Structure**:
```
╔════════════════════════════════════════════════════════════════════╗
║  [2 spaces padding]                                                ║
║  ⚠️  WRAPPING BLOCKED - ACTION REQUIRED  ⚠️  [centered with spaces]║
║  [2 spaces padding]                                                ║
║  Document: [name with .md extension]                              ║
║  [blank line]                                                      ║
║  STATUS: [current] (Required: [required])                         ║
║  [blank line]                                                      ║
║  INCOMPLETE ITEMS ([count]):                                      ║
║  ❌ [Item name] - [Status with details]                           ║
║  [... repeat for each item]                                       ║
║  [blank line]                                                      ║
║  IMPACT:                                                          ║
║  • [Impact point 1]                                               ║
║  • [Impact point 2]                                               ║
║  [... 3-5 impact points]                                          ║
║  [blank line]                                                      ║
║  ACTION REQUIRED:                                                 ║
║  [blank line - extra spacing for readability]                     ║
║  Option 1: [Action Name] ([effort estimate if available])        ║
║  → [Step 1]                                                       ║
║  → [Step 2]                                                       ║
║  → [Step 3]                                                       ║
║  [blank line]                                                      ║
║  Option 2: [Alternative Action]                                   ║
║  → [Step 1]                                                       ║
║  → [Step 2]                                                       ║
║  → [When to use this option]                                      ║
║  [blank line]                                                      ║
║  Option 3: [Fallback Action]                                      ║
║  → [Step 1]                                                       ║
║  → [Step 2]                                                       ║
║  → [When to use this option]                                      ║
║  [blank line]                                                      ║
║  CHOOSE NOW - [Urgency message]                                  ║
║  [2 spaces padding]                                                ║
╚════════════════════════════════════════════════════════════════════╝
```

### Step 3: Calculate Line Padding

Each content line inside the box MUST be exactly 70 characters (72 total including borders):
- Start: `║  ` (3 chars: border + 2 spaces)
- Content: variable length
- End: padding spaces + `║` (1 char border)

**Padding formula**: `68 - content.length` spaces before `║`

**Example**:
```
║  STATUS: 🚧 In Progress (Required: ✅ Complete)                    ║
   ^--2    ^--content (44 chars)                    ^--padding--^   ^
   spaces                                             (22 spaces)    border
```

### Step 4: Tailor Options to Context

**Option 1: Complete All Work**
- Use when: Items can be finished in reasonable time (<8 hours)
- Provide: Specific steps for each incomplete item
- Include: Effort estimate based on complexity

**Option 2: Accept Current State**
- Use when: Current work is cohesive and deployable
- Provide: How to mark items as deferred
- Include: Justification guidance (why deferring is acceptable)
- Warn: Only use if current state is architecturally sound

**Option 3: Keep Working**
- Use when: More work is definitely needed
- Provide: Guidance on iterative progress
- Include: When to call /update-context-docs

### Step 5: Make It Visible

**CRITICAL**: This disclaimer must be:
- ✅ The FIRST thing in your response (before any other text)
- ✅ Impossible to miss (box drawing + emojis)
- ✅ Actionable (clear options with concrete steps)
- ✅ Urgent (conveys that this blocks progress)

**DO NOT**:
- ❌ Bury in the middle of a response
- ❌ Make it a small note or aside
- ❌ Use subtle formatting
- ❌ Leave it ambiguous what to do

### Step 6: Example Usage Pattern

```rust
// In your response:
if (!prerequisitesMet) {
  // 1. Display HUGE DISCLAIMER first (prominent box)
  displayHugeDisclaimer(gapInfo);
  
  // 2. Then explain gaps in detail
  explainWhatIsMissing();
  
  // 3. Then provide specific guidance
  provideActionableSteps();
  
  // 4. End with clear question
  askUserToChooseOption();
} else {
  // Prerequisites met - proceed with wrapping
  performWrapping();
}
```

---

## Reference

- **Commands**: `/update-context-docs`, `/wrap-context-docs`
- **Rules**: `@changeset-doc`, `@prd-doc`
