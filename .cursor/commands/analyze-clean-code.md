---
description: Analyzes code quality metrics including function length, nesting depth, parameter count, magic values, and duplication. Use proactively to assess code maintainability.
---

## Analyze Clean Code

This command analyzes code quality metrics to identify maintainability issues and assess code quality.

## Context Documents

**Expect in context**:
- Changeset document (`docs/dev/1-WIP/YYYY-MM-DD-*.md`) - tracks implementation progress
- PRD document (`docs/ft/*/1-WIP/YYYY-MM-DD-*.md`) - tracks requirement changes

**Use these documents to**:
- Focus analysis on affected packages
- Update "Validation Results" section in changeset
- Update Scope checkbox for "Code Quality"

## When Invoked

1. **Check for context documents**:
   - Read changeset if provided
   - Focus on affected packages listed

2. **Identify changed files**:
   ```bash
   git diff --name-only HEAD~1 | grep -E '\.(ts|tsx|js|jsx)$'
   ```

2. **Analyze each file** for clean code metrics.

## Metrics to Analyze

### Function Length
| Threshold | Rating |
|-----------|--------|
| ≤20 lines | ⭐⭐⭐ Excellent |
| 21-40 lines | ⭐⭐ Good |
| 41-60 lines | ⭐ Needs work |
| >60 lines | 🔴 Must refactor |

### Nesting Depth
| Threshold | Rating |
|-----------|--------|
| ≤2 levels | ⭐⭐⭐ Excellent |
| 3 levels | ⭐⭐ Good |
| 4 levels | ⭐ Needs work |
| >4 levels | 🔴 Must refactor |

### Parameter Count
| Threshold | Rating |
|-----------|--------|
| ≤3 params | ⭐⭐⭐ Excellent |
| 4 params | ⭐⭐ Good |
| 5 params | ⭐ Needs work |
| >5 params | 🔴 Use options object |

### Magic Values
Any literal value that should be a named constant:
- Numbers (except 0, 1, -1)
- Strings (except empty string)
- Repeated values

### Code Duplication
- Identical code blocks
- Similar logic that could be abstracted
- Copy-paste patterns

### Naming Quality
- Descriptive variable names
- Clear function names (verb + noun)
- Consistent naming conventions

## Output Format

```markdown
## Clean Code Analysis Report

### Files Analyzed
- file1.rs
- file2.rs

### Overall Score: X/10 ⭐

### Metrics Summary
| Metric | Score | Issues |
|--------|-------|--------|
| Function length | X/10 | Y functions too long |
| Nesting depth | X/10 | Y deeply nested blocks |
| Parameter count | X/10 | Y functions with many params |
| Magic values | X/10 | Y magic values found |
| Duplication | X/10 | Y duplicated blocks |
| Naming | X/10 | Y unclear names |

### Priority Fixes

#### 🔴 Critical (Score impact: high)
1. **[file:line]** Function `processData` is 85 lines
   - Current: Single function doing everything
   - Suggested: Split into 3-4 focused functions

#### ⚠️ Important (Score impact: medium)
1. **[file:line]** Magic number `86400`
   - Meaning: Seconds in a day
   - Fix: `const SECONDS_PER_DAY = 86400;`

#### ℹ️ Minor (Score impact: low)
1. **[file:line]** Variable `d` could be more descriptive
   - Suggested: `document` or `data`

### Refactoring Suggestions
1. Extract `validateInput()` from `processRequest()`
2. Create constant file for magic values
3. Use options object for `createUser(name, email, age, role, dept)`
```

## Update Changeset

If changeset document exists:

1. **Update Validation Results** section:

```markdown
### Code Quality (@analyze-clean-code)

**Last Run**: YYYY-MM-DD
**Overall Score**: X/10 ⭐

**Summary**:
- Critical issues: X
- Major issues: Y
- Minor issues: Z

**Priority Fixes**:
- [List critical issues with file:line]

**Added to Refactoring Needed**:
- [List items added]
```

2. **Update Scope** checkbox for "Code Quality" if score ≥ 7/10.

3. **Add issues** to **Refactoring Needed** section under `### From @analyze-clean-code`.

## Reference

- **Command**: `/analyze-clean-code`
- **Rules**: `@rust-code-style`, `@changeset-doc`
