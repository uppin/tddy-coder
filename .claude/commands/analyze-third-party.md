Comprehensive analysis of an external project or repository.

Ask the user for the target: either a Git repository URL to clone, or a local path to an existing project.

## Steps

### 1. Obtain the Project

**If a Git URL is provided:**
```
git clone <url> /tmp/third-party-analysis
```
Work from the cloned directory for all analysis.

**If a local path is provided:**
Work directly from that path. Do not modify any files.

### 2. Project Structure Analysis

Map the project layout:
- Top-level directory structure
- Build system (Cargo.toml, package.json, Makefile, etc.)
- Workspace/monorepo structure if applicable
- Entry points (main files, lib files, binary targets)
- Configuration files
- Test directory structure

### 3. Code Quality Assessment

Analyze representative source files for:
- Code organization and module structure
- Error handling patterns
- Documentation coverage (doc comments, README)
- Test coverage (presence and quality of tests)
- Dependency count and freshness
- Code style consistency

### 4. Technology Stack Analysis

Identify:
- Programming language(s) and versions
- Framework(s) and major libraries
- Build tools and task runners
- CI/CD configuration
- Deployment artifacts

### 5. Environment and Compatibility

Check for:
- System dependencies or native libraries required
- Platform-specific code
- Minimum language/runtime version requirements
- Known deprecation warnings in dependencies
- License information

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

## Summary Assessment
<2-3 paragraph overall assessment: strengths, weaknesses, risks, and recommendations>
```

### 7. Cleanup

If a repository was cloned to `/tmp/third-party-analysis`, ask the user if they want to keep it or remove it:
```
rm -rf /tmp/third-party-analysis
```
