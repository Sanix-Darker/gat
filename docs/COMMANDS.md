# gat Command Reference

`gat` is a ticket-oriented Git worktree workflow helper. It layers a fast,
opinionated command surface over native `git worktree`, `tmux`, `fzf`, Docker
Compose, and shell integration, while keeping Git as the single source of
truth.

This document is the complete reference for every command and option. For a
condensed overview see `README.md`; for terminal man pages see `docs/man/`
(install with `make install-man`).

---

## Table of contents

- [Conventions](#conventions)
- [Global behavior](#global-behavior)
  - [Output formats](#output-formats)
  - [Exit codes](#exit-codes)
  - [Environment variables](#environment-variables)
  - [Configuration sources and precedence](#configuration-sources-and-precedence)
  - [Ticket normalization](#ticket-normalization)
  - [Shortcuts](#shortcuts)
  - [Progress output](#progress-output)
- [Commands](#commands)
  - [new (alias: add, and the bare `gat <ticket>` form)](#new)
  - [go](#go)
  - [switch](#switch)
  - [describe (alias: desc)](#describe)
  - [sessions](#sessions)
  - [ui (alias: dashboard)](#ui)
  - [config](#config)
  - [merge](#merge)
  - [path](#path)
  - [list (alias: ls)](#list)
  - [watch](#watch)
  - [search (alias: find)](#search)
  - [tmux (alias: session, start)](#tmux)
  - [dx (alias: docker)](#dx)
  - [rm (alias: remove, delete)](#rm)
  - [archive](#archive)
  - [prune](#prune)
  - [shell-init](#shell-init)
  - [doctor](#doctor)
  - [help / --version](#help--version)

---

## Conventions

- `<angle brackets>` denote required values you supply.
- `[square brackets]` denote optional arguments.
- `a|b` denotes mutually exclusive choices.
- "ticket" means the worktree identifier you pass to most commands. It can be a
  bare number (which gets a prefix, e.g. `12345` becomes `TICKET-12345`), an
  already-prefixed identifier (`TICKET-12345`), or any branch name.
- "primary worktree" is the main checkout Git reports first in
  `git worktree list`. "linked worktree" is any additional worktree.

---

## Global behavior

### Output formats

Most commands accept `--format <text|json|shell>`:

- `text` (default): human-readable output.
- `json`: a single JSON object or array, suitable for scripting and piping to
  `jq`.
- `shell`: `KEY=value` assignment lines intended to be `eval`'d by the shell
  integration so a parent shell can `cd` after the process exits.

Shorthands: `--json` is equivalent to `--format json`; some commands also accept
`--shell` for `--format shell`.

### Exit codes

- `0`: success.
- `1`: a runtime or Git failure (for example a refused unsafe operation, a
  missing worktree, or a failed Git command).
- `2`: a CLI usage error (unknown option, missing required argument).

### Environment variables

| Variable | Effect |
|----------|--------|
| `GAT_TICKET_PREFIX` | Default prefix for numeric tickets (default `TICKET`). |
| `GAT_TMUX_LAYOUT` | Layout preset: `classic`, `ai-focus`, `editor-focus`, `wide`. |
| `GAT_TMUX_LEFT_WIDTH` | Left pane width percent (0-100). |
| `GAT_TMUX_BOTTOM_HEIGHT` | Bottom-right pane height percent (0-100). |
| `GAT_TMUX_SHELL` | Shell used inside tmux panes. |
| `GAT_TMUX_CODEX_CMD` | Command launched in the AI pane. |
| `GAT_TMUX_EDITOR_CMD` | Editor launched in the editor pane. |
| `GAT_DOCKER_COMPOSE_DIR` | Compose directory relative to the worktree (default `.docker`). |
| `GAT_DOCKER_WORKTREE_MOUNT` | Container mount path matched to the worktree (default `/cbr/apps`). |
| `GAT_DOCKER_SERVICE` | Default Docker Compose service. |
| `GAT_QUIET` | Suppress progress lines on stderr. |
| `GAT_VERBOSE` | Enable info-level logging. |
| `RUST_LOG` | Standard `env_logger` filter (e.g. `debug`). |
| `XDG_CONFIG_HOME` | Overrides the config root (`$XDG_CONFIG_HOME/gat`). |
| `GAT_SHELL_INTEGRATION` | Set internally by the shell wrapper; signals shell mode. |

### Configuration sources and precedence

Settings are merged from several sources. Highest priority wins:

1. Command-line flags (e.g. `--layout`, `--prefix`).
2. Environment variables (`GAT_*`).
3. Repository and global Git config (`gat.*` keys).
4. The config file at `${XDG_CONFIG_HOME:-~/.config}/gat/config.toml`.
5. Built-in defaults.

See [config](#config) for managing the file, and the
[Git config keys](#git-config-keys) table under that command.

### Ticket normalization

When a target is a bare number, `gat` prepends a prefix and a hyphen:

- `gat 12345` -> branch/worktree `TICKET-12345` (default prefix `TICKET`).
- `gat 12345 --prefix ABC` -> `ABC-12345`.
- `gat 12345 --no-prefix` -> `12345` unchanged.
- Non-numeric targets (`feature/login`, `TICKET-9`) are never modified.

The prefix comes from `--prefix`, then `gat.ticketPrefix` Git config, then
`GAT_TICKET_PREFIX`, then the `TICKET` default.

### Shortcuts

`go` and `path` accept navigation shortcuts as the target:

- `^`: the default branch's worktree.
- `@`: the current worktree.

`--base`/`--from` on `new` also accept:

- `^`: the default branch.
- `@`: the current branch.

### Progress output

Mutating commands (`new`, `tmux`, `switch`, `rm`, `archive`, `prune`, `merge`,
`describe`) print short progress lines to **stderr**, prefixed with `gat:`, so
stdout stays clean for scripts and shell integration. Set `GAT_QUIET=1` to
silence them.

---

## Commands

### new

Create or reuse a ticket worktree. This is the primary workflow, and the bare
`gat <ticket>` form is shorthand for `gat new <ticket>`.

**Synopsis**

```
gat <ticket> [options]
gat new <ticket> [options]
gat add <ticket> [options]
```

**Description**

Creates a sibling worktree next to the primary checkout. For a repository at
`/repos/project` and ticket `TICKET-12345`, the default worktree path is
`/repos/project-TICKET-12345`. If a worktree for the branch already exists, the
command is idempotent and simply reports the existing worktree rather than
creating a new one.

On creation, `gat`:

1. Resolves the ticket name (see [Ticket normalization](#ticket-normalization)).
2. Picks a base ref (the resolved `--base`, else the repository default branch).
3. Creates the branch if it does not exist, then adds the worktree.
4. Records creation time in the usage metadata store.
5. Applies a setup template if configured (see `--template`).
6. Stores a description if `--description` was given.

When the shell integration is active, creating or reusing a worktree also
changes the current directory into it.

**Options**

| Option | Description |
|--------|-------------|
| `--prefix <prefix>` | Prefix for numeric tickets (default `TICKET`). |
| `--no-prefix` | Do not prefix a numeric ticket. |
| `--branch <branch>` | Use this branch name instead of the ticket name. |
| `--base, --from, --from-ref <ref>` | Base ref for a new branch. Accepts `^` (default branch) and `@` (current branch). |
| `--path <path>` | Override the computed worktree path. |
| `--detach` | Create a detached worktree (no branch) for investigation. |
| `-d, --description <text>` | Attach a description to the worktree. |
| `-t, --template <name>` | Apply a named setup template after creation. |
| `--no-template` | Skip templating even if a `default` template exists. |
| `-n, --dry-run` | Print the planned action without changing anything. |
| `--format <text\|json\|shell>` | Output format. `--json`/`--shell` shorthands accepted. |

**Examples**

```bash
# Create TICKET-12345 next to the repo, switching into it (with shell integration).
gat 12345

# Preview only.
gat 12345 --dry-run

# Custom prefix and base branch.
gat 4567 --prefix ABC --base develop

# Use an explicit branch name and a description.
gat new login-fix --branch feature/login --description "fix login redirect"

# Detached investigation worktree from the current branch.
gat new spike --detach --from @

# Apply a template.
gat 12345 --template node
```

**Related**

- [go](#go) to switch to an existing worktree without creating one.
- [tmux](#tmux) to create a worktree and open a tmux session in one step.
- [Worktree templates](#config) under config for `--template`.

---

### go

Resolve and switch to an existing worktree. With shell integration, this
changes the current directory; without it, it prints the path.

**Synopsis**

```
gat go <ticket|^|@> [options]
```

**Description**

Looks up a worktree by branch name, path basename, full path, or a shortcut
(`^` for the default branch's worktree, `@` for the current worktree), records
an access timestamp, and reports the path. Unlike `new`, it does not create a
worktree unless `--create` is passed.

**Options**

| Option | Description |
|--------|-------------|
| `-c, --create` | Create the worktree if it does not exist (delegates to `new`). |
| `--prefix <prefix>` | Prefix for numeric ticket targets. |
| `--no-prefix` | Do not prefix a numeric target. |
| `--format <text\|json\|shell>` | Output format. |

**Examples**

```bash
# Switch to the TICKET-12345 worktree (cd with shell integration).
gat go 12345

# Jump to the default branch's worktree.
gat go ^

# Create on demand if missing.
gat go 12345 --create

# Print the path as JSON.
gat go 12345 --json
```

**Related**

- [path](#path) to print only the path.
- [switch](#switch) to attach a tmux session instead of cd-ing.

---

### switch

Attach to (or open) the tmux session for an existing worktree.

**Synopsis**

```
gat switch <ticket> [options]
```

**Description**

Resolves what to do in this order:

1. If a tmux session for the ticket is already running, attach to it (or switch
   the client when already inside tmux).
2. Otherwise, if the worktree exists, build the gat tmux layout and attach.
3. Otherwise, fail with a message telling you to create the worktree first.

Unlike [tmux](#tmux), `switch` never creates a worktree; the target must exist.
The session name folds in the worktree description when one is set (truncated to
100 characters), and gat records `@gat_path`, `@gat_branch`, and
`@gat_description` options on the session for tooling.

**Options**

| Option | Description |
|--------|-------------|
| `--session <name>` | Override the tmux session name. |
| `--prompt-file <path>` | Prompt draft path used when creating panes. |
| `--codex-cmd <cmd>` | Command for the AI pane (default `codex`). |
| `--editor-cmd <cmd>` | Editor for the prompt pane (default `nvim`). |
| `--layout <preset>` | Layout preset: `classic`, `ai-focus`, `editor-focus`, `wide`. |
| `--no-attach` | Ensure the session exists without attaching. |
| `-n, --dry-run` | Print the planned action only. |
| `--format <text\|json\|shell>` | Output format. |
| `--prefix <prefix>` / `--no-prefix` | Ticket prefix handling. |

**Examples**

```bash
# Attach to (or open) the session for TICKET-12345.
gat switch 12345

# See what would happen without touching tmux.
gat switch 12345 --dry-run --json

# Open with the AI-focused layout but do not attach.
gat switch 12345 --layout ai-focus --no-attach
```

**Related**

- [sessions](#sessions) to list live sessions before switching.
- [tmux](#tmux) which can also create the worktree.

---

### describe

Set or show a worktree's description. Descriptions appear in `gat list` and are
folded into tmux session names.

**Synopsis**

```
gat describe <ticket> [text...] [--clear]
gat desc <ticket> [text...]
```

**Description**

With no text, prints the current description. With trailing words, joins them
into a single description and stores it. `--clear` removes the description. The
target worktree must exist.

**Options**

| Option | Description |
|--------|-------------|
| `--clear` | Remove the description. |
| `--prefix <prefix>` / `--no-prefix` | Ticket prefix handling. |
| `--format <text\|json>` | Output format. |

**Examples**

```bash
# Set a multi-word description (no quotes needed).
gat describe 12345 fix the login redirect bug

# Show the current description.
gat describe 12345

# Clear it.
gat describe 12345 --clear

# As JSON.
gat describe 12345 --json
```

**Related**

- [list](#list) shows descriptions in a column.
- [new](#new) accepts `--description` at creation time.

---

### sessions

List live gat-managed tmux sessions.

**Synopsis**

```
gat sessions [--format <text|json>]
```

**Description**

Enumerates tmux sessions whose names start with `gat-` and reports each one's
attach state, window count, branch, path, and description (read from the
`@gat_*` options gat sets when it creates a session). Sessions created outside
gat are not listed. When no tmux server is running, prints a friendly "No gat
tmux sessions." message and exits successfully. Requires `tmux` on `PATH`.

**Options**

| Option | Description |
|--------|-------------|
| `--format <text\|json>` | Output format. `--json` shorthand accepted. |

**Examples**

```bash
# Human-readable table.
gat sessions

# Machine-readable for scripting.
gat sessions --json | jq '.[] | select(.attached)'
```

**Output columns (text)**

`Session`, `A` (attached marker `*`), `Win` (window count), `Branch`, and
`Description / Path`.

**Related**

- [switch](#switch) to attach to one of the listed sessions.
- [tmux](#tmux) to create sessions.

---

### ui

Launch the interactive TUI dashboard. Designed to run inside a tmux pane.

**Synopsis**

```
gat ui [--fast]
gat dashboard [--fast]
```

**Description**

Opens a keyboard-driven dashboard with two tabs, Worktrees and Sessions, showing
change stats, idle time, and descriptions. From the dashboard you can navigate,
filter, switch to a session, edit a description, and remove a worktree.

> The TUI is gated behind the optional `tui` Cargo feature so the standard build
> stays lean. Build or install it with `cargo install --path . --features tui`.
> Without the feature, `gat ui` prints a message explaining how to enable it.

**Key bindings**

| Key | Action |
|-----|--------|
| `j` / `k` or Down / Up | Move selection |
| `Tab` | Toggle Worktrees / Sessions list |
| `Enter` | Switch to (or open) the selected worktree's session |
| `d` | Describe the selected worktree |
| `x` | Remove the selected worktree (asks `y/N`) |
| `/` | Incremental filter (`Enter` commits, `Esc` cancels) |
| `r` | Refresh the snapshot |
| `q` / `Esc` | Quit |

**Options**

| Option | Description |
|--------|-------------|
| `--fast` | Skip dirty/merged/change-stat checks for instant startup. |

**Examples**

```bash
gat ui
gat ui --fast
```

**Related**

- [list](#list) and [sessions](#sessions) provide the same data non-interactively.

---

### config

Inspect and edit the gat configuration file.

**Synopsis**

```
gat config init [--force]
gat config path
gat config list [--format <text|json>]
gat config get <key>
gat config set <key> <value...>
```

**Description**

Manages the config file at `${XDG_CONFIG_HOME:-~/.config}/gat/config.toml`.

- `init` writes a default config file. Refuses to overwrite an existing file
  unless `--force` is given.
- `path` prints the config file path.
- `list` shows every recognized key with its effective value (after the file,
  Git config, and environment layers are merged).
- `get <key>` prints one key's effective value; exits non-zero if unset.
- `set <key> <value>` writes a key to the config file only (never capturing a
  Git/env override) and validates the value.

**Recognized keys**

| Key | Type | Notes |
|-----|------|-------|
| `ticket_prefix` | string | Empty value clears it. |
| `docker_compose_dir` | string | |
| `docker_worktree_mount` | string | |
| `docker_service` | string | |
| `verbose` | bool | `true/false/1/0/yes/no/on/off`. |
| `tmux.layout` | preset | Write-only sugar; sets the geometry keys below. |
| `tmux.left_width` | percent | 0-100. |
| `tmux.bottom_height` | percent | 0-100. |
| `tmux.shell` | string | |
| `tmux.codex_cmd` | string | |
| `tmux.editor_cmd` | string | |
| `tmux.focus_left` | bool | |

<a id="git-config-keys"></a>
**Equivalent Git config keys** (read-only overlay, set with `git config`):
`gat.ticketPrefix`, `gat.dockerComposeDir`, `gat.dockerWorktreeMount`,
`gat.dockerService`, `gat.tmuxLayout`, `gat.tmuxShell`, `gat.tmuxCodexCmd`,
`gat.tmuxEditorCmd`, `gat.tmuxLeftWidth`, `gat.tmuxBottomHeight`.

**Worktree templates** are configured as `[template.<name>]` sections in the
config file, each supporting `copy`, `symlink`, and `run` list keys:

```toml
[template.default]
copy = [".env.example", "config/local.json"]
symlink = ["node_modules", "target"]
run = ["npm install"]
```

**Options**

| Option | Description |
|--------|-------------|
| `-f, --force` | (init) overwrite an existing config file. |
| `--format <text\|json>` | (list) output format. |

**Examples**

```bash
gat config init
gat config path
gat config set ticket_prefix ABC
gat config set tmux.layout ai-focus
gat config get tmux.left_width
gat config list --json
```

**Related**

- [Worktree templates](#new) used by `gat new --template`.
- [tmux](#tmux) layout presets.

---

### merge

Merge a ticket branch into the default branch and optionally clean up.

**Synopsis**

```
gat merge <ticket> [options]
```

**Description**

Runs the merge in the **primary worktree** with strict safety checks, then
optionally removes the worktree, deletes the branch, and kills the tmux session.

Safety guarantees:

- The target worktree must exist, be non-primary, and be clean.
- The branch cannot be the base branch.
- The primary worktree must be clean and already have the base branch checked
  out. `gat` refuses rather than switching branches for you.
- A merge conflict is aborted (`git merge --abort`) so the repository is left
  unchanged.

Cleanup steps only run after a successful merge. If the branch is already merged,
the merge step is skipped but cleanup still runs.

**Options**

| Option | Description |
|--------|-------------|
| `--into <branch>` | Merge into this branch (default: repository default branch). |
| `--no-ff` | Always create a merge commit. |
| `--remove`, `--rm` | Remove the worktree after a successful merge. |
| `--delete-branch` | Remove the worktree and delete the merged branch. |
| `--kill-session` | Kill the worktree's tmux session after the merge. |
| `--cleanup` | Shorthand for `--remove --delete-branch --kill-session`. |
| `-y, --yes` | Skip the confirmation prompt. |
| `-n, --dry-run` | Print the planned merge/cleanup only. |
| `--prefix <prefix>` / `--no-prefix` | Ticket prefix handling. |
| `--format <text\|json\|shell>` | Output format. |

**Examples**

```bash
# Merge TICKET-12345 into the default branch (with confirmation).
gat merge 12345

# Merge and fully clean up, no prompt.
gat merge 12345 --cleanup --yes

# Merge into a specific branch with a merge commit.
gat merge 12345 --into develop --no-ff

# Preview.
gat merge 12345 --dry-run --json
```

**Related**

- [prune](#prune) `--merged` to bulk-remove already-merged worktrees.
- [rm](#rm) for removing a worktree without merging.

---

### path

Print only the resolved path to a worktree.

**Synopsis**

```
gat path <ticket|^|@> [options]
```

**Description**

Resolves a worktree the same way [go](#go) does, but always prints just the path
to stdout and never changes directory. Useful in scripts and command
substitution.

**Options**

| Option | Description |
|--------|-------------|
| `--prefix <prefix>` | Prefix for numeric ticket targets. |
| `--no-prefix` | Do not prefix a numeric target. |

**Examples**

```bash
# Print the path.
gat path 12345

# Use it in another command.
cd "$(gat path 12345)"
code "$(gat path ^)"
```

**Related**

- [go](#go) which also switches directories.

---

### list

List registered worktrees with status, change stats, and idle time.

**Synopsis**

```
gat list [--format <text|json>] [--fast]
gat ls [...]
```

**Description**

Shows every worktree with its branch, state (primary/detached/dirty/merged/
locked/missing), short HEAD, a change summary, idle time, and description. In
full mode it computes per-worktree status in parallel across worker threads. The
`@` marker indicates the current worktree.

The `Changes` column is rendered as `<files>f +<insertions> -<deletions>`, where
files counts staged, unstaged, and untracked paths, and insertions/deletions
come from `git diff --shortstat HEAD`. The `Idle` column shows days since the
worktree was last accessed (falling back to filesystem mtime when no usage is
tracked).

**Options**

| Option | Description |
|--------|-------------|
| `--fast` | Skip the per-worktree dirty/merged/diff checks for instant output. |
| `--watch` | Continuously re-render (delegates to `watch`). |
| `--format <text\|json>` | Output format. `--json` shorthand accepted. |

**JSON fields**

`path`, `branch`, `head`, `primary`, `detached`, `dirty`, `merged`,
`changed_files`, `insertions`, `deletions`, `locked`, `prunable`, `idle_days`,
`description`.

**Examples**

```bash
gat list
gat ls --fast
gat list --json | jq '.[] | select(.dirty)'
```

**Related**

- [watch](#watch) for a live view.
- [search](#search) for fuzzy selection.

---

### watch

Continuously re-render the worktree list until interrupted.

**Synopsis**

```
gat watch [--interval <ms>] [--once] [--full] [--fast]
```

**Description**

Clears the screen and redraws the `list` table on an interval. Defaults to fast
mode (skipping expensive checks) so it stays responsive with many worktrees.
Ctrl+C restores the cursor and exits cleanly.

**Options**

| Option | Description |
|--------|-------------|
| `--interval <ms>` | Refresh interval in milliseconds (default 1000, minimum 100). |
| `--once` | Render once and exit (useful for tests and scripts). |
| `--full` | Include dirty/merged/change-stat checks. |
| `--fast` | Skip expensive checks (default). |

**Examples**

```bash
gat watch
gat watch --interval 2000 --full
gat watch --once
```

**Related**

- [list](#list) for a one-shot listing.
- [ui](#ui) for an interactive alternative.

---

### search

Search worktrees, optionally through `fzf`.

**Synopsis**

```
gat search [query] [options]
gat find [query] [options]
```

**Description**

Builds a stable, tab-separated feed of `branch<TAB>state<TAB>path`. By default it
pipes the feed through `fzf` for interactive selection; the selected worktree
can be switched to (with shell integration), printed, or opened in tmux. With
`--print` or `--no-fzf` it emits the raw feed for use with `awk`, `grep`, or
other tools.

**Options**

| Option | Description |
|--------|-------------|
| `--print` | Print the tab-separated feed instead of invoking `fzf`. |
| `--path` | Print only the selected path. |
| `--tmux` | Open the selected worktree's tmux session. |
| `--no-fzf` | Do not invoke `fzf`; print the feed. |
| `--full` | Include dirty/merged checks in the feed. |
| `--fast` | Skip expensive checks (default). |
| `--format <text\|json\|shell>` | Output format for the selected item. |

**Examples**

```bash
# Interactive fuzzy switch.
gat search

# Pre-seed the fzf query.
gat search login

# Emit the feed for scripting.
gat search --print | awk -F '\t' '$2 ~ /dirty/ { print $1 }'

# Open the selection directly in tmux.
gat search --tmux
```

**Related**

- [list](#list) for the full status table.
- [switch](#switch)/[tmux](#tmux) for session handling.

---

### tmux

Create or reuse a worktree and open an AI-ready tmux session for it.

**Synopsis**

```
gat tmux <ticket> [options]
gat session <ticket> [options]
gat start <ticket> [options]
```

**Description**

Creates the worktree if needed (like [new](#new)), then creates a three-pane
tmux session:

- Left pane: the AI command (`codex` by default), started in the worktree.
- Right-top pane: the editor (`nvim` by default) opened on the worktree root.
- Right-bottom pane: a shell in the worktree.

Pane proportions come from the active layout (preset or explicit geometry, see
[config](#config)). If `gat` is already inside tmux, it switches the client
rather than nesting. The session name folds in the worktree description, and
gat records `@gat_path`, `@gat_branch`, and `@gat_description` on the session.

A prompt draft file is created outside the repository under
`${XDG_CONFIG_HOME:-~/.config}/gat/worktrees/<repo-key>/<ticket>/pre-prompt.md`
and exported to panes as `GAT_PROMPT_FILE`.

**Options**

| Option | Description |
|--------|-------------|
| `--session <name>` | Override the tmux session name. |
| `--prompt-file <path>` | Override the prompt draft path. |
| `--codex-cmd <cmd>` | Command for the AI pane (default `codex`). |
| `--editor-cmd <cmd>` | Editor for the prompt pane (default `nvim`). |
| `--layout <preset>` | Layout preset: `classic`, `ai-focus`, `editor-focus`, `wide`. |
| `--no-attach` | Create/reuse without attaching. |
| `-n, --dry-run` | Print the tmux plan only (does not create the worktree). |
| `--format <text\|json\|shell>` | Output format. |
| `--prefix <prefix>` / `--no-prefix` | Ticket prefix handling. |

**Examples**

```bash
gat tmux 12345
gat tmux 12345 --layout ai-focus
gat tmux 12345 --codex-cmd "aider" --editor-cmd "code -w"
gat tmux 12345 --dry-run --format json
```

**Related**

- [switch](#switch) which attaches without creating a worktree.
- [sessions](#sessions) to list sessions.

---

### dx

Enter or run a command in a worktree-scoped Docker container.

**Synopsis**

```
gat dx [--service <name>] [--doctor] [--] [command...]
gat docker [...]
```

**Description**

Finds a running container whose bind mount source matches the current worktree
root and `exec`s into it; if none is running, falls back to
`docker compose run --rm <service>` from the worktree's compose directory. With
no command, runs `bash`. Service selection uses `--service`, then
`GAT_DOCKER_SERVICE`, then `gat.dockerService`, then the `dp1` default, then the
only runnable declared service. `--doctor` prints a diagnosis of what `dx`
would do without running anything.

**Options**

| Option | Description |
|--------|-------------|
| `--service <name>` | Compose service to exec into or run. |
| `--doctor` | Print Docker/worktree diagnostics instead of running. |
| `--` | Treat everything after as the command to run. |

**Examples**

```bash
# Shell into the worktree's container.
gat dx

# Run a specific command.
gat dx -- npm test

# Target a service.
gat dx --service web -- bash

# Diagnose resolution.
gat dx --doctor
```

**Related**

- Docker config keys under [config](#config).

---

### rm

Remove a linked worktree after safety checks.

**Synopsis**

```
gat rm <ticket> [options]
gat remove <ticket> [options]
gat delete <ticket> [options]
```

**Description**

Removes a worktree via native Git. Refuses to remove the primary worktree, a
locked worktree (without `--force`), or a dirty worktree (without `--force`).
Optionally deletes the branch afterward. Cleans up the worktree's usage metadata
entry. By default prompts for confirmation.

**Options**

| Option | Description |
|--------|-------------|
| `-f, --force` | Remove a dirty or locked worktree. |
| `-y, --yes` | Skip the confirmation prompt. |
| `--delete-branch` | Delete the branch after removal (must be merged). |
| `--force-delete-branch` | Force-delete the branch (implies `--delete-branch`). |
| `-n, --dry-run` | Show the planned removal only. |
| `--prefix <prefix>` / `--no-prefix` | Ticket prefix handling. |
| `--format <text\|json>` | Output format. |

**Examples**

```bash
gat rm 12345
gat rm 12345 --yes --delete-branch
gat rm 12345 --force --yes        # remove even if dirty
gat delete 12345 --dry-run
```

**Related**

- [archive](#archive) to preserve the worktree instead of deleting.
- [merge](#merge) `--cleanup` to merge then remove.

---

### archive

Move a worktree to an archive directory using `git worktree move`.

**Synopsis**

```
gat archive <ticket> [options]
```

**Description**

Relocates a worktree (preserving it as a valid Git worktree) into an archive
root, by default `../<repo-name>-archive/`. Refuses to archive the primary
worktree, and refuses a dirty worktree without `--force`. Validates that the
archive destination is writable, and updates the usage metadata to the new path.

**Options**

| Option | Description |
|--------|-------------|
| `--archive-dir <path>` | Archive root (default `../<repo>-archive`). |
| `-f, --force` | Archive a dirty worktree. |
| `-y, --yes` | Skip the confirmation prompt. |
| `-n, --dry-run` | Show the planned archive move only. |
| `--prefix <prefix>` / `--no-prefix` | Ticket prefix handling. |
| `--format <text\|json>` | Output format. |

**Examples**

```bash
gat archive 12345
gat archive 12345 --yes --archive-dir ~/archives
gat archive 12345 --dry-run
```

**Related**

- [rm](#rm) to delete instead of archive.

---

### prune

Prune stale Git worktree metadata and optionally remove worktrees.

**Synopsis**

```
gat prune [--merged] [--older-than <days>] [options]
```

**Description**

Always runs `git worktree prune` to clear stale administrative metadata. With
`--merged`, additionally removes clean worktrees whose branches are merged into
the default branch. With `--older-than <days>`, removes worktrees not accessed in
that many days (skipping dirty ones unless `--force`). Both selectors can be
combined.

**Options**

| Option | Description |
|--------|-------------|
| `--merged` | Remove merged, clean worktrees. |
| `--older-than <days>` | Remove worktrees unused for N days. |
| `-f, --force` | Include dirty worktrees when pruning by age. |
| `-y, --yes` | Skip confirmation prompts. |
| `-n, --dry-run` | Show planned pruning only. |
| `--format <text\|json>` | Output format. |

**Examples**

```bash
gat prune
gat prune --merged --yes
gat prune --older-than 30 --dry-run
gat prune --merged --older-than 60 --yes
```

**Related**

- [rm](#rm) for removing a single worktree.
- [doctor](#doctor) which warns about stale worktrees.

---

### shell-init

Emit shell integration code.

**Synopsis**

```
gat shell-init [--shell <bash|zsh|fish>]
```

**Description**

Prints a shell function that wraps `gat` so commands which resolve a path
(`new`, `go`, `search`) can change the parent shell's directory. A child process
cannot `cd` its parent, so the wrapper evaluates structured shell output. Add the
output to your shell rc.

**Options**

| Option | Description |
|--------|-------------|
| `--shell <bash\|zsh\|fish>` | Shell dialect (default `bash`). |

**Examples**

```bash
# bash/zsh: add to ~/.bashrc or ~/.zshrc
eval "$(gat shell-init --shell bash)"

# fish: add to config.fish
gat shell-init --shell fish | source
```

**Related**

- [doctor](#doctor) reports whether integration is active.

---

### doctor

Inspect local setup, dependencies, and stale worktrees.

**Synopsis**

```
gat doctor [--format <text|json>]
```

**Description**

Reports the Git version, repository roots, ticket prefix, whether shell
integration is active, and the availability of `docker`, `tmux`, `codex`,
`nvim`, and `fzf`. It also warns about worktrees not accessed in over 30 days and
prunes orphaned metadata entries as a side effect.

**Options**

| Option | Description |
|--------|-------------|
| `--format <text\|json>` | Output format. |

**Examples**

```bash
gat doctor
gat doctor --json
```

**Related**

- [prune](#prune) to act on the stale-worktree warnings.

---

### help / --version

```
gat help
gat -h
gat --help
gat --version
gat -V
gat version
```

`help` (and no arguments) prints the global usage summary. `--version` prints the
program version.
