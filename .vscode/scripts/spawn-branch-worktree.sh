#!/bin/bash
# spawn-feature-worktree.sh
# Creates a new feature worktree from master branch for feature development

# Source common utilities
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/worktree-utils.sh"

# Get the repository root directory
REPO_PATH=$(git rev-parse --show-toplevel)

# Base path for worktrees (under repo/.worktrees/)
BASE_PATH="$REPO_PATH/.worktrees"

BRANCH_NAME="$1"

if [ -z "$BRANCH_NAME" ]; then
    echo "Error: Branch name is required"
    echo "Usage: $0 [branch-name]"
    exit 1
fi

# Sanitize branch name for worktree directory path (filesystem safety)
WORKTREE_NAME=$(sanitize_worktree_name "$BRANCH_NAME")

WORKTREE_PATH="$BASE_PATH/$WORKTREE_NAME"

# Pick a color based on worktree name
BG_COLOR=$(pick_worktree_color "$WORKTREE_NAME")

cd "$REPO_PATH" || exit 1

# Check if branch already exists
BRANCH_EXISTS=false
if git show-ref --verify --quiet "refs/heads/$BRANCH_NAME"; then
    BRANCH_EXISTS=true
fi

# Create worktree from master or switch to existing branch
WORKTREE_CREATED=false
if [ -d "$WORKTREE_PATH/.git" ] || git worktree list | grep -q "$WORKTREE_PATH"; then
    echo "✓ Worktree already exists: $WORKTREE_PATH"
    echo "  Will open in Cursor..."
else
    WORKTREE_CREATED=true
    # Remove orphaned directory if it exists
    if [ -d "$WORKTREE_PATH" ]; then
        echo "Removing orphaned directory: $WORKTREE_PATH"
        rm -rf "$WORKTREE_PATH"
    fi

    if [ "$BRANCH_EXISTS" = true ]; then
        echo "Branch '$BRANCH_NAME' already exists, switching to it"
        echo "Creating worktree: $WORKTREE_PATH"
        if git worktree add "$WORKTREE_PATH" "$BRANCH_NAME"; then
            echo "✓ Created worktree for existing branch successfully"
        else
            echo "❌ Failed to create worktree"
            exit 1
        fi
    else
        echo "Creating new branch '$BRANCH_NAME' from master"
        echo "Creating worktree: $WORKTREE_PATH"
        if git worktree add "$WORKTREE_PATH" -b "$BRANCH_NAME" master; then
            echo "✓ Created new branch worktree successfully"
        else
            echo "❌ Failed to create worktree"
            exit 1
        fi
    fi
fi

# Setup VSCode settings with custom background color
setup_worktree_vscode_settings "$WORKTREE_PATH" "$BG_COLOR"

# Open in Cursor (force new window)
if [ "$WORKTREE_CREATED" = true ]; then
    ACTION_MSG="Created and opened"
else
    ACTION_MSG="Opened existing"
fi

if open_worktree_in_cursor "$WORKTREE_PATH" --new-window; then
    echo "✓ $ACTION_MSG feature worktree in new Cursor window: $WORKTREE_PATH"
else
    echo "⚠ Failed to open Cursor automatically"
    echo "  Worktree location: $WORKTREE_PATH"
    echo "  You can manually open it with: cursor $WORKTREE_PATH"
fi

# Get current branch and display summary
CURRENT=$(git rev-parse --abbrev-ref HEAD)
print_worktree_summary "$CURRENT" "$BRANCH_NAME" "$WORKTREE_PATH"

