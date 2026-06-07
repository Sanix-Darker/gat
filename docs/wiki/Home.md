# GAT Wiki - Home

Welcome to the **GAT** (Git Automation Tool) wiki! This documentation covers everything you need to know about using and configuring gat.

##  Documentation Index

### Getting Started
- [Installation](Installation.md) - How to install gat
- [Quick Start](Quick-Start.md) - Get up and running in 5 minutes
- [Shell Integration](Shell-Integration.md) - Enable automatic `cd` support

### Core Features
- [Worktree Management](Worktree-Management.md) - Create and manage Git worktrees
- [Tmux Integration](Tmux-Integration.md) - AI-ready tmux sessions
- [Docker Integration](Docker-Integration.md) - Worktree-scoped containers
- [Search and Navigation](Search-Navigation.md) - Find and switch worktrees

### Configuration
- [Configuration Guide](Configuration.md) - Complete configuration reference
- [Tmux Layout Customization](Tmux-Layout.md) - Customize your tmux layout
- [Environment Variables](Environment-Variables.md) - All environment variables

### Advanced Topics
- [Architecture](Architecture.md) - How gat works internally
- [Troubleshooting](Troubleshooting.md) - Common issues and solutions
- [Contributing](Contributing.md) - How to contribute to gat

### Release Notes
- [Version 0.2.0](Release-0.2.0.md) - Latest release (Configurable Layouts)
- [Changelog](../../CHANGELOG.md) - Full changelog

##  What is GAT?

GAT is a ticket-oriented Git worktree management tool that makes parallel development workflows effortless. It combines:

- **Git Worktrees**: Work on multiple branches simultaneously
- **Tmux Integration**: AI-ready development environments
- **Docker Support**: Worktree-scoped containers
- **Shell Integration**: Seamless directory navigation

##  Key Features

### 1. Ticket-Oriented Workflow
```bash
gat 12345  # Creates/opens worktree for ticket TICKET-12345
```

### 2. Configurable Tmux Layouts
```bash
# Customize your dev environment
git config gat.tmuxLeftWidth 60
git config gat.tmuxBottomHeight 40
git config gat.tmuxShell /bin/zsh
```

### 3. Docker Integration
```bash
gat dx bash  # Enter worktree's docker container
```

### 4. Smart Search
```bash
gat search  # Interactive worktree picker with fzf
```

##  Recent Improvements (v0.2.0)

### Critical Bug Fixes
-  **Fixed FZF deadlock** - Properly handles large worktree lists
-  **Fixed watch mode** - Clean terminal exit on Ctrl+C
-  **Fixed Docker parser** - Robust YAML parsing with serde_yaml
-  **Fixed hardcoded paths** - Configurable shell instead of `/bin/bash`

### New Features
-  **Fully configurable tmux layouts** - Customize pane sizes, shell, commands
-  **Hierarchical configuration** - Env vars → git config → config file
-  **Structured logging** - Debug with `GAT_VERBOSE=1`
-  **Archive validation** - Early validation of archive directories

##  Quick Links

- [Full Configuration Reference](Configuration.md)
- [Tmux Layout Examples](Tmux-Layout.md#examples)
- [Troubleshooting Guide](Troubleshooting.md)
- [GitHub Repository](https://github.com/user/gat)

##  Example Workflows

### Parallel Feature Development
```bash
# Work on multiple features simultaneously
gat 12345  # Feature A
gat 12346  # Feature B (in different terminal)
gat 12347  # Bug fix (in third terminal)
```

### AI-Assisted Development
```bash
# Create tmux session with Codex
gat tmux 12345
# Opens three panes:
# - Left: Codex AI assistant
# - Right-top: Your editor
# - Right-bottom: Shell
```

### Docker Development
```bash
# Create worktree and enter container
gat 12345
gat dx bash
# Now inside container for that worktree
```

##  Learn More

Start with the [Quick Start Guide](Quick-Start.md) to get gat up and running, then explore the [Configuration Guide](Configuration.md) to customize it to your workflow.

---

**Version:** 0.2.0  
**Last Updated:** 2026-06-02  
**License:** UNLICENSED
