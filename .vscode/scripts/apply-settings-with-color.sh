#!/bin/bash
# apply-settings-with-color.sh
# Apply settings template with optional color scheme to current or specified worktree
# Automatically detects dark/light mode and uses appropriate template

set -e

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
source "$SCRIPT_DIR/cursor-utils.sh"

# Get repo root
REPO_ROOT=$(git rev-parse --show-toplevel)

# Default to current worktree
WORKTREE_PATH="${1:-$REPO_ROOT}"

# Optional: branch name for deterministic color (default to current branch)
BRANCH_NAME="${2:-$(git rev-parse --abbrev-ref HEAD)}"

# Detect theme mode
THEME_MODE=$(detect_theme_mode)

# Pick color based on branch name
BG_COLOR=$(pick_worktree_color "$BRANCH_NAME")

echo "🎨 Applying settings template with color scheme"
echo "   Worktree: $WORKTREE_PATH"
echo "   Branch: $BRANCH_NAME"
echo "   Theme mode: $THEME_MODE"
echo "   Color: $BG_COLOR"
echo ""

# Apply settings with color
setup_worktree_vscode_settings "$WORKTREE_PATH" "$BG_COLOR"

echo ""
echo "✓ Settings applied successfully!"
echo ""
echo "To apply a different color, run:"
echo "  $0 <worktree-path> <branch-name>"
