# Tmux Layout Customization

Complete guide to customizing GAT's tmux session layouts.

## 📖 Table of Contents

- [Overview](#overview)
- [Quick Start](#quick-start)
- [Configuration Methods](#configuration-methods)
- [Layout Parameters](#layout-parameters)
- [Preset Layouts](#preset-layouts)
- [Custom Layouts](#custom-layouts)
- [Examples](#examples)
- [Troubleshooting](#troubleshooting)

---

## Overview

GAT creates AI-ready tmux sessions with configurable layouts. Version 0.2.0 introduces full customization of:

- Pane sizes (percentage-based)
- Shell selection
- Commands per pane
- Focus control

### Default Layout

```
┌─────────────────┬──────────┐
│                 │          │
│                 │  Editor  │
│   AI/Codex      │  (45%)   │
│   (55%)         ├──────────┤
│                 │  Shell   │
│                 │  (35%)   │
└─────────────────┴──────────┘
```

**Dimensions:**
- Left pane: 55% width
- Right top: 65% of right column height
- Right bottom: 35% of right column height
- Focus: Left (AI) pane

---

## Quick Start

### 1. Check Current Configuration

```bash
gat doctor
```

Output shows current shell and settings.

### 2. Simple Customization

```bash
# Make AI pane larger
git config gat.tmuxLeftWidth 70

# Make shell pane taller
git config gat.tmuxBottomHeight 40

# Use zsh instead of bash
git config gat.tmuxShell /bin/zsh

# Use aider instead of codex
git config gat.tmuxCodexCmd "aider"
```

### 3. Test It

```bash
gat tmux 12345 --dry-run
# Shows planned layout

gat tmux 12345
# Creates session with your settings
```

---

## Configuration Methods

GAT supports **hierarchical configuration** with this priority (highest to lowest):

1. **CLI Arguments** (always wins)
2. **Environment Variables**
3. **Repository Git Config**
4. **Global Git Config**
5. **Config File**
6. **Defaults**

### Method 1: Command Line (Temporary)

```bash
gat tmux 12345 \
  --codex-cmd "aider" \
  --editor-cmd "code ." \
  --no-attach
```

**Use when:** Testing layouts, one-off changes

### Method 2: Environment Variables (Session)

```bash
# Add to ~/.bashrc or ~/.zshrc
export GAT_TMUX_LEFT_WIDTH=70
export GAT_TMUX_BOTTOM_HEIGHT=40
export GAT_TMUX_SHELL=/bin/zsh
export GAT_TMUX_CODEX_CMD="aider"
export GAT_TMUX_EDITOR_CMD="nvim"
```

**Use when:** User-wide preferences

### Method 3: Git Config (Repository)

```bash
# Repository-specific
cd /path/to/repo
git config gat.tmuxLeftWidth 70
git config gat.tmuxBottomHeight 40
git config gat.tmuxShell /bin/zsh
git config gat.tmuxCodexCmd "cursor"
git config gat.tmuxEditorCmd "nvim"

# Global (all repositories)
git config --global gat.tmuxLeftWidth 70
```

**Use when:** Project-specific or global preferences

### Method 4: Config File (Permanent)

Create `~/.config/gat/config.toml`:

```toml
ticket_prefix = "MYPROJ"

[tmux]
left_width = 70
bottom_height = 40
shell = "/bin/zsh"
codex_cmd = "aider"
editor_cmd = "nvim"
focus_left = true
```

**Use when:** Complex configurations, multiple settings

---

## Layout Parameters

### Pane Dimensions

| Parameter | Type | Range | Default | Description |
|-----------|------|-------|---------|-------------|
| `left_width` | u8 | 1-100 | 55 | Left pane width percentage |
| `bottom_height` | u8 | 1-100 | 35 | Bottom pane height percentage |

**Important:** Values are percentages of available space:
- `left_width=55` means left pane is 55%, right column is 45%
- `bottom_height=35` means bottom pane is 35% of right column

### Commands

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `shell` | string | `$SHELL` or `/bin/bash` | Shell for all panes |
| `codex_cmd` | string | `codex` | Command for AI pane |
| `editor_cmd` | string | `nvim` | Command for editor pane |

**Variable Substitution:**
- `{prompt_file}` - Path to prompt markdown file
- `{worktree}` - Path to worktree root
- `{ticket}` - Ticket number (e.g., TICKET-12345)

Example:
```bash
git config gat.tmuxEditorCmd "nvim {prompt_file}"
```

### Focus Control

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `focus_left` | bool | true | Focus left (AI) pane on creation |

```bash
# Focus editor pane instead of AI
git config gat.tmuxFocusLeft false
```

---

## Preset Layouts

### Classic (Default)

```
┌─────────────────┬──────────┐
│                 │ Editor   │
│   AI (55%)      │ (65%)    │
│                 ├──────────┤
│                 │ Shell    │
└─────────────────┴──────────┘
```

**Configuration:**
```bash
git config gat.tmuxLeftWidth 55
git config gat.tmuxBottomHeight 35
```

### AI-Focus

```
┌────────────────────────┬────┐
│                        │ Ed │
│   AI (70%)             │    │
│                        ├────┤
│                        │ Sh │
└────────────────────────┴────┘
```

**Configuration:**
```bash
git config gat.tmuxLeftWidth 70
git config gat.tmuxBottomHeight 40
```

**Use when:** Heavy AI usage, code review

### Editor-Focus

```
┌──────┬───────────────────────┐
│      │                       │
│  AI  │   Editor (70%)        │
│      │                       │
│(30%) ├───────────────────────┤
│      │   Shell (25%)         │
└──────┴───────────────────────┘
```

**Configuration:**
```bash
git config gat.tmuxLeftWidth 30
git config gat.tmuxBottomHeight 25
```

**Use when:** Heavy editing, occasional AI help

### Side-by-Side

```
┌────────────────────┬────────────────────┐
│                    │                    │
│   Editor (50%)     │   AI (50%)         │
│                    │                    │
│                    │                    │
│                    │                    │
└────────────────────┴────────────────────┘
```

**Configuration:**
```bash
git config gat.tmuxLeftWidth 50
git config gat.tmuxBottomHeight 0  # Effectively 2 panes
```

**Use when:** Pair programming with AI, side-by-side work

---

## Custom Layouts

### Multi-Pane Advanced (Future Feature)

**Status:** Planned for v0.3.0

Will support custom layouts via TOML:

```toml
[tmux.layout]
type = "custom"

[[tmux.layout.panes]]
name = "editor"
command = "nvim {prompt_file}"
width = 50
focus = true

[[tmux.layout.panes]]
name = "ai"
command = "aider"
split = "horizontal"
width = 30

[[tmux.layout.panes]]
name = "terminal"
split = "horizontal"
width = 20

[[tmux.layout.panes]]
name = "logs"
command = "tail -f app.log"
split = "vertical"
height = 30
parent = "terminal"
```

**Capabilities:**
- 4+ panes
- Nested splits
- Per-pane commands
- Per-pane directories
- Layout templates

---

## Examples

### Example 1: Aider + VS Code

```bash
# Use aider for AI, VS Code for editing
git config gat.tmuxCodexCmd "aider --model claude-3-opus"
git config gat.tmuxEditorCmd "code ."
git config gat.tmuxLeftWidth 60
```

### Example 2: Cursor + Terminal Focus

```bash
# Large terminal, small Cursor instance
export GAT_TMUX_CODEX_CMD="cursor"
export GAT_TMUX_LEFT_WIDTH=30
export GAT_TMUX_BOTTOM_HEIGHT=50  # Half the right column
```

### Example 3: Fish Shell

```bash
# Use fish instead of bash
git config gat.tmuxShell /usr/local/bin/fish
```

### Example 4: Helix Editor

```bash
# Use helix editor with custom layout
git config gat.tmuxEditorCmd "hx"
git config gat.tmuxLeftWidth 40
git config gat.tmuxBottomHeight 30
```

### Example 5: No AI (Just Editor + Shell)

```bash
# Minimal setup
git config gat.tmuxCodexCmd ""
git config gat.tmuxLeftWidth 1  # Minimal AI pane
git config gat.tmuxEditorCmd "nvim ."
```

### Example 6: Per-Project Settings

```bash
# Project A: Heavy AI usage
cd ~/projects/projectA
git config gat.tmuxLeftWidth 75
git config gat.tmuxCodexCmd "aider --model gpt-4"

# Project B: Editor focus
cd ~/projects/projectB
git config gat.tmuxLeftWidth 25
git config gat.tmuxCodexCmd "copilot"
```

### Example 7: NixOS Setup

```bash
# NixOS doesn't have /bin/bash
export GAT_TMUX_SHELL=$(which bash)

# Or use nix-shell wrapper
git config gat.tmuxShell "nix-shell --run bash"
```

---

## Troubleshooting

### Issue: Shell not found

**Error:**
```
shell /bin/bash not found
```

**Solution:**
```bash
# Check your shell
echo $SHELL

# Set it explicitly
git config gat.tmuxShell "$SHELL"

# Or find bash
git config gat.tmuxShell "$(which bash)"
```

### Issue: Wrong pane focused

**Problem:** Editor pane focused instead of AI

**Solution:**
```bash
git config gat.tmuxFocusLeft true
```

### Issue: Panes too small/large

**Problem:** Can't see content properly

**Solution:**
```bash
# Check current settings
gat tmux 12345 --dry-run

# Adjust
git config gat.tmuxLeftWidth 60  # Try different values
git config gat.tmuxBottomHeight 40
```

### Issue: Command not working

**Problem:** Custom command fails

**Solution:**
```bash
# Test command separately
/bin/zsh -c "aider"

# Check if command needs full path
which aider
git config gat.tmuxCodexCmd "$(which aider)"

# Or use absolute path
git config gat.tmuxCodexCmd "/Users/me/.local/bin/aider"
```

### Issue: Layout looks wrong

**Problem:** Percentages don't match visual layout

**Explanation:** Tmux calculates actual dimensions based on:
- Terminal size
- Font size
- Cell sizes
- Rounding

**Solution:**
```bash
# Use tmux list-windows to see actual dimensions
tmux list-windows -t gat-TICKET-12345

# Adjust percentages to compensate
```

### Issue: Environment variables not working

**Problem:** `GAT_TMUX_*` vars ignored

**Solution:**
```bash
# Check they're exported
env | grep GAT_TMUX

# Export them
export GAT_TMUX_LEFT_WIDTH=70

# Or add to shell rc file
echo 'export GAT_TMUX_LEFT_WIDTH=70' >> ~/.bashrc
source ~/.bashrc
```

---

## Advanced Tips

### Tip 1: Per-Session Layouts

```bash
# Different layouts for different ticket types
alias gat-feature="GAT_TMUX_LEFT_WIDTH=40 gat tmux"
alias gat-bugfix="GAT_TMUX_LEFT_WIDTH=70 gat tmux"

gat-feature 12345  # Editor-focused
gat-bugfix 12346   # AI-focused
```

### Tip 2: Save and Load Layouts

```bash
# Save current tmux layout
tmux list-windows -t gat-TICKET-12345 -F "#{window_layout}"

# Apply saved layout
tmux select-layout -t gat-TICKET-12345 "layout-string"
```

### Tip 3: Dynamic Sizing

```bash
# Adjust based on terminal size
if [ $COLUMNS -gt 200 ]; then
  export GAT_TMUX_LEFT_WIDTH=60
else
  export GAT_TMUX_LEFT_WIDTH=50
fi
```

### Tip 4: Integration with IDE

```bash
# Open current worktree in IDE
git config gat.tmuxEditorCmd "code . && tmux wait-for -S done"

# Split into IDE and terminal
git config gat.tmuxEditorCmd "code ."
git config gat.tmuxLeftWidth 1  # Minimal, IDE is external
```

---

## Reference

### All Configuration Keys

| Git Config Key | Env Variable | Type | Default |
|----------------|--------------|------|---------|
| `gat.tmuxLeftWidth` | `GAT_TMUX_LEFT_WIDTH` | u8 | 55 |
| `gat.tmuxBottomHeight` | `GAT_TMUX_BOTTOM_HEIGHT` | u8 | 35 |
| `gat.tmuxShell` | `GAT_TMUX_SHELL` | string | `$SHELL` |
| `gat.tmuxCodexCmd` | `GAT_TMUX_CODEX_CMD` | string | codex |
| `gat.tmuxEditorCmd` | `GAT_TMUX_EDITOR_CMD` | string | nvim |
| `gat.tmuxFocusLeft` | `GAT_TMUX_FOCUS_LEFT` | bool | true |

### Tmux Layout String Format

GAT uses tmux's percentage-based splits internally:

```bash
# Horizontal split: 55% left, 45% right
tmux split-window -h -l 45%

# Vertical split: 35% bottom
tmux split-window -v -l 35%
```

**Checksum:** Tmux layout strings include checksums for validation. GAT doesn't expose these directly but uses them internally for reliability.

---

## See Also

- [Configuration Guide](Configuration.md) - All configuration options
- [Tmux Integration](Tmux-Integration.md) - Complete tmux features
- [Quick Start](Quick-Start.md) - Get started quickly
- [Environment Variables](Environment-Variables.md) - All env vars

---

**Version:** 0.2.0  
**Last Updated:** 2026-06-02
