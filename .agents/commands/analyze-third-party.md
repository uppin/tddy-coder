---
description: Comprehensive analysis of an external project or repository — structure, code quality, technology stack, dependencies, and environment compatibility.
---

Comprehensive analysis of an external project or repository.

Ask the user for the target: either a Git repository URL to clone, or a local path to an existing project.

Typical requests: "analyze https://github.com/owner/repo for CI failures", "examine ./local-project
for security vulnerabilities", "review https://github.com/lib/package for performance issues",
"investigate /path/to/project for dependency conflicts".

## Steps

### 1. Obtain the Project

**If a Git URL is provided:**
```
git clone <url> /tmp/third-party-analysis
```
Work from the cloned directory for all analysis. Use absolute paths throughout.

**If a local path is provided:**
Verify the path exists and is accessible, then work directly from it. Do not modify any files.

### 2. Project Structure Analysis

Map the project layout:
- Top-level directory structure
- Build system (Cargo.toml, package.json, requirements.txt, Makefile, etc.)
- Workspace/monorepo structure if applicable
- Entry points (main files, lib files, binary targets)
- Configuration files
- Test directory structure

Read README files for the project overview before drawing conclusions.

### 3. Code Quality Assessment

Analyze representative source files for:
- Code organization and module structure; architectural patterns and separation of concerns
- Error handling patterns
- Documentation coverage (doc comments, README)
- Test coverage (presence and quality of tests)
- Dependency count and freshness
- Code style consistency
- Common anti-patterns, security-relevant code patterns, and performance bottlenecks

### 4. Technology Stack Analysis

Identify:
- Programming language(s) and versions
- Framework(s) and major libraries
- Build tools and task runners
- CI/CD configuration (GitHub Actions, GitLab CI, Docker, deployment scripts)
- Deployment artifacts

Language-specific entry points:
- TypeScript/JavaScript: `tsconfig.json`, build tools, testing frameworks
- Rust: `Cargo.toml`, workspace structure, feature flags
- Python: `requirements.txt`, virtual environment setup
- Other languages: adapt accordingly

Analyze build/development dependencies separately from runtime dependencies, and check version
constraints for conflicts.

### 5. Environment and Compatibility

Check for:
- System dependencies or native libraries required
- Platform-specific code (Linux vs macOS vs Windows)
- Minimum language/runtime version requirements
- Environment variable requirements
- Font and system resource dependencies (notably for PDF/image processing)
- Network access requirements
- Known deprecation warnings in dependencies
- License information

**When investigating a specific symptom** (e.g. "0-byte outputs in CI"):
1. Compare CI environment setup against local development
2. Check whether CI scripts install the needed system dependencies
3. Examine error handling in the core functionality
4. Look for silent failures in stream processing or file operations
5. Check for environment-specific resource requirements

### 6. Report

Present findings as:

```
## Project Overview
- Name: <project name>
- Language: <primary language>
- Type: <library/binary/web app/etc.>
- License: <license>

## Structure
<directory tree of key paths>

## Technology Stack
| Category | Technology |
|----------|-----------|
| Language | <lang + version> |
| Framework | <framework> |
| Build | <build system> |
| Tests | <test framework> |
| CI/CD | <ci system> |

## Code Quality
- Organization: <rating + notes>
- Error handling: <rating + notes>
- Documentation: <rating + notes>
- Test coverage: <rating + notes>
- Style consistency: <rating + notes>

## Dependencies
- Total: <count>
- Notable: <list key dependencies>
- Concerns: <outdated, vulnerable, or heavyweight deps>

## Compatibility Notes
<any platform, version, or environment concerns>

## Issues Identified
<categorized Critical / Warning / Info, each with file:line, a code example where relevant,
and a concrete suggested fix>

## Summary Assessment
<2-3 paragraph overall assessment: strengths, weaknesses, risks, and recommendations>
```

### 7. Cleanup

Leave the cloned repository itself untouched. Remove only temporary files you created during the
analysis that are not part of the third-party repo.

If a repository was cloned to `/tmp/third-party-analysis`, ask the user if they want to keep it or remove it:
```
rm -rf /tmp/third-party-analysis
```

## Working Practices

- **Parallel tool usage:** issue multiple file reads at once, run grep searches with different
  patterns in parallel, and explore different aspects of the codebase simultaneously.
- **Thorough investigation:** don't stop at the first finding — explore multiple angles and
  cross-reference issues across different parts of the codebase.
- **Actionable insights:** give specific, implementable solutions with code examples, reference
  official documentation, and suggest alternatives where applicable.
