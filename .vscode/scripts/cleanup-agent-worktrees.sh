#!/bin/bash
# cleanup-agent-worktrees.sh
# Removes all agent worktrees created by spawn-agent-worktree.sh

# Get the repository root directory
REPO_PATH=$(git rev-parse --show-toplevel)
REPO_NAME=$(basename "$REPO_PATH")

# Base path for worktrees (under repo/.worktrees/)
BASE_PATH="$REPO_PATH/.worktrees"

# Worktrees to keep (won't be deleted)
KEEP_WORKTREES=(
    "$REPO_PATH"
)

cd "$REPO_PATH" || exit 1

echo "=== Current worktrees ==="
git worktree list
echo ""

# Get all worktrees under BASE_PATH
WORKTREES=$(git worktree list --porcelain | grep "^worktree $BASE_PATH" | sed 's/^worktree //')

REMOVED=0
SKIPPED=0
FAILED=0

for wt in $WORKTREES; do
    # Check if this worktree should be kept
    SKIP=false
    for keep in "${KEEP_WORKTREES[@]}"; do
        if [ "$wt" = "$keep" ]; then
            SKIP=true
            break
        fi
    done

    if [ "$SKIP" = true ]; then
        echo "Keeping: $wt"
        ((SKIPPED++))
        continue
    fi

    # Get the branch name for this worktree
    BRANCH=$(git worktree list --porcelain | grep -A2 "^worktree $wt$" | grep "^branch " | sed 's/^branch refs\/heads\///')

    echo "Removing worktree: $wt"

    # Try to remove without force first to check for uncommitted changes
    ERROR_OUTPUT=$(git worktree remove "$wt" 2>&1)

    if [ $? -eq 0 ]; then
        ((REMOVED++))

        # Delete the branch if it starts with "wt/"
        if [[ "$BRANCH" == wt/* ]]; then
            echo "  Deleting branch: $BRANCH"
            git branch -D "$BRANCH" 2>/dev/null
        fi
    else
        # Show the reason for failure
        echo "  ❌ Failed to remove: $ERROR_OUTPUT"
        ((FAILED++))

        # Prompt user for force removal on any failure
        echo ""
        read -p "  Do you want to force remove it? (Y/N): " -n 1 -r
        echo ""

        if [[ $REPLY =~ ^[Yy]$ ]]; then
            git worktree remove --force "$wt" 2>/dev/null
            if [ $? -eq 0 ]; then
                echo "  ✓ Force removed successfully"
                ((REMOVED++))
                ((FAILED--))

                # Delete the branch if it starts with "wt/"
                if [[ "$BRANCH" == wt/* ]]; then
                    echo "  Deleting branch: $BRANCH"
                    git branch -D "$BRANCH" 2>/dev/null
                fi
            else
                echo "  ❌ Force remove also failed"
            fi
        else
            echo "  Skipped force removal"
        fi
    fi
done

echo ""
echo "=== Summary ==="
echo "Removed: $REMOVED worktrees"
echo "Failed: $FAILED worktrees"
echo "Kept: $SKIPPED worktrees"

# Clean up orphaned wt/* branches (branches without worktrees)
echo ""
echo "=== Cleaning up orphaned branches ==="
ORPHANED_COUNT=0
for branch in $(git branch --format='%(refname:short)' | grep "^wt/"); do
    # Check if this branch has an associated worktree
    if ! git worktree list | grep -q "\\[$branch\\]"; then
        echo "Removing orphaned branch: $branch"
        git branch -D "$branch" 2>/dev/null
        if [ $? -eq 0 ]; then
            ((ORPHANED_COUNT++))
        fi
    fi
done

if [ $ORPHANED_COUNT -eq 0 ]; then
    echo "No orphaned branches found"
else
    echo "Removed: $ORPHANED_COUNT orphaned branches"
fi

echo ""
echo "=== Remaining worktrees ==="
git worktree list
