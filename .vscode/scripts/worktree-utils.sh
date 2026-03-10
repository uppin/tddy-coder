#!/bin/bash
# worktree-utils.sh
# Common utilities for worktree scripts

# Source cursor utilities for VSCode/Cursor integration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/cursor-utils.sh"

# Sanitize a name for use as a worktree directory name (filesystem-safe)
# Usage: sanitize_worktree_name "User Input Name"
# Returns: sanitized name via stdout (lowercase, underscores, alphanumeric only)
sanitize_worktree_name() {
    local name="$1"
    echo "$name" | tr '[:upper:]' '[:lower:]' | tr ' ' '_' | sed 's/[^a-z0-9_-]//g'
}

# Display colorful worktree summary
# Usage: print_worktree_summary "current_branch" "worktree_branch" "worktree_path" ["created_from_branch"] ["detached"]
print_worktree_summary() {
    local current_branch="$1"
    local worktree_branch="$2"
    local worktree_path="$3"
    local created_from="${4:-}"
    local is_detached="${5:-false}"
    
    # Color codes
    local GREEN='\033[0;32m'
    local BLUE='\033[0;34m'
    local CYAN='\033[0;36m'
    local YELLOW='\033[0;33m'
    local MAGENTA='\033[0;35m'
    local BOLD='\033[1m'
    local DIM='\033[2m'
    local RESET='\033[0m'
    
    echo ""
    echo -e "${BOLD}═══════════════════════════════════════════${RESET}"
    echo -e "${BOLD}           WORKTREE SUMMARY${RESET}"
    echo -e "${BOLD}═══════════════════════════════════════════${RESET}"
    echo -e "${CYAN}Current worktree:${RESET}  ${GREEN}${current_branch}${RESET}"
    
    if [ "$is_detached" = "true" ]; then
        echo -e "${CYAN}New worktree:${RESET}      ${MAGENTA}(detached HEAD)${RESET}"
    else
        echo -e "${CYAN}New worktree:${RESET}      ${BLUE}${worktree_branch}${RESET}"
    fi
    
    if [ -n "$created_from" ]; then
        echo -e "${CYAN}Created from:${RESET}      ${YELLOW}${created_from}${RESET}"
    fi
    
    echo -e "${CYAN}Location:${RESET}          ${DIM}${worktree_path}${RESET}"
    echo -e "${BOLD}═══════════════════════════════════════════${RESET}"
}
