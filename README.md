# gat

`gat` is a ticket-oriented Git worktree helper that layers a fast, opinionated
command surface over native `git worktree`, `tmux`, `fzf`, Docker Compose, and
shell integration — while keeping Git as the single source of truth.

## Quick start

```bash
gat 12345
```

From inside a Git repository, this creates or reuses a sibling worktree:

```
../<repo-name>-TICKET-12345
```

Numeric tickets are prefixed with `TICKET-` by default, so `gat 12345` maps to
branch `TICKET-12345` and worktree `../<repo-name>-TICKET-12345`.
Already-prefixed tickets such as `gat TICKET-12345` are preserved as-is.

## Features

### Worktree lifecycle

| Command | Description |
|---------|-------------|
| `gat <ticket>` / `gat new <ticket>` | Create or reuse a ticket worktree next to the repo |
| `gat go <ticket>` | Switch to an existing worktree (cd with shell integration) |
| `gat switch <ticket>` | Attach to (or open) the tmux session for an existing worktree |
| `gat rm <ticket>` / `gat delete <ticket>` | Remove a worktree with safety checks |
| `gat archive <ticket>` | Move a worktree to an archive directory (preserves it) |
| `gat merge <ticket>` | Merge a ticket branch into default, optionally clean up |
| `gat prune` | Prune stale git metadata and optionally remove merged/old worktrees |

### Inspection and navigation

| Command | Description |
|---------|-------------|
| `gat list` / `gat ls` | List all worktrees with status, change stats, idle time, descriptions |
| `gat path <ticket>` | Print a worktree's resolved path |
| `gat watch` | Continuously re-render the worktree list (live monitor) |
| `gat search` / `gat find` | Search worktrees interactively via `fzf` or emit a tab-separated feed |
| `gat sessions` | List live gat-managed tmux sessions with attach state and metadata |
| `gat doctor` | Inspect local setup, dependencies, and stale worktrees |

### Tmux session management

| Command | Description |
|---------|-------------|
| `gat tmux <ticket>` | Create worktree (if needed) and open a three-pane AI/editor/shell tmux session |
| `gat switch <ticket>` | Attach to an existing worktree's tmux session (does not create worktrees) |
| Layout presets | `classic` (55/35), `ai-focus` (70/40), `editor-focus` (35/25), `wide` (50/50) |
| Custom geometry | `--layout`, `--codex-cmd`, `--editor-cmd`, explicit width/height overrides |

### Docker integration

| Command | Description |
|---------|-------------|
| `gat dx` / `gat docker` | Exec into a running container matching the current worktree, or `docker compose run` |
| `gat dx --doctor` | Diagnose worktree/container resolution without running anything |
| `gat dx --service <name>` | Target a specific compose service |

### Descriptions and metadata

```bash
gat 12345 --description "fix login redirect"   # set at creation
gat describe 12345 add OAuth support            # set or update later
gat describe 12345                              # show current
gat describe 12345 --clear                      # remove
```

Descriptions appear in `list`, `sessions`, `search`, and are folded into tmux
session names (truncated to 100 chars). Gat records `@gat_path`, `@gat_branch`,
and `@gat_description` as tmux session options for tooling.

### Interactive TUI

```bash
gat ui            # full interactive dashboard (requires --features tui)
gat ui --fast     # skip expensive checks for instant startup
```

A keyboard-driven dashboard with Worktrees and Sessions tabs showing change
stats, idle time, and descriptions. Navigate, filter, switch sessions, edit
descriptions, and remove worktrees without leaving the keyboard.

| Key | Action |
|-----|--------|
| `j`/`k` or arrows | Move selection |
| `Tab` | Toggle Worktrees / Sessions list |
| `Enter` | Switch to (or open) the selected session |
| `d` | Describe the selected worktree |
| `x` | Remove the selected worktree (asks `y/N`) |
| `/` | Incremental filter (`Enter` commits, `Esc` cancels) |
| `r` | Refresh |
| `q`/`Esc` | Quit |

Build with: `cargo install --path . --features tui`

### Configuration

`gat config` manages `${XDG_CONFIG_HOME:-~/.config}/gat/config.toml`:

```bash
gat config init                  # write a default config file
gat config path                  # print the config file path
gat config list                  # show all keys and effective values
gat config get tmux.left_width   # read one key
gat config set ticket_prefix ABC
gat config set tmux.layout ai-focus
```

Settings are merged with this precedence (highest wins):
1. CLI flags (`--layout`, `--prefix`, ...)
2. Environment variables (`GAT_*`)
3. Repository/global Git config (`gat.*` keys)
4. Config file (`config.toml`)
5. Built-in defaults

#### Config keys

| Key | Type | Notes |
|-----|------|-------|
| `ticket_prefix` | string | Default prefix for numeric tickets |
| `docker_compose_dir` | string | Compose directory relative to worktree |
| `docker_worktree_mount` | string | Container mount path |
| `docker_service` | string | Default compose service |
| `verbose` | bool | Enable verbose output |
| `tmux.layout` | preset | Write-only sugar: `classic`, `ai-focus`, `editor-focus`, `wide` |
| `tmux.left_width` | percent | 0-100 |
| `tmux.bottom_height` | percent | 0-100 |
| `tmux.shell` | string | Shell inside tmux panes |
| `tmux.codex_cmd` | string | AI pane command |
| `tmux.editor_cmd` | string | Editor pane command |
| `tmux.focus_left` | bool | Focus the left pane by default |

#### Git config keys (read-only overlay)

`gat.ticketPrefix`, `gat.dockerComposeDir`, `gat.dockerWorktreeMount`,
`gat.dockerService`, `gat.tmuxLayout`, `gat.tmuxShell`, `gat.tmuxCodexCmd`,
`gat.tmuxEditorCmd`, `gat.tmuxLeftWidth`, `gat.tmuxBottomHeight`.

### Worktree templates

Templates make fresh worktrees immediately usable by copying config files,
symlinking shared directories, and running setup commands:

```toml
[template.default]
copy = [".env.example", "config/local.json"]
symlink = ["node_modules", "target"]
run = ["npm install"]
```

```bash
gat new 12345                   # applies the "default" template
gat new 12345 --template rust   # applies a named template
gat new 12345 --no-template     # skip templating
```

Copy and symlink sources resolve relative to the primary worktree. Missing
sources are skipped with a warning; a failing `run` command aborts.

### Ticket prefixing

```bash
gat 12345                    # TICKET-12345 (default)
gat 12345 --prefix ABC       # ABC-12345
gat 12345 --no-prefix        # 12345 (raw)
git config gat.ticketPrefix ABC   # set repo default
export GAT_TICKET_PREFIX=ABC      # set session default
```

Non-numeric targets (`feature/login`, `TICKET-9`) are never modified.

### Shortcuts

`go`, `path`, and `switch` accept navigation shortcuts:

| Shortcut | Meaning |
|----------|---------|
| `^` | The default branch's worktree |
| `@` | The current worktree |

```bash
gat go ^          # jump to the default branch
gat path @        # print the current worktree path
```

`--base`/`--from` on `new` also accept `^` (default branch) and `@` (current branch).

### Listing and change stats

`gat list` shows each worktree with branch, state (primary/detached/dirty/merged/
locked/missing), short HEAD, a `Changes` column (`<files>f +<insertions> -<deletions>`),
an `Idle` column (days since last access), and the description. The `@` marker
indicates the current worktree.

```bash
gat list                 # human-readable table
gat list --json          # machine-readable JSON
gat list --fast          # skip expensive git status/diff calls
```

### Search

```bash
gat search               # interactive fzf selection
gat search login         # pre-seed the fzf query
gat search --print       # emit a tab-separated feed for scripting
gat search --tmux        # open the selected worktree in tmux
```

### Tmux session layout

`gat tmux 12345` creates or reuses the worktree, then sets up a three-pane
tmux session named `gat-TICKET-12345`:

- **Left pane** (55% width): AI command (`codex` by default), started in the worktree
- **Right-top pane** (65% of right column): Editor (`nvim` by default) on the worktree root
- **Right-bottom pane** (35% of right column): Shell in the worktree

#### Layout presets

| Preset | Left width | Bottom height | Focus |
|--------|-----------:|--------------:|-------|
| `classic` | 55% | 35% | AI |
| `ai-focus` | 70% | 40% | AI |
| `editor-focus` | 35% | 25% | editor |
| `wide` | 50% | 50% | AI |

```bash
gat tmux 12345 --layout ai-focus
git config gat.tmuxLayout ai-focus
export GAT_TMUX_LAYOUT=editor-focus
```

Explicit geometry (`--left-width`, `--bottom-height` or config/env equivalents)
overrides the preset baseline.

A prompt draft file is stored outside the repository at
`${XDG_CONFIG_HOME:-~/.config}/gat/worktrees/<repo-key>/<ticket>/pre-prompt.md`
and exported to panes as `GAT_PROMPT_FILE`. Pass `--prompt-file <path>` to
override the prompt draft location.

### Merge lifecycle

`gat merge <ticket>` merges a ticket branch into the default branch with strict
safety checks:

```bash
gat merge 12345                    # merge into default branch
gat merge 12345 --into develop     # merge into a specific branch
gat merge 12345 --cleanup --yes    # merge, then remove worktree + branch + session
gat merge 12345 --dry-run --json   # preview without changing anything
```

Safety guarantees: target worktree must exist, be non-primary, and be clean;
primary worktree must be clean and on the base branch; merge conflicts are
aborted. Cleanup (`--remove`, `--delete-branch`, `--kill-session`, or `--cleanup`
for all three) only runs after a successful merge.

### Docker access

```bash
gat dx              # shell into the worktree's container
gat dx -- npm test  # run a specific command
gat dx --doctor     # diagnose resolution without running
```

`gat dx` resolves the current worktree root, finds running containers whose bind
mount matches, and `docker exec -ti`s in. Falls back to `docker compose run --rm`
from `<worktree>/.docker/` if no container is running.

Service selection: `--service` > `GAT_DOCKER_SERVICE` > `gat.dockerService` >
`dp1` (when declared) > the only non-disabled declared service.

### Archive and delete

```bash
gat archive 12345                           # move to ../<repo>-archive/
gat archive 12345 --archive-dir ~/archives  # custom archive root
gat delete 12345 --yes --delete-branch      # remove worktree and branch
gat delete 12345 --force --yes              # remove even if dirty
```

Dirty worktrees are blocked unless `--force` is passed. Branch deletion is opt-in.

### Prune

```bash
gat prune                          # run git worktree prune
gat prune --merged --yes           # remove merged, clean worktrees
gat prune --older-than 30          # remove worktrees idle >30 days
gat prune --merged --older-than 60 --yes --dry-run  # preview combined
```

### Watch

```bash
gat watch                     # live monitor, 1s interval
gat watch --interval 2000     # 2s interval
gat watch --full              # include dirty/merged checks
gat watch --once              # render once and exit
```

### Progress output

Mutating commands print short progress lines to **stderr** (prefixed `gat:`),
keeping stdout clean for scripts and shell integration. Set `GAT_QUIET=1` to
silence them.

### Shell integration

A child process cannot `cd` its parent shell. Install the wrapper:

```bash
# bash / zsh
eval "$(gat shell-init --shell bash)"

# fish
gat shell-init --shell fish | source
```

With integration active, `gat 12345`, `gat go 12345`, and `gat search` can
change the parent shell's directory. Run `gat doctor` to confirm.

### Environment variables

| Variable | Effect |
|----------|--------|
| `GAT_TICKET_PREFIX` | Default prefix for numeric tickets |
| `GAT_TMUX_LAYOUT` | Layout preset |
| `GAT_TMUX_LEFT_WIDTH` | Left pane width (0-100) |
| `GAT_TMUX_BOTTOM_HEIGHT` | Bottom pane height (0-100) |
| `GAT_TMUX_SHELL` | Shell inside tmux panes |
| `GAT_TMUX_CODEX_CMD` | AI pane command |
| `GAT_TMUX_EDITOR_CMD` | Editor pane command |
| `GAT_DOCKER_COMPOSE_DIR` | Compose directory (default `.docker`) |
| `GAT_DOCKER_WORKTREE_MOUNT` | Container mount path |
| `GAT_DOCKER_SERVICE` | Default compose service |
| `GAT_QUIET` | Suppress progress output |
| `GAT_VERBOSE` | Enable verbose logging |
| `RUST_LOG` | Standard `env_logger` filter |
| `XDG_CONFIG_HOME` | Config root override |

### Exit codes

- `0` — success
- `1` — runtime or Git failure
- `2` — CLI usage error

### Output formats

Most commands accept `--format <text|json|shell>`:

- `text` (default): human-readable
- `json`: single JSON object or array for scripting (`jq`-compatible)
- `shell`: `KEY=value` assignments for `eval` by the shell integration

Shorthands: `--json` = `--format json`, `--shell` = `--format shell`.

### Complete command reference

```
gat <ticket> [--dry-run] [--prefix <p>] [--no-prefix] [--description <text>]
             [--branch <b>] [--base <ref>] [--path <p>] [--detach]
             [--template <name>] [--no-template] [--format <fmt>]
gat new <ticket>        [same options as above]  (alias: add)
gat go <ticket>         [--create] [--prefix <p>] [--no-prefix] [--format <fmt>]
gat switch <ticket>     [--session <name>] [--no-attach] [--dry-run]
                        [--codex-cmd <cmd>] [--editor-cmd <cmd>]
                        [--layout <preset>] [--prompt-file <path>] [--format <fmt>]
gat describe <ticket>   [text...] [--clear] [--format <fmt>]  (alias: desc)
gat sessions            [--format <fmt>]
gat ui                  [--fast]  (alias: dashboard)
gat config init         [--force]
gat config path
gat config list         [--format <fmt>]
gat config get <key>
gat config set <key> <value...>
gat merge <ticket>      [--into <branch>] [--no-ff] [--remove] [--delete-branch]
                        [--kill-session] [--cleanup] [--yes] [--dry-run]
                        [--prefix <p>] [--no-prefix] [--format <fmt>]
gat path <ticket>       [--prefix <p>] [--no-prefix]
gat list                [--format <fmt>] [--fast]  (alias: ls)
gat watch               [--interval <ms>] [--once] [--full] [--fast]
gat search [query]      [--print] [--path] [--tmux] [--no-fzf] [--full] [--fast]
                        (alias: find)
gat tmux <ticket>       [--session <name>] [--no-attach] [--dry-run]
                        [--codex-cmd <cmd>] [--editor-cmd <cmd>]
                        [--layout <preset>] [--prompt-file <path>] [--format <fmt>]
                        (aliases: session, start)
gat dx [--service <name>] [--doctor] [--] [command...]  (alias: docker)
gat rm <ticket>         [--yes] [--force] [--delete-branch] [--force-delete-branch]
                        [--dry-run] [--prefix <p>] [--no-prefix] [--format <fmt>]
                        (aliases: remove, delete)
gat archive <ticket>    [--yes] [--force] [--archive-dir <path>] [--dry-run]
                        [--prefix <p>] [--no-prefix] [--format <fmt>]
gat prune               [--merged] [--older-than <days>] [--yes] [--force] [--dry-run]
gat shell-init          [--shell <bash|zsh|fish>]
gat doctor              [--format <fmt>]
gat --version / gat --help / gat help
```

### Documentation

- `docs/COMMANDS.md` — complete command reference with every option, example,
  JSON field, and exit code.
- `docs/man/man1/` — man pages for `gat` and each subcommand. Install with
  `make install-man`, then `man gat`, `man gat-merge`, etc.

## Build and install

```bash
make verify          # format, test, clippy, rustdoc
make doc             # rebuild local source documentation
make install         # install to ~/.cargo/bin
```

Equivalent Cargo commands:

```bash
cargo build --release
cargo install --path .
cargo install --path . --features tui   # with TUI dashboard
```
