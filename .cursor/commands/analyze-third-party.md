# Analyze Third-Party Projects

This command enables comprehensive analysis of external projects to understand their codebase, identify issues, and provide insights.

## Usage

When the user requests analysis of a third-party project, follow this systematic approach:

### 1. Input Handling

**Git Repository Analysis:**

- Clone the repository to `tmp/` directory using absolute paths
- Use format: `git clone <url> <path/to/repo>/tmp/<project-name>`
- Always use the project name from the URL as the directory name

**Local Directory Analysis:**

- Verify the provided path exists and is accessible
- Use the provided path directly for analysis

### 2. Project Structure Analysis

**Initial Discovery:**

1. List root directory contents to understand project type
2. Identify key files: `package.json`, `Cargo.toml`, `requirements.txt`, etc.
3. Read README files for project overview
4. Examine configuration files (CI/CD, build tools, etc.)

**Dependency Analysis:**

1. Parse dependency files to understand external libraries
2. Check for version constraints and potential conflicts
3. Identify deprecated or security-vulnerable dependencies
4. Analyze build and development dependencies separately

### 3. Code Quality Assessment

**Code Structure:**

- Examine source code organization
- Identify architectural patterns
- Check for proper separation of concerns
- Look for test coverage and test structure

**Potential Issues:**

- Search for common anti-patterns
- Identify error handling approaches
- Check for security vulnerabilities in code patterns
- Look for performance bottlenecks

### 4. Technology Stack Analysis

**Framework/Language Specific:**

- TypeScript/JavaScript: Check `tsconfig.json`, build tools, testing frameworks
- Rust: Examine `Cargo.toml`, workspace structure, feature flags
- Python: Check `requirements.txt`, virtual environment setup
- Other languages: Adapt analysis accordingly

**CI/CD and Deployment:**

- Examine GitHub Actions, GitLab CI, or other CI configurations
- Check Docker configurations if present
- Look for deployment scripts and configurations

### 5. Environment and Compatibility Issues

**Common CI/CD Problems:**

- Missing system dependencies
- Environment variable requirements
- Platform-specific issues (Linux vs macOS vs Windows)
- Version compatibility problems

**Build and Runtime Issues:**

- Compilation dependencies
- Runtime library requirements
- Font and system resource dependencies (especially for PDF/image processing)
- Network access requirements

### 6. Specific Analysis Focus Areas

**When analyzing issues like "0-byte outputs in CI":**

1. Compare CI environment setup vs local development
2. Check for system dependency installation in CI scripts
3. Examine error handling in core functionality
4. Look for silent failures in stream processing or file operations
5. Check for environment-specific resource requirements

### 7. Documentation and Reporting

**Analysis Structure:**

1. **Overview**: Project type, main purpose, technology stack
2. **Architecture**: High-level structure and patterns
3. **Dependencies**: Key libraries and potential issues
4. **Issues Identified**: Specific problems found with solutions
5. **Recommendations**: Best practices and improvements

**Issue Reporting Format:**

- Clearly categorize issues (Critical, Warning, Info)
- Provide specific file references with line numbers
- Include code examples when relevant
- Suggest concrete solutions with implementation details

### 8. Cleanup

**After Analysis:**

- Keep the clone of the repo untouched.
- Clean up any temporary files (that are not part of the 3rd party repo) created during analysis
- Preserve only essential findings in the summary

### 9. Best Practices

**Parallel Tool Usage:**

- Use multiple `read_file` calls simultaneously for efficiency
- Combine `grep` searches with different patterns in parallel
- Execute `codebase_search` queries simultaneously when exploring different aspects

**Thorough Investigation:**

- Don't stop at first findings - explore multiple angles
- Cross-reference issues across different parts of the codebase
- Look for patterns in similar issues reported by the community

**Actionable Insights:**

- Provide specific, implementable solutions
- Include code examples for fixes when possible
- Reference official documentation and best practices
- Suggest alternative approaches when applicable

## Example Usage Scenarios

1. **"Analyze https://github.com/owner/repo for CI failures"**
2. **"Examine ./local-project for security vulnerabilities"**
3. **"Review https://github.com/lib/package for performance issues"**
4. **"Investigate /path/to/project for dependency conflicts"**
