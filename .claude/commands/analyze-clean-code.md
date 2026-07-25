Analyze code quality metrics for all changed files in the current branch.

## Steps

### 1. Identify Changes

Run `git diff main...HEAD --name-only` to get all changed files. Analyze each non-test source file.

### 2. Metrics to Evaluate

**File length (lines, whole file — not just the diff):**
- Acceptable: 500 lines or fewer
- Needs attention: more than 500 — mark the file for review and propose how to split it

Measure the file's total length, not the size of the change in it. A one-line edit to a
1,200-line module still flags that module. Report it as a review item, not as a regression
introduced by the current change, and say which it is.

For every flagged file, propose concrete options: name the cohesive groups of code you found
(by responsibility, not by line range), the module each would become, and what the split would
cost — shared private state that would have to become `pub(crate)`, call sites that would move,
tests that would need repointing. If splitting is genuinely not worth it, say so and why.

**Function length (lines):**
- Excellent: 20 lines or fewer
- Acceptable: 21-40 lines
- Needs attention: 41-60 lines
- Must refactor: more than 60 lines

**Nesting depth (max levels of indentation in a function):**
- Excellent: 2 or fewer
- Acceptable: 3
- Needs attention: 4
- Must refactor: more than 4

**Parameter count (per function):**
- Excellent: 3 or fewer
- Acceptable: 4
- Needs attention: 5
- Must refactor: more than 5 — consider using an options struct

**Magic values:**
- Unnamed numeric constants
- Hardcoded string literals that represent configuration
- Repeated literal values that should be constants

**Code duplication:**
- Repeated code blocks (3+ lines appearing more than once)
- Similar functions that could be generalized
- Copy-pasted logic with minor variations

**Naming quality:**
- Single-letter variable names (except idiomatic `i`, `n`, etc.)
- Misleading names (name suggests one thing, code does another)
- Inconsistent naming conventions within a module
- Abbreviations that reduce readability

### 3. Output Format

Present findings as:

```
## Overall Score: <A/B/C/D/F>

## Metrics Summary

| Metric | Excellent | Acceptable | Needs Attention | Must Refactor |
|--------|-----------|------------|-----------------|---------------|
| File length | — | <count ≤500> | <count >500> | — |
| Function length | <count> | <count> | <count> | <count> |
| Nesting depth | <count> | <count> | <count> | <count> |
| Parameter count | <count> | <count> | <count> | <count> |

## Priority Fixes (Must Refactor)

### <file path>:<function name>
Metric: <which metric>
Current: <current value>
Target: <target value>
Suggestion: <how to refactor>

## Improvements (Needs Attention)

### <file path>:<function name>
Metric: <which metric>
Current: <current value>
Suggestion: <how to improve>

## Oversized Files (more than 500 lines) — flagged for review

### <file path> — <n> lines (<pre-existing | grown in this change>)
Responsibilities found: <cohesive group 1>, <cohesive group 2>, …
Proposed split: <module name> ← <group>; <module name> ← <group>
Cost: <shared state to expose, call sites to move, tests to repoint>
Recommendation: <split now | split later | leave as is, because …>

## Magic Values Found
- <file>:<line> — <value> — suggestion: <named constant>

## Duplication Found
- <file A>:<lines> and <file B>:<lines> — <description>
```

### 4. Scoring

- **A**: No "must refactor" items, fewer than 3 "needs attention"
- **B**: No "must refactor" items, 3+ "needs attention"
- **C**: 1-2 "must refactor" items
- **D**: 3-5 "must refactor" items
- **F**: More than 5 "must refactor" items

### 5. Update Changeset

If a changeset document exists in `docs/dev/1-WIP/`, update with the code quality score and priority fixes.

If "must refactor" items are found, ask the user whether to proceed with refactoring or just report.
