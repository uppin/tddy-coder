# Update Context Documentation

**Purpose**: Automatically update feature and development documentation based on current implementation state, test results, and codebase analysis.

**When to use**: After completing major development milestones, before code reviews, or when documentation needs to reflect current reality.

---

## Command Workflow

### 1. Discovery Phase

**Scan for relevant documentation:**
- **CRITICAL**: Check for PRD documents FIRST in `docs/ft/{product-area}/1-WIP/`
  - If PRD document exists: ONLY modify the PRD document
  - If NO PRD document exists: Modify the original feature document
  - **NEVER modify original feature document if PRD exists**
- Find all dev docs in `packages/*/docs/` that reference current work
- Identify feature-dev doc relationships
- Check product area overview: `docs/ft/{product-area}/1-OVERVIEW.md`

**Priority Order for Feature Documentation:**
1. **PRD document** (if exists): `docs/ft/{product-area}/1-WIP/PRD-YYYY-MM-DD-feature-name.md`
2. **Original feature document** (only if no PRD): `docs/ft/{product-area}/feature-name.md`

**Analyze current implementation:**
- Search codebase for completed features (implemented files, modules, structs, impls)
- Identify passing/failing tests (test files, `cargo test` results)
- Analyze git commits for completed work
- Review E2E/integration test results

### 2. Feature Documentation Update

**CRITICAL DECISION POINT:**

**IF PRD document exists:**
- ✅ **Update ONLY the PRD document**
- ❌ **DO NOT modify the original feature document**
- PRD document path: `docs/ft/{product-area}/1-WIP/PRD-YYYY-MM-DD-feature-name.md`
- Update "Affected Features" section to reflect which features are impacted
- Update acceptance criteria within the PRD document

**IF NO PRD document exists:**
- ✅ **Update the original feature document**
- Feature document path: `docs/ft/{product-area}/feature-name.md`
- Update acceptance criteria checkboxes normally

**Update acceptance criteria checkboxes:**
- **Search pattern**: Look for `- [ ]` (unchecked) or `- [x]` (checked) patterns
- **Verify implementation**: Check if feature exists in codebase
- **Update status**: Convert `- [ ]` → `- [x]` for implemented features
- **Add completion markers**: Add "✅ Implemented" or "❌ Not Started" status

**Update sections (in PRD OR original feature doc):**
- **Success Criteria**: Mark functional/performance/UX requirements as complete
- **Implementation Phases**: Update phase status (Planning/In Progress/Complete)
- **User Stories**: Mark acceptance criteria as met based on test results
- **Affected Features** (PRD only): List all feature documents being modified

**Preservation Rule:**
- Original feature documents = historical record (DO NOT MODIFY if PRD exists)
- PRD documents = current changes (ALWAYS MODIFY when they exist)

**Example transformation:**
```markdown
<!-- BEFORE -->
### Functional Requirements
- [ ] Text Markup Undo: Highlight, underline, strikethrough undoable via Ctrl+Z
- [ ] Form Field Undo: Form field changes undoable via Ctrl+Z
- [ ] Unified Stack: Document operations integrated with undo stack

<!-- AFTER -->
### Functional Requirements
- [x] Text Markup Undo: Highlight, underline, strikethrough undoable via Ctrl+Z ✅
- [x] Form Field Undo: Form field changes undoable via Ctrl+Z ✅
- [x] Unified Stack: Document operations integrated with undo stack ✅
```

### 3. Development Documentation Update (`docs/dev/1-WIP/*.md`, `packages/*/docs/*.md`)

**CRITICAL: Changesets are living documents - always update them during development.**

**Update Scope checkboxes (in changeset documents):**
- **Find Scope section**: Look for `## Scope` with high-level deliverables
- **Update progress indicators**:
  - `[ ]` → Not started
  - `[~]` → In progress (use when work has begun but not complete)
  - `[x]` → Complete ✅
- **Mapping to workflow stages**:
  - **Package Documentation**: Check when planning complete (`@plan-ft-dev`)
    - Updates to `[x]` after changeset created with affected packages listed
  - **Implementation**: Progress from `[~]` to `[x]` as code changes land
    - Updates to `[~]` when first code changes committed
    - Updates to `[x]` when all implementation milestones complete
  - **Testing**: Check when acceptance tests pass (`@test-acceptance`)
    - Updates to `[~]` when acceptance tests created (`@ft-dev`)
    - Updates to `[x]` when all acceptance tests passing
  - **Integration**: Check when cross-package tests pass
    - Updates to `[x]` when integration tests passing
  - **Technical Debt**: Check when production readiness verified (`@prod-ready`)
    - Updates to `[x]` when production readiness checklist complete
  - **Code Quality**: Check when linting/validation passes (`/validate-tests`, `/analyze-clean-code`)
    - Updates to `[x]` when linting passes and code review complete (`@pr-wrap`)

**Scope Update Responsibility Matrix**:

| Scope Item | Rule/Command | When to Update | Status Change |
|-----------|-------------|----------------|---------------|
| Package Documentation | `@plan-ft-dev` | After changeset creation | `[ ]` → `[x]` |
| Implementation | `/update-context-docs` | First commit with code changes | `[ ]` → `[~]` |
| Implementation | `/update-context-docs` | All milestones complete | `[~]` → `[x]` |
| Testing | `@ft-dev` | Acceptance tests created | `[ ]` → `[~]` |
| Testing | `@test-acceptance` | All acceptance tests pass | `[~]` → `[x]` |
| Integration | `@test-acceptance` | Integration tests pass | `[ ]` → `[x]` |
| Technical Debt | `@prod-ready` | Production readiness verified | `[ ]` → `[x]` |
| Code Quality | `/analyze-clean-code` | Clean code analysis passes | `[ ]` → `[~]` |
| Code Quality | `/validate-tests` | All validations pass | `[~]` → `[x]` |

**Example Scope transformation:**
```markdown
<!-- BEFORE (during @plan-ft-dev) -->
## Scope
- [ ] **Package Documentation**: Update package READMEs and dev docs
- [ ] **Implementation**: Complete code changes across affected packages
- [ ] **Testing**: All acceptance tests passing
- [ ] **Integration**: Cross-package integration verified
- [ ] **Technical Debt**: Production readiness gaps addressed
- [ ] **Code Quality**: Linting, type checking, and code review complete

<!-- DURING DEVELOPMENT (after some implementation) -->
## Scope
- [x] **Package Documentation**: Update package READMEs and dev docs ✅
- [~] **Implementation**: Complete code changes across affected packages
- [ ] **Testing**: All acceptance tests passing
- [ ] **Integration**: Cross-package integration verified
- [ ] **Technical Debt**: Production readiness gaps addressed
- [ ] **Code Quality**: Linting, type checking, and code review complete

<!-- AFTER (ready for /wrap-context-docs) -->
## Scope
- [x] **Package Documentation**: Update package READMEs and dev docs ✅
- [x] **Implementation**: Complete code changes across affected packages ✅
- [x] **Testing**: All acceptance tests passing ✅
- [x] **Integration**: Cross-package integration verified ✅
- [x] **Technical Debt**: Production readiness gaps addressed ✅
- [x] **Code Quality**: Linting, type checking, and code review complete ✅
```

**Update implementation milestones:**
- **Find all milestone sections** (e.g., `### Milestone 1: Command Interface & Types`)
- **Check deliverables** against codebase (file exists, exports correct types)
- **Mark completed**: Convert `- [ ]` → `- [x]` for completed deliverables
- **Update acceptance criteria**: Mark as complete based on test results

**Add/Update status sections:**
- **Implementation Status**: Current progress percentage, completed/pending tasks
- **Test Results**: E2E/integration/unit test pass/fail counts
- **Known Issues**: Extract from linter errors, TODO comments, failing tests
- **Technical Debt**: Identify code smells, console.log statements, abandoned files

**Update TODO lists:**
- Mark completed tasks as `[x]`
- Add new tasks discovered during analysis
- Re-prioritize based on current needs

**Example transformation:**
```markdown
<!-- BEFORE -->
### Milestone 2: Yjs Document Setup
**Deliverables**:
- [ ] Create `yjs-document.rs`
- [ ] Implement `YjsDocument` class
- [ ] Set up Y.Doc with metadata and commands
- [ ] Configure Y.UndoManager

**Acceptance Criteria**:
- [ ] Y.Doc initializes correctly
- [ ] Commands can be added to Y.Array

<!-- AFTER -->
### Milestone 2: Yjs Document Setup ✅
**Status**: Complete (2025-11-06)

**Deliverables**:
- [x] Create `yjs-document.rs` ✅
- [x] Implement `YjsDocument` class ✅
- [x] Set up Y.Doc with metadata and commands ✅
- [x] Configure Y.UndoManager ✅

**Acceptance Criteria**:
- [x] Y.Doc initializes correctly ✅
- [x] Commands can be added to Y.Array ✅

**Evidence**:
- File: `packages/client-lib/src/state/yjs-document.rs` (200 lines)
- Tests passing: `yjs-integration.rs` (8/8)
```

### 4. Add Implementation Evidence

**For feature docs:**
- Add links to key implementation files
- Reference E2E/integration tests that prove functionality

**For dev docs:**
- Add file paths for implemented deliverables
- Add test file references with pass/fail status
- Add git commit SHAs for completed work
- Add known issues with tracking IDs

### 5. Update Technical Debt Sections

**Scan codebase for:**
- `// TODO:` comments → add to technical debt list
- `// FIXME:` comments → add to known issues
- `#[allow(...)]` suppressions → identify code quality issues
- Abandoned files (not used in any module) → mark for deletion

**Example technical debt entry:**
```markdown
#### Issue #3: Excessive Console Logging
**Location**: `packages/client-lib/src/state/yjs-document.rs`
**Issue**: 25+ console statements in production code
**Risk**: Performance impact, console pollution
**TODO**: Replace with `tracing` or crate logging framework
**Priority**: High
```

### 6. Update Test Results

**Extract from test runs:**
- Total tests: X passed, Y failed
- Test files: List with pass/fail counts
- Test coverage: Extract from coverage reports
- Performance metrics: Test execution times

**Add to dev docs:**
```markdown
### 🧪 Test Results
**Last Run**: 2025-11-06
**Status**: ✅ All Passing (10/10 tests)

**E2E Tests**:
- `undo-redo.rs`: 2/2 passed ✅
- `yjs-integration.rs`: 8/8 passed ✅

**Test Quality**:
- ✅ No mocks in E2E tests
- ✅ Computer vision validation
- ✅ Real Yjs, real parser
```

---

## Usage

### Interactive Invocation
```bash
/update-context-docs
```

The command will:
1. Ask which feature/dev doc to update (or detect from current work)
2. Analyze codebase for implementation evidence
3. Show proposed changes as diff
4. Ask for confirmation before updating
5. Update documentation files
6. Report completion summary

### Automated Invocation (in workflows)
```bash
/update-context-docs --auto --doc=editor-undo-redo-integration
```

---

## Detection Heuristics

### How to find relevant docs:

**From current branch:**
```bash
# Extract feature from branch name
# e.g., feat/undo-redo-yjs-clean → "undo-redo"
FEATURE=$(git branch --show-current | grep -oE 'feat/([^/]+)' | cut -d'/' -f2)

# CRITICAL: Check for PRD docs FIRST
find docs/ft/*/1-WIP/ -iname "*${FEATURE}*.md"

# If no PRDs found, then check original feature docs
if [ $? -ne 0 ]; then
  find docs/ft/ -type f -iname "*${FEATURE}*.md" ! -path "*/1-WIP/*"
fi

# Find matching dev docs
find packages/*/docs/ -iname "*${FEATURE}*.md"
```

**From recent commits:**
```bash
# Get files changed in last 10 commits
git log -10 --name-only --pretty=format:"" | grep -E "^(docs/ft|docs/dev)" | sort -u

# Prioritize PRDs over feature docs
PRDS=$(git log -10 --name-only --pretty=format:"" | grep -E "^docs/ft/.*/1-WIP/")
FEATURES=$(git log -10 --name-only --pretty=format:"" | grep -E "^docs/ft/.*\.md$" | grep -v "/1-WIP/")

# Use PRDs if they exist, otherwise use feature docs
[ -n "$PRDS" ] && echo "$PRDS" || echo "$FEATURES"
```

**From current file context:**
- If user is viewing a PRD doc, update that PRD
- If user is viewing a feature doc, CHECK if PRD exists first
- If PRD exists: update PRD, NOT the feature doc
- If user is viewing implementation file, find related docs by keyword matching

### How to verify implementation:

**For deliverables:**
```bash
# Check if file exists
test -f packages/client-lib/src/state/yjs-document.rs && echo "✅" || echo "❌"

# Check if struct/impl exists (Rust)
grep -qE "pub struct YjsDocument|impl YjsDocument" packages/client-lib/src/state/yjs-document.rs && echo "✅" || echo "❌"
```

**For acceptance criteria:**
```bash
# Check if tests exist and pass (from repo root)
cargo test -p client-lib undo_redo 2>&1 | grep -q "passed" && echo "✅" || echo "❌"
```

---

## Output Format

### Summary Report (Console)
```
📝 Documentation Update Summary
═══════════════════════════════

📄 PRD Doc: docs/ft/editor-app/1-WIP/PRD-2025-01-05-undo-redo-integration.md
   ✅ Updated 15 acceptance criteria checkboxes
   ✅ Marked 3 implementation phases as complete
   ✅ Added evidence links to 8 implementation files
   ✅ Updated "Affected Features" section (3 features impacted)
   ℹ️  Original feature doc preserved (not modified)

📄 Dev Doc: docs/dev/1-WIP/yjs-undo-redo-phase1-command-logging.md
   ✅ Updated 4 milestone statuses
   ✅ Marked 32 deliverables as complete
   ✅ Added test results section (10/10 tests passing)
   ✅ Added 6 known issues to technical debt
   ✅ Updated TODO list (18 completed, 4 remaining)

📊 Implementation Evidence:
   - Files created: 8
   - Tests passing: 10/10
   - Storybook stories: 2
   - E2E coverage: 100% of user stories

⚠️  Warnings:
   - 3 TODO comments need tracking numbers
   - 1 abandoned file should be deleted
   - 25 console.log statements in production code

✅ Documentation is now up-to-date with codebase reality!
```

---

## Implementation Checklist

When running this command, verify:

- [ ] **CRITICAL**: Checked for PRD documents FIRST
- [ ] **CRITICAL**: If PRD exists, ONLY modified PRD (not original feature doc)
- [ ] **CRITICAL**: If no PRD, modified original feature doc
- [ ] **CRITICAL**: Updated changeset Scope checkboxes based on current workflow stage
- [ ] PRD "Affected Features" section is up-to-date (if applicable)
- [ ] Changeset "Scope" section reflects current progress ([ ], [~], or [x])
- [ ] Feature/PRD doc acceptance criteria match test results
- [ ] Dev doc milestones reflect actual file existence
- [ ] All "✅ Implemented" claims have file evidence
- [ ] Test result counts match actual test runs
- [ ] Known issues have tracking IDs
- [ ] Technical debt items have priority levels
- [ ] Git commit references are valid
- [ ] Storybook story URLs work
- [ ] File paths are accurate
- [ ] TODO comments are tracked

---

## Error Handling

**If feature/dev doc not found:**
- Check PRD directory first: `docs/ft/*/1-WIP/`
- If no PRD, check original feature docs: `docs/ft/*/*.md`
- Prompt user to specify manually
- Search by keywords in doc titles
- List all available docs for selection

**If both PRD and feature doc exist:**
- ✅ **Update PRD document ONLY**
- ❌ **DO NOT update original feature document**
- Inform user: "PRD document found - original feature doc preserved"

**If implementation files not found:**
- Mark deliverable as incomplete
- Add warning to report
- Suggest checking file paths in dev doc

**If tests fail to run:**
- Capture error output
- Add to known issues section
- Mark acceptance criteria as incomplete

**If git commands fail:**
- Skip commit SHA references
- Use file modification timestamps instead

---

## Related Rules

- **@feature-doc.mdc**: Feature documentation structure and requirements
- **@prd-doc.mdc**: PRD document structure and workflow
- **@changeset-doc.mdc**: Changeset document structure and requirements
- **@plan-ft.mdc**: Feature planning and PRD creation workflow
- **@plan-ft-dev.mdc**: Changeset document creation workflow
- **@dev-doc.mdc**: Development documentation structure and requirements
- **@test-acceptance.mdc**: Acceptance testing guidelines
- **@requirements-change.mdc**: Guidance on when to create PRDs vs direct updates

## Related Subagents

- `/validate-tests`: Test validation for acceptance criteria

## PRD Document Priority

**Key Principle**: PRD documents take precedence over original feature documents.

**Why?**
- Original feature docs = historical record (immutable)
- PRD docs = current requirements (mutable)
- Preserves evolution of requirements
- Maintains audit trail

**When updating documentation:**
1. Search for PRD document first
2. If found: update PRD ONLY
3. If not found: update original feature doc
4. Never modify both for the same feature

---

**Status**: 🚧 Command Definition Complete
**Next Step**: Implement command logic in AI assistant workflow
**Related Command**: `/update-all-rules` (for cursor rules documentation)

