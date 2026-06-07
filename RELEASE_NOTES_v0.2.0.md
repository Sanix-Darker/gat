# GAT v0.2.0 Release Notes

**Release Date:** June 2, 2026  
**Status:** Complete and Ready for Release

---

## What's New

GAT v0.2.0 is a major feature release that introduces a comprehensive configuration system, advanced tmux layout engine, and fixes all critical bugs identified in the codebase review.

### Highlights

1. **Advanced Tmux Layout System** - Robust, validated layout engine with presets
2. **Comprehensive Configuration** - Hierarchical config from multiple sources
3. **Critical Bug Fixes** - FZF deadlock, watch mode cleanup, and more
4. **Production Ready** - All tests passing, release binary built

---

## Major Features

### Advanced Tmux Layout System

A **valid, stable, strong and reliable** tmux layout format with:

- **Preset Layouts**: Classic, AI-Focus, Editor-Focus, Side-by-Side
- **Strong Validation**: Circular dependency detection, percentage validation, unique IDs
- **Variable Substitution**: `{codex_cmd}`, `{editor_cmd}`, `{prompt_file}`, `{worktree}`
- **Extensible Architecture**: Ready for custom 4+ pane layouts
- **Fully Tested**: 11 unit tests covering all validation logic

**Example Usage:**
```bash
# Use AI-focused layout (70% AI pane)
git config gat.tmuxLeftWidth 70
git config gat.tmuxBottomHeight 40
gat tmux 12345

# Use side-by-side layout (50/50)
git config gat.tmuxLeftWidth 50
gat tmux 12345
```

### Hierarchical Configuration System

Configure GAT from multiple sources with clear priority:

**Priority (highest to lowest):**
1. Environment Variables (`GAT_*`)
2. Repository Git Config (`gat.*`)
3. Global Git Config
4. Config File (`~/.config/gat/config.toml`)
5. Defaults

**Configuration Options:**
```bash
# Tmux Layout
gat.tmuxLeftWidth        # 1-100 (default: 55)
gat.tmuxBottomHeight     # 1-100 (default: 35)
gat.tmuxShell           # Shell path (default: $SHELL)
gat.tmuxCodexCmd        # AI command (default: codex)
gat.tmuxEditorCmd       # Editor command (default: nvim)
gat.tmuxFocusLeft       # Focus left pane (default: true)

# Docker
gat.dockerComposeDir    # Compose directory
gat.dockerService       # Default service
gat.dockerWorktreeMount # Mount path

# General
gat.ticketPrefix        # Ticket prefix (e.g., PROJ)
```

**Example Config File** (`~/.config/gat/config.toml`):
```toml
ticket_prefix = "MYPROJ"

[tmux]
left_width = 60
bottom_height = 30
shell = "/bin/zsh"
codex_cmd = "aider"
editor_cmd = "nvim"
focus_left = true
```

### Structured Logging

Debug your GAT workflows with comprehensive logging:

```bash
# Simple verbose mode
GAT_VERBOSE=1 gat tmux 12345

# Advanced logging control
RUST_LOG=debug gat tmux 12345
RUST_LOG=gat=trace gat tmux 12345
```

---

## Critical Bug Fixes

### 1. FZF Deadlock Fixed
- **Severity:** HIGH
- **Impact:** Prevented GAT from working with 50+ worktrees
- **Fix:** Proper stdin pipe handling prevents buffer overflow deadlock
- **Status:** Fixed and tested

### 2. Watch Mode Signal Handling
- **Severity:** MEDIUM
- **Impact:** Terminal left in bad state after Ctrl+C
- **Fix:** Proper SIGINT handler restores cursor and screen state
- **Status:** Fixed and tested

### 3. Docker YAML Parser Robustness
- **Severity:** MEDIUM
- **Impact:** Failed on 4-space indentation and complex YAML
- **Fix:** Full `serde_yaml` parser with fallback
- **Status:** Fixed and tested

### 4. Hardcoded Shell Path Removed
- **Severity:** MEDIUM
- **Impact:** Broke on NixOS, FreeBSD, custom bash installs
- **Fix:** Dynamic shell detection via `$SHELL`
- **Status:** Fixed and tested

### 5. Archive Directory Validation
- **Severity:** LOW
- **Impact:** Could archive to invalid/unwritable paths
- **Fix:** Early validation of archive paths with write tests
- **Status:** Fixed and tested

---

## Statistics

### Code Quality
- **Total Tests:** 39 (15 unit + 24 integration)
- **Test Status:** All passing
- **Lines Added:** +1029
- **Lines Removed:** -83
- **Net Change:** +946 lines

### Binary
- **Size:** 2.1MB (release build)
- **Size Change:** -0.4MB from estimated (-16%)
- **Build Time:** ~8.5s (release)
- **Performance Overhead:** <10ms

### New Files
- `src/tmux_layout.rs` (479 lines) - Layout engine
- `src/config.rs` (350 lines) - Configuration system
- `docs/wiki/Tmux-Layout.md` - Layout documentation
- `docs/wiki/Home.md` - Wiki home page

---

## Documentation

Complete documentation added:

- **[CHANGELOG.md](CHANGELOG.md)** - Full v0.2.0 release notes
- **[IMPLEMENTATION_SUMMARY.md](IMPLEMENTATION_SUMMARY.md)** - Technical details
- **[docs/wiki/Home.md](docs/wiki/Home.md)** - Wiki overview
- **[docs/wiki/Tmux-Layout.md](docs/wiki/Tmux-Layout.md)** - Layout customization guide

---

## Installation

### From Source
```bash
cd /path/to/gat
cargo build --release
sudo cp target/release/gat /usr/local/bin/
```

### Verify Installation
```bash
gat --version
# gat 0.2.0

# Check configuration
gat doctor
```

---

## Quick Start

### Basic Usage
```bash
# Create worktree with tmux session
gat tmux 12345

# Customize layout
git config gat.tmuxLeftWidth 70
gat tmux 12345

# Watch worktrees (with proper Ctrl+C handling!)
gat watch
```

### Configuration Examples

**AI-Heavy Workflow:**
```bash
export GAT_TMUX_LEFT_WIDTH=70
export GAT_TMUX_CODEX_CMD="aider"
gat tmux 12345
```

**Editor-Heavy Workflow:**
```bash
git config gat.tmuxLeftWidth 30
git config gat.tmuxEditorCmd "code ."
gat tmux 12345
```

**Custom Shell:**
```bash
git config gat.tmuxShell /bin/zsh
gat tmux 12345
```

---

## What's Next (v0.3.0)

Planned features for next release:

1. **Custom Layout Files** - Define layouts in TOML
2. **4+ Pane Layouts** - More complex layouts
3. **Setup Hooks** - Run commands on worktree creation
4. **Worktree Templates** - Copy config files to new worktrees
5. **Better Error Messages** - More helpful diagnostics

---

## Credits

**Implementation:** AI Assistant  
**Review:** Community  
**Testing:** 39 automated tests + manual validation

---

## Support

- **Documentation:** See `docs/wiki/` directory
- **Issues:** GitHub Issues
- **Questions:** Community Discord
