#!/bin/bash
# cursor-utils.sh
# Common utilities for finding and using Cursor IDE

# Array of background colors to cycle through - more distinct but subtle
WORKTREE_COLORS=(
    "#1e2030"  # Soft blue-gray
    "#1f2d26"  # Muted forest green
    "#2d1f26"  # Soft burgundy
    "#2d2a1f"  # Warm brown
    "#1f2a2d"  # Steel blue
    "#2a1f2d"  # Deep purple
    "#2d251f"  # Copper
    "#1f2d2a"  # Teal
)

# Pick a background color based on worktree name (deterministic)
# Usage: pick_worktree_color "worktree-name"
# Returns: color hex string via stdout
pick_worktree_color() {
    local name="$1"
    local color_index=$(( $(echo "$name" | cksum | cut -d' ' -f1) % ${#WORKTREE_COLORS[@]} ))
    echo "${WORKTREE_COLORS[$color_index]}"
}

# Find and setup Cursor CLI
# Returns the path to the cursor command via stdout
# Returns 0 on success, 1 if not found
find_cursor_ide() {
    # On macOS, prioritize finding Cursor in Applications
    if [ "$(uname)" = "Darwin" ]; then
        if [ -f "/Applications/Cursor.app/Contents/Resources/app/bin/cursor" ]; then
            echo "/Applications/Cursor.app/Contents/Resources/app/bin/cursor"
            return 0
        fi
    fi

    # Fall back to cursor command in PATH
    if command -v cursor &> /dev/null; then
        echo "cursor"
        return 0
    fi

    # Not found - log to stderr so it doesn't interfere with stdout
    >&2 echo "⚠ Cursor CLI not found in standard locations"
    return 1
}


# Array of background colors to cycle through - more distinct but subtle
WORKTREE_COLORS=(
    "#1e2030"  # Soft blue-gray
    "#1f2d26"  # Muted forest green
    "#2d1f26"  # Soft burgundy
    "#2d2a1f"  # Warm brown
    "#1f2a2d"  # Steel blue
    "#2a1f2d"  # Deep purple
    "#2d251f"  # Copper
    "#1f2d2a"  # Teal
)

# Pick a background color based on worktree name (deterministic)
# Usage: pick_worktree_color "worktree-name"
# Returns: color hex string via stdout
pick_worktree_color() {
    local name="$1"
    local color_index=$(( $(echo "$name" | cksum | cut -d' ' -f1) % ${#WORKTREE_COLORS[@]} ))
    echo "${WORKTREE_COLORS[$color_index]}"
}

# Detect current theme mode (dark or light)
# Returns: "dark" or "light"
detect_theme_mode() {
    # First check Cursor/VSCode user settings
    local user_settings=""
    
    if [ -f "$HOME/Library/Application Support/Cursor/User/settings.json" ]; then
        user_settings="$HOME/Library/Application Support/Cursor/User/settings.json"
    elif [ -f "$HOME/Library/Application Support/Code/User/settings.json" ]; then
        user_settings="$HOME/Library/Application Support/Code/User/settings.json"
    fi
    
    if [ -n "$user_settings" ]; then
        # Check for explicit theme setting
        local theme=$(grep -o '"workbench.colorTheme"[[:space:]]*:[[:space:]]*"[^"]*"' "$user_settings" 2>/dev/null | sed 's/.*: *"\([^"]*\)".*/\1/')
        if [ -n "$theme" ]; then
            # Check if theme name contains "light", "bright", or "white"
            if echo "$theme" | grep -iE "light|bright|white" > /dev/null; then
                echo "light"
                return
            else
                echo "dark"
                return
            fi
        fi
    fi
    
    # Fallback to macOS system theme
    if [ "$(uname)" = "Darwin" ]; then
        if defaults read -g AppleInterfaceStyle 2>/dev/null | grep -q "Dark"; then
            echo "dark"
        else
            echo "light"
        fi
    else
        # Default to dark on non-macOS systems
        echo "dark"
    fi
}

# Setup VSCode settings for a worktree with custom background color
# Usage: setup_worktree_vscode_settings "/path/to/worktree" "#hexcolor"
setup_worktree_vscode_settings() {
    local worktree_path="$1"
    local bg_color="$2"
    local repo_path=$(git rev-parse --show-toplevel)
    
    # Detect current theme mode
    local theme_mode=$(detect_theme_mode)
    local settings_template="$repo_path/.vscode/settings-${theme_mode}.example.json"
    
    echo "Detected theme mode: $theme_mode"

    # Create .vscode directory if it doesn't exist
    mkdir -p "$worktree_path/.vscode"

    # Start with settings template if available, otherwise fallback
    local base_settings="$settings_template"
    if [ ! -f "$settings_template" ]; then
        echo "⚠ Warning: settings-${theme_mode}.example.json not found"
        # Try the opposite mode
        local fallback_mode="dark"
        if [ "$theme_mode" = "dark" ]; then
            fallback_mode="light"
        fi
        settings_template="$repo_path/.vscode/settings-${fallback_mode}.example.json"
        if [ -f "$settings_template" ]; then
            echo "Using fallback: settings-${fallback_mode}.example.json"
            base_settings="$settings_template"
        else
            echo "Using repo settings.json"
            base_settings="$repo_path/.vscode/settings.json"
        fi
    fi

    if [ -f "$base_settings" ]; then
        # Use jq if available to merge settings properly
        if command -v jq &> /dev/null; then
            jq --arg bgcolor "$bg_color" '. + {
                "workbench.colorCustomizations": {
                    "editor.background": $bgcolor,
                    "sideBar.background": $bgcolor,
                    "sideBarSectionHeader.background": $bgcolor,
                    "activityBar.background": $bgcolor,
                    "panel.background": $bgcolor,
                    "terminal.background": $bgcolor,
                    "titleBar.activeBackground": $bgcolor,
                    "titleBar.inactiveBackground": $bgcolor,
                    "statusBar.background": $bgcolor,
                    "statusBar.noFolderBackground": $bgcolor,
                    "statusBar.debuggingBackground": $bgcolor,
                    "tab.activeBackground": $bgcolor,
                    "tab.inactiveBackground": $bgcolor,
                    "editorGroupHeader.tabsBackground": $bgcolor,
                    "breadcrumb.background": $bgcolor
                }
            }' "$base_settings" > "$worktree_path/.vscode/settings.json"
            echo "Merged settings template with background color"
        else
            # Fallback: copy existing settings and append customizations
            cp "$base_settings" "$worktree_path/.vscode/settings.json"

            # Use sed to insert customizations before the closing brace
            sed -i.bak '$ d' "$worktree_path/.vscode/settings.json"
            cat >> "$worktree_path/.vscode/settings.json" << EOF
,
  "workbench.colorCustomizations": {
    "editor.background": "$bg_color",
    "sideBar.background": "$bg_color",
    "sideBarSectionHeader.background": "$bg_color",
    "activityBar.background": "$bg_color",
    "panel.background": "$bg_color",
    "terminal.background": "$bg_color",
    "titleBar.activeBackground": "$bg_color",
    "titleBar.inactiveBackground": "$bg_color",
    "statusBar.background": "$bg_color",
    "statusBar.noFolderBackground": "$bg_color",
    "statusBar.debuggingBackground": "$bg_color",
    "tab.activeBackground": "$bg_color",
    "tab.inactiveBackground": "$bg_color",
    "editorGroupHeader.tabsBackground": "$bg_color",
    "breadcrumb.background": "$bg_color"
  }
}
EOF
            rm "$worktree_path/.vscode/settings.json.bak" 2>/dev/null
            echo "Copied settings template and added background color"
        fi
    else
        # No existing settings template or repo settings, create fresh with color
        cat > "$worktree_path/.vscode/settings.json" << EOF
{
  "workbench.colorCustomizations": {
    "editor.background": "$bg_color",
    "sideBar.background": "$bg_color",
    "sideBarSectionHeader.background": "$bg_color",
    "activityBar.background": "$bg_color",
    "panel.background": "$bg_color",
    "terminal.background": "$bg_color",
    "titleBar.activeBackground": "$bg_color",
    "titleBar.inactiveBackground": "$bg_color",
    "statusBar.background": "$bg_color",
    "statusBar.noFolderBackground": "$bg_color",
    "statusBar.debuggingBackground": "$bg_color",
    "tab.activeBackground": "$bg_color",
    "tab.inactiveBackground": "$bg_color",
    "editorGroupHeader.tabsBackground": "$bg_color",
    "breadcrumb.background": "$bg_color"
  }
}
EOF
        echo "Created new settings with background color"
    fi
}

# Open a worktree in Cursor IDE (detached process)
# Usage: open_worktree_in_cursor "/path/to/worktree" [--new-window]
# Returns: 0 on success, 1 if Cursor not found
open_worktree_in_cursor() {
    local worktree_path="$1"
    local new_window_flag="$2"

    if CURSOR_CMD=$(find_cursor_ide); then
        echo "🚀 Opening Cursor IDE..."
        echo "   Command: $CURSOR_CMD"
        echo "   Path: $worktree_path"
        if [ "$new_window_flag" = "--new-window" ]; then
            echo "   Mode: New window"
        else
            echo "   Mode: Current window"
        fi
        
        # Spawn Cursor as a detached process so parent shell exit doesn't kill it
        # - Redirect stdout/stderr to /dev/null
        # - Run in background (&)
        # - Disown to detach from shell job control
        if [ "$new_window_flag" = "--new-window" ]; then
            nohup "$CURSOR_CMD" --new-window "$worktree_path" > /dev/null 2>&1 &
            CURSOR_PID=$!
        else
            nohup "$CURSOR_CMD" "$worktree_path" > /dev/null 2>&1 &
            CURSOR_PID=$!
        fi
        
        echo "   PID: $CURSOR_PID"
        
        # Give it a moment to start, then check if process exists
        sleep 0.5
        if ps -p "$CURSOR_PID" > /dev/null 2>&1; then
            echo "   ✓ Process started successfully"
        else
            echo "   ⚠ Process may have exited immediately (PID $CURSOR_PID not found)"
            echo "   This might be normal if Cursor reuses an existing process"
        fi
        
        disown 2>/dev/null || true
        return 0
    else
        echo "Error: Cursor CLI not found. Please either:"
        if [ "$(uname)" = "Darwin" ]; then
            echo "  1. Install Cursor at /Applications/Cursor.app, or"
            echo "  2. Install Cursor CLI: Cursor -> Shell Command: Install 'cursor' command in PATH"
        else
            echo "  - Install Cursor CLI: Cursor -> Shell Command: Install 'cursor' command in PATH"
        fi
        echo ""
        echo "You can manually open: $worktree_path"
        return 1
    fi
}
