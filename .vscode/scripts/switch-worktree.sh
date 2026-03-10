#!/bin/bash
# switch-worktree.sh
# Interactive switcher for git worktrees

# Get the repository root directory
REPO_PATH=$(git rev-parse --show-toplevel 2>/dev/null)

if [ -z "$REPO_PATH" ]; then
    echo "Error: Not in a git repository"
    exit 1
fi

cd "$REPO_PATH" || exit 1

# Get all worktrees
WORKTREE_LIST=$(git worktree list)

if [ -z "$WORKTREE_LIST" ]; then
    echo "No worktrees found"
    exit 1
fi

echo "=== Available Worktrees ==="
echo "$WORKTREE_LIST"
echo ""

# Check if fzf is available
if command -v fzf &> /dev/null; then
    # Use fzf for selection
    SELECTED=$(echo "$WORKTREE_LIST" | fzf --height=10 --prompt="Select worktree: ")

    if [ -z "$SELECTED" ]; then
        echo "No selection made"
        exit 0
    fi

    # Extract the path (first column)
    WORKTREE_PATH=$(echo "$SELECTED" | awk '{print $1}')
else
    # Use simple numbered menu
    IFS=$'\n' read -d '' -r -a WORKTREES <<< "$WORKTREE_LIST"

    # Display numbered options
    for i in "${!WORKTREES[@]}"; do
        echo "$((i+1)). ${WORKTREES[$i]}"
    done
    echo ""

    # Prompt for selection
    read -p "Select worktree number (1-${#WORKTREES[@]}): " selection

    # Validate input
    if ! [[ "$selection" =~ ^[0-9]+$ ]] || [ "$selection" -lt 1 ] || [ "$selection" -gt "${#WORKTREES[@]}" ]; then
        echo "Invalid selection"
        exit 1
    fi

    # Get selected worktree path
    SELECTED_LINE="${WORKTREES[$((selection-1))]}"
    WORKTREE_PATH=$(echo "$SELECTED_LINE" | awk '{print $1}')
fi

if [ -z "$WORKTREE_PATH" ]; then
    echo "Error: Could not determine worktree path"
    exit 1
fi

# Source common cursor utilities
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/cursor-utils.sh"

echo "Opening: $WORKTREE_PATH"

if CURSOR_CMD=$(find_cursor_ide); then
    "$CURSOR_CMD" "$WORKTREE_PATH"
    echo "Done! Opened worktree in Cursor"
else
    echo "Error: Cursor CLI not found. Please either:"
    if [ "$(uname)" = "Darwin" ]; then
        echo "  1. Install Cursor at /Applications/Cursor.app, or"
        echo "  2. Install Cursor CLI: Cursor -> Shell Command: Install 'cursor' command in PATH"
    else
        echo "  - Install Cursor CLI: Cursor -> Shell Command: Install 'cursor' command in PATH"
    fi
    echo ""
    echo "You can manually open: $WORKTREE_PATH"
    exit 1
fi
