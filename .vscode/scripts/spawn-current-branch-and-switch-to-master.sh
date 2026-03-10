#!/bin/bash
# spawn-current-branch-and-switch-to-master.sh
# Opens current branch in a new worktree and switches current worktree to master
# Useful for continuing work on current branch in a new window while main worktree goes to master

# Source common utilities
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/worktree-utils.sh"

# Get the repository root directory
REPO_PATH=$(git rev-parse --show-toplevel)

# Base path for worktrees (under repo/.worktrees/)
BASE_PATH="$REPO_PATH/.worktrees"

# Get current branch name
CURRENT_BRANCH=$(git rev-parse --abbrev-ref HEAD)

if [ "$CURRENT_BRANCH" = "master" ] || [ "$CURRENT_BRANCH" = "main" ]; then
    echo "Error: Already on master/main branch. Nothing to do."
    exit 1
fi

# Sanitize branch name for worktree directory path (filesystem safety)
WORKTREE_NAME=$(sanitize_worktree_name "$CURRENT_BRANCH")

WORKTREE_PATH="$BASE_PATH/$WORKTREE_NAME"

# Pick a color based on worktree name
BG_COLOR=$(pick_worktree_color "$WORKTREE_NAME")

cd "$REPO_PATH" || exit 1

# Check for uncommitted changes
if ! git diff-index --quiet HEAD -- || [ -n "$(git ls-files --others --exclude-standard)" ]; then
    echo "Error: You have uncommitted changes. Please commit or stash them first."
    git status --short
    exit 1
fi

# Create worktree and switch current to master
if [ -d "$WORKTREE_PATH/.git" ] || git worktree list | grep -q "$WORKTREE_PATH"; then
    echo "Worktree already exists: $WORKTREE_PATH"
else
    # Remove orphaned directory if it exists
    if [ -d "$WORKTREE_PATH" ]; then
        echo "Removing orphaned directory: $WORKTREE_PATH"
        rm -rf "$WORKTREE_PATH"
    fi
    
    echo "Current branch: $CURRENT_BRANCH"
    echo "Switching current worktree to master..."
    if git checkout master; then
        echo "✓ Switched to master"
        
        echo "Creating worktree for '$CURRENT_BRANCH': $WORKTREE_PATH"
        if git worktree add "$WORKTREE_PATH" "$CURRENT_BRANCH"; then
            echo "✓ Created worktree for '$CURRENT_BRANCH' successfully"
        else
            echo "❌ Failed to create worktree"
            # Try to switch back
            git checkout "$CURRENT_BRANCH"
            exit 1
        fi
    else
        echo "❌ Failed to switch to master"
        exit 1
    fi
fi

# Setup VSCode settings with custom background color
setup_worktree_vscode_settings "$WORKTREE_PATH" "$BG_COLOR"

# Open in Cursor (force new window)
if open_worktree_in_cursor "$WORKTREE_PATH" --new-window; then
    echo "Done! Opened '$CURRENT_BRANCH' in new Cursor window: $WORKTREE_PATH"
    echo "Current worktree is now on master"
else
    echo "Done! Created worktree for '$CURRENT_BRANCH': $WORKTREE_PATH"
    echo "Current worktree is now on master"
fi

# Display summary
print_worktree_summary "master" "$CURRENT_BRANCH" "$WORKTREE_PATH"

