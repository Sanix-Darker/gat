# GAT Feature Research - Similar Tools Analysis

**Research Date:** June 2, 2026  
**Researched By:** AI Assistant  
**Purpose:** Identify interesting features from similar git worktree tools

---

## Executive Summary

Research conducted on git worktree automation tools including **workmux**, **dmux**, **cmux**, **ofsht**, **wtp**, **git-worktree-runner**, and others. This document outlines features that GAT currently has, features it lacks, and recommendations for future development.

---

## Competing Tools Overview

### 1. workmux (Rust)
- **URL:** https://github.com/raine/workmux
- **Focus:** Git worktrees + tmux windows for zero-friction parallel AI agent development
- **Key Features:**
  - One worktree = one tmux window = one agent model
  - Full merge lifecycle automation
  - Copy config files and symlink dependencies on creation
  - Run install commands automatically
  - Merge skill for autonomous agent workflow
  - Single command for: merge branch, delete worktree, close tmux window, remove local branch

### 2. dmux (CLI/TUI)
- **URL:** https://github.com/standardagents/dmux
- **Focus:** Dev agent multiplexer for git worktrees
- **Key Features:**
  - Multiplexes 11+ AI agents (Claude Code, Codex, Gemini CLI, etc.)
  - TUI interface for managing multiple agents
  - Agent orchestration across worktrees

### 3. cmux (Terminal App)
- **URL:** https://github.com/manaflow-ai/cmux
- **Focus:** Ghostty-based macOS terminal with vertical tabs
- **Key Features:**
  - Terminal with git branch and PR status in sidebar
  - Notification rings for agent activity
  - Vertical tab management
  - Does NOT create/manage worktrees (complementary tool)

### 4. ofsht (Rust)
- **URL:** https://github.com/wadackel/ofsht
- **Focus:** Git worktree CLI with automation
- **Key Features:**
  - Automated setup workflows
  - Command execution on worktree creation
  - Configuration management

### 5. wtp (Go)
- **URL:** https://github.com/satococoa/wtp
- **Focus:** Powerful git worktree CLI with automation
- **Key Features:**
  - Automated setup and branch tracking
  - Smart navigation between worktrees
  - Branch status monitoring

### 6. git-worktree-runner (Bash)
- **URL:** https://github.com/coderabbitai/git-worktree-runner
- **Focus:** Editor and AI tool integration
- **Key Features:**
  - Per-branch worktree creation
  - Configuration copying
  - Dependency installation automation
  - Workspace setup automation

---

## Feature Comparison Matrix

| Feature | GAT | workmux | dmux | ofsht | wtp | git-worktree-runner |
|---------|-----|---------|------|-------|-----|---------------------|
| **Core Worktree Management** |
| Create/delete worktrees | Yes | Yes | Yes | Yes | Yes | Yes |
| List worktrees | Yes | Yes | Yes | Yes | Yes | Yes |
| Archive worktrees | Yes | No | No | No | No | No |
| Prune stale/merged | Yes | No | No | No | No | No |
| Age tracking | No | No | No | No | No | No |
| **Tmux Integration** |
| Tmux session creation | Yes | Yes | Yes (via tmux) | No | No | No |
| Configurable layouts | Yes | No | No | N/A | N/A | N/A |
| Multiple layout presets | Yes | No | No | N/A | N/A | N/A |
| **Automation** |
| Copy config files | No | Yes | No | Yes | Yes | Yes |
| Symlink dependencies | No | Yes | No | Yes | Yes | Yes |
| Run setup commands | No | Yes | Yes | Yes | Yes | Yes |
| Post-creation hooks | No | Yes | Yes | Yes | No | Yes |
| **Merge/PR Workflow** |
| Merge automation | No | Yes | No | No | No | No |
| Full cleanup lifecycle | No | Yes | No | No | No | No |
| PR integration | No | Partial | No | No | No | No |
| Branch status tracking | No | No | No | No | Yes | No |
| **Agent Integration** |
| Multi-agent support | No | Yes | Yes | No | No | Yes |
| Agent orchestration | No | Yes | Yes | No | No | No |
| **Docker** |
| Docker integration | Yes | No | No | No | No | No |
| Container per worktree | Yes | No | No | No | No | No |
| **Configuration** |
| Hierarchical config | Yes | Partial | No | Partial | No | Partial |
| Per-project settings | Yes | No | No | No | No | Yes |
| **UI/UX** |
| FZF search | Yes | Partial | No | No | No | No |
| Watch mode | Yes | No | No | No | No | No |
| TUI interface | No | No | Yes | No | No | No |
| Shell integration | Yes | No | No | No | Yes | No |

---

## Features GAT HAS That Others Don't

1. **Docker Integration**
   - Worktree-scoped containers
   - Automatic container detection and entry
   - Docker compose integration
   - Service selection

2. **Advanced Tmux Layout System**
   - Multiple preset layouts (Classic, AI-Focus, Editor-Focus, Side-by-Side)
   - Percentage-based pane sizing
   - Fully validated layout engine
   - Variable substitution

3. **Archive Functionality**
   - Move worktrees to archive directory
   - Preserves git metadata
   - Reversible archiving

4. **Comprehensive Configuration**
   - Hierarchical config system (ENV > Git > File > Defaults)
   - Per-repository settings
   - Global defaults

5. **Watch Mode**
   - Real-time worktree monitoring
   - Auto-refresh on changes
   - Proper signal handling

6. **Smart Prune**
   - Detect merged branches
   - Detect stale worktrees
   - Safe removal with validation

---

## Features GAT LACKS (Opportunities for Enhancement)

### HIGH PRIORITY

#### 1. Worktree Template/Setup System
**What:** Automatically copy config files and run setup commands on worktree creation

**Use Case:**
```bash
# On worktree creation, automatically:
# - Copy .env.example to .env
# - Symlink node_modules from primary worktree
# - Run npm install
# - Run database migrations
# - Start development server

gat new 12345 --template nodejs-app
```

**Implementation:**
- Define templates in `.gat/templates/` directory
- Template config specifies:
  - Files to copy
  - Files to symlink
  - Commands to run
  - Environment variables to set
- Hooks system for pre/post creation actions

**Benefit:** Eliminates manual setup for each worktree, saves 5-10 minutes per worktree

#### 2. Merge Lifecycle Automation
**What:** Single command to merge branch, delete worktree, cleanup

**Use Case:**
```bash
# Instead of:
git checkout main
git merge TICKET-12345
git branch -d TICKET-12345
gat rm 12345
tmux kill-window -t gat-TICKET-12345

# Just:
gat merge 12345  # Does everything
```

**Implementation:**
- Validate worktree is clean
- Merge to target branch (configurable: main/develop)
- Delete worktree
- Close tmux session if exists
- Remove local branch
- Optionally push changes

**Benefit:** Reduces 5-6 commands to 1, prevents forgetting cleanup steps

#### 3. Worktree Age Tracking & Auto-Cleanup
**What:** Track when worktrees were created/last used, suggest cleanup

**Use Case:**
```bash
gat list --sort age  # Show oldest worktrees first
gat prune --older-than 30d  # Remove worktrees older than 30 days
gat doctor  # Warns about stale worktrees

# Output:
# Stale worktrees (not modified in 30+ days):
# - TICKET-11234 (45 days old, last used: 2026-01-15)
# - TICKET-11567 (60 days old, last used: 2026-01-01)
```

**Implementation:**
- Store metadata in `.gat/worktree-metadata.json`
- Track: creation_time, last_access_time, last_commit_time
- Update on `gat` commands
- Periodic cleanup suggestions

**Benefit:** Prevents disk space bloat, keeps workspace clean

#### 4. Branch/PR Status Integration
**What:** Show git branch status and associated PR in worktree listings

**Use Case:**
```bash
gat list

# Output:
# TICKET-12345  /worktrees/TICKET-12345  [ahead 3, behind 1]  PR#123 (open)
# TICKET-12346  /worktrees/TICKET-12346  [up-to-date]        PR#124 (merged)
# TICKET-12347  /worktrees/TICKET-12347  [ahead 5]           No PR
```

**Implementation:**
- Integrate with GitHub/GitLab APIs
- Show commits ahead/behind main
- Show associated PR status
- Configurable via `gat.githubToken` or `gat.gitlabToken`

**Benefit:** At-a-glance status, know what needs attention

### MEDIUM PRIORITY

#### 5. Shared Dependency Management
**What:** Symlink or share `node_modules`, `.venv`, etc. across worktrees

**Use Case:**
```bash
# Primary worktree has 2GB node_modules
# New worktrees symlink to it instead of re-downloading

gat new 12345 --link-deps
# Creates worktree with symlink: ./ node_modules -> ../../main/node_modules
```

**Implementation:**
- Detect dependency directories (node_modules, .venv, vendor, target)
- Create symlinks to primary worktree
- Handle conflicts (different dependency versions)
- Optional: use junction points on Windows

**Benefit:** Saves GB of disk space, faster worktree creation

#### 6. Multi-Agent Orchestration
**What:** Run multiple AI agents in parallel across different worktrees

**Use Case:**
```bash
# Spawn 3 agents working on different tickets
gat agent spawn 12345 --agent aider  # Agent 1
gat agent spawn 12346 --agent cursor  # Agent 2
gat agent spawn 12347 --agent codex  # Agent 3

gat agent list
# Shows active agents, their worktrees, status

gat agent kill 12345  # Stop agent in worktree 12345
```

**Implementation:**
- Track agent PIDs and worktrees
- Provide agent communication protocol
- Monitor agent health
- Aggregate agent logs

**Benefit:** Parallel development, multiply productivity

#### 7. Worktree Groups/Projects
**What:** Organize worktrees into logical groups

**Use Case:**
```bash
gat group create auth-refactor
gat group add auth-refactor 12345 12346 12347

gat group list auth-refactor
# Shows all worktrees in the group

gat group archive auth-refactor
# Archives entire group
```

**Implementation:**
- Store groups in `.gat/groups.json`
- Group operations (list, archive, delete)
- Visual indication in `gat list`

**Benefit:** Manage related work as a unit

#### 8. TUI (Text User Interface)
**What:** Interactive terminal UI for worktree management

**Use Case:**
```bash
gat tui

# Shows:
# - List of all worktrees
# - Status indicators
# - Arrow key navigation
# - Quick actions (create, delete, open, merge)
# - Real-time updates
```

**Implementation:**
- Use `ratatui` crate (Rust TUI library)
- Keyboard shortcuts for all actions
- Mouse support optional
- Real-time refresh

**Benefit:** More intuitive than CLI for some users

### LOW PRIORITY

#### 9. Build Cache Sharing
**What:** Share build artifacts across worktrees

**Use Case:**
```bash
# Rust projects: share target/ directory
# JS projects: share .next/, dist/, build/

gat new 12345 --share-cache
```

**Implementation:**
- Detect build directories
- Use symlinks or copy-on-write
- Handle conflicts

**Benefit:** Faster builds, less disk usage

#### 10. Worktree Snapshots
**What:** Save/restore worktree state

**Use Case:**
```bash
gat snapshot create 12345 "before-refactor"
# ... make changes ...
gat snapshot restore 12345 "before-refactor"
```

**Implementation:**
- Use git stash or custom snapshot format
- Store in `.gat/snapshots/`

**Benefit:** Experimentation safety net

#### 11. Remote Worktree Support
**What:** Create worktrees on remote machines via SSH

**Use Case:**
```bash
gat remote add staging user@staging-server
gat remote create staging 12345
# Creates worktree on remote server
```

**Implementation:**
- SSH integration
- Remote command execution
- File synchronization

**Benefit:** Work with remote environments

#### 12. Integration with IDEs
**What:** VS Code / IntelliJ extensions for GAT

**Features:**
- Tree view of worktrees
- Right-click actions
- Status bar integration
- Quick switcher

**Benefit:** GUI users can benefit from GAT

---

## Unique Features GAT Could Pioneer

### 1. AI Agent Skill System
Integrate with agent skill files for autonomous workflows:

```bash
gat skill install @workmux/merge
gat skill install @gat/auto-test
gat skill install @gat/code-review

# Agent can now:
# - Autonomously merge PRs
# - Run tests before merge
# - Request code review
```

### 2. Worktree Health Monitoring
Track worktree health metrics:

```bash
gat doctor --full

# Checks:
# - Disk space usage
# - Uncommitted changes
# - Untracked files
# - Behind main by X commits
# - Open PRs
# - Docker containers running
# - Stale tmux sessions
# - Broken symlinks
```

### 3. Smart Conflict Detection
Warn before creating worktrees that might conflict:

```bash
gat new 12345

# Warning: Worktree TICKET-11234 modified the same files
# Potential conflicts: src/auth.rs, src/config.rs
# Continue? (y/N)
```

### 4. Worktree Diff Visualization
Show changes across all worktrees:

```bash
gat diff-all

# Shows:
# - Files changed in each worktree
# - Line changes summary
# - Potential merge conflicts
```

### 5. Cost/Resource Tracking
Track resource usage per worktree:

```bash
gat stats 12345

# Disk usage: 2.3 GB
# Docker container: 512 MB RAM
# CPU time: 45 minutes
# Created: 2026-06-01
# Last used: 2 hours ago
# Commits: 15
# Lines changed: +450, -120
```

---

## Implementation Roadmap

### v0.3.0 - Essential Automation
- Worktree templates and setup hooks
- Merge lifecycle automation
- Age tracking and auto-cleanup suggestions
- PR/branch status integration

### v0.4.0 - Advanced Workflows
- Shared dependency management
- Multi-agent orchestration
- Worktree groups/projects
- TUI interface

### v0.5.0 - Polish & Innovation
- Build cache sharing
- Worktree snapshots
- Smart conflict detection
- Health monitoring
- Resource tracking

### v1.0.0 - Ecosystem
- IDE extensions
- Remote worktree support
- Agent skill system
- Worktree diff visualization

---

## Recommended Immediate Priorities

Based on research and user value, implement these features first:

1. **Worktree Templates** (High Impact, Medium Effort)
   - Biggest productivity gain
   - Eliminates repetitive setup
   - Foundation for other automation

2. **Merge Lifecycle Automation** (High Impact, Low Effort)
   - Simple to implement
   - Immediate value
   - Reduces human error

3. **Age Tracking** (Medium Impact, Low Effort)
   - Prevents disk bloat
   - Easy to implement
   - Low maintenance overhead

4. **PR Status Integration** (High Impact, Medium Effort)
   - Highly requested by users
   - Improves workflow visibility
   - Differentiates from competitors

---

## Technical Considerations

### Configuration Format
```toml
# .gat/config.toml

[templates.nodejs]
copy = [".env.example:.env", ".vscode/"]
link = ["node_modules"]
run = ["npm install", "npm run db:migrate"]

[templates.rust]
link = ["target"]
run = ["cargo build"]

[worktree]
auto_cleanup_days = 30
track_usage = true

[github]
token = "ghp_..."
show_pr_status = true

[merge]
target_branch = "main"
auto_push = true
delete_branch = true
close_tmux = true
```

### Metadata Storage
```json
// .gat/worktree-metadata.json
{
  "worktrees": {
    "TICKET-12345": {
      "created_at": "2026-06-01T10:00:00Z",
      "last_accessed_at": "2026-06-02T14:30:00Z",
      "last_commit_at": "2026-06-02T12:00:00Z",
      "template": "nodejs",
      "pr_number": 123,
      "agent": "aider",
      "tags": ["auth", "refactor"]
    }
  }
}
```

---

## Conclusion

GAT already has strong fundamentals with its tmux integration, Docker support, and configuration system. The research shows clear opportunities for enhancement in three areas:

1. **Automation** - Template system and setup hooks
2. **Lifecycle Management** - Merge automation and cleanup
3. **Visibility** - Age tracking and PR status

Implementing these features will position GAT as the most comprehensive git worktree tool available, combining the best ideas from workmux, dmux, and others while maintaining its unique strengths.

---

**Sources:**
- workmux: https://github.com/raine/workmux
- dmux: https://github.com/standardagents/dmux
- cmux: https://github.com/manaflow-ai/cmux
- ofsht: https://github.com/wadackel/ofsht
- wtp: https://github.com/satococoa/wtp
- git-worktree-runner: https://github.com/coderabbitai/git-worktree-runner
- Various blog posts and discussions on git worktree automation

Content rephrased for compliance with licensing restrictions.
