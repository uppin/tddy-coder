#!/bin/bash
# spawn-current-branch-worktree.sh
# Opens the current branch in a new detached worktree (including uncommitted changes)
# Useful for viewing the same branch in multiple windows

# Source common utilities
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/worktree-utils.sh"

# Get the repository root directory
REPO_PATH=$(git rev-parse --show-toplevel)

# Base path for worktrees (under repo/.worktrees/)
BASE_PATH="$REPO_PATH/.worktrees"

# Get current branch name
CURRENT_BRANCH=$(git rev-parse --abbrev-ref HEAD)

# Get worktree name from CLI argument or generate default
if [ -n "$1" ]; then
    WORKTREE_NAME=$(sanitize_worktree_name "$1")
else
    # Generate default worktree name with timestamp
    WORKTREE_NAME="view-$(date +%Y%m%d-%H%M%S)"
    
    # Prompt for optional custom name
    read -p "Worktree name (press Enter for '$WORKTREE_NAME'): " CUSTOM_NAME
    
    if [ -n "$CUSTOM_NAME" ]; then
        WORKTREE_NAME=$(sanitize_worktree_name "$CUSTOM_NAME")
    fi
fi

WORKTREE_PATH="$BASE_PATH/$WORKTREE_NAME"

# Pick a color based on worktree name
BG_COLOR=$(pick_worktree_color "$WORKTREE_NAME")

cd "$REPO_PATH" || exit 1

# Stash current changes if any (we'll apply them in the worktree)
STASH_CREATED=false
if ! git diff-index --quiet HEAD -- || [ -n "$(git ls-files --others --exclude-standard)" ]; then
    echo "Stashing current changes (including untracked files) to include in worktree..."
    git stash push -u -m "Temp stash for worktree $WORKTREE_NAME"
    STASH_CREATED=true
fi

# Create worktree from current HEAD (detached - not on any branch)
if [ -d "$WORKTREE_PATH/.git" ] || git worktree list | grep -q "$WORKTREE_PATH"; then
    echo "Worktree already exists: $WORKTREE_PATH"
else
    # Remove orphaned directory if it exists
    if [ -d "$WORKTREE_PATH" ]; then
        echo "Removing orphaned directory: $WORKTREE_PATH"
        rm -rf "$WORKTREE_PATH"
    fi
    
    echo "Creating worktree from current branch ($CURRENT_BRANCH) at commit $(git rev-parse --short HEAD)"
    echo "Location: $WORKTREE_PATH"
    if git worktree add --detach "$WORKTREE_PATH" HEAD; then
        echo "✓ Created worktree successfully (detached HEAD state)"
        
        # Apply stashed changes if we created a stash
        if [ "$STASH_CREATED" = true ]; then
            echo "Applying stashed changes to worktree..."
            if (cd "$WORKTREE_PATH" && git stash apply stash@{0}); then
                echo "✓ Applied changes to worktree"
                # Restore changes to original branch too
                echo "Restoring changes to original branch..."
                git stash pop
                echo "✓ Changes restored to both locations"
            else
                echo "❌ Failed to apply stash to worktree"
                git stash pop
            fi
        fi
    else
        echo "❌ Failed to create worktree"
        # Pop stash back if we created it
        if [ "$STASH_CREATED" = true ]; then
            git stash pop
        fi
        exit 1
    fi
fi

# Setup VSCode settings with custom background color
setup_worktree_vscode_settings "$WORKTREE_PATH" "$BG_COLOR"

# Open in Cursor (force new window)
if open_worktree_in_cursor "$WORKTREE_PATH" --new-window; then
    echo "Done! Opened current branch worktree in new Cursor window: $WORKTREE_PATH"
else
    echo "Done! Created current branch worktree: $WORKTREE_PATH"
fi

# Display summary (detached HEAD mode)
print_worktree_summary "$CURRENT_BRANCH" "(detached HEAD)" "$WORKTREE_PATH" "$CURRENT_BRANCH" "true"
