//! Interactive terminal dashboard for `gat` (the `tui` feature).
//!
//! The module is split into two layers:
//!
//! * A pure state machine ([`UiState`]) that owns selection, the active tab,
//!   the incremental filter, and any transient modal input. It has no terminal
//!   dependencies, so it is fully unit-testable.
//! * A thin driver ([`run`]) that wires the state machine to `crossterm` input
//!   and `ratatui` rendering, and dispatches actions back into the application
//!   layer.
//!
//! Keeping the state machine pure means navigation and filtering are verified
//! by ordinary tests, while the terminal plumbing stays minimal.

use crate::app::UiSnapshot;
use crate::error::Result;

/// A key event reduced to the subset the dashboard understands.
///
/// Using our own enum (rather than `crossterm`'s) keeps the state machine
/// independent of the input backend and trivially testable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Key {
    /// A printable character.
    Char(char),
    /// Enter / Return.
    Enter,
    /// Escape.
    Esc,
    /// Backspace.
    Backspace,
    /// Up arrow.
    Up,
    /// Down arrow.
    Down,
    /// Tab.
    Tab,
}

/// Which list the dashboard is focused on.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Tab {
    /// The worktree list.
    Worktrees,
    /// The live tmux session list.
    Sessions,
}

impl Tab {
    /// Returns the other tab.
    fn toggled(self) -> Self {
        match self {
            Tab::Worktrees => Tab::Sessions,
            Tab::Sessions => Tab::Worktrees,
        }
    }
}

/// A transient modal interaction layered over the main view.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Mode {
    /// Normal navigation.
    Normal,
    /// Incremental filter input; holds the current query.
    Filter(String),
    /// Editing the selected worktree's description; holds the draft text.
    Describe(String),
    /// Confirming removal of the selected worktree; holds its branch label.
    ConfirmRemove(String),
}

/// An action the driver should perform after handling a key.
///
/// Returning an explicit action keeps the state machine free of terminal or
/// process side effects, which is what makes it testable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Action {
    /// Nothing to do; keep looping.
    None,
    /// Quit the dashboard.
    Quit,
    /// Re-read repository state into a fresh snapshot.
    Refresh,
    /// Switch to (or open) the tmux session for the given target.
    Switch(String),
    /// Persist a description for the given target.
    SetDescription { target: String, description: String },
    /// Remove the worktree for the given target.
    Remove(String),
}

/// Pure dashboard state shared by the driver and the tests.
pub struct UiState {
    /// Latest repository snapshot.
    snapshot: UiSnapshot,
    /// Active tab.
    tab: Tab,
    /// Selected index within the active tab's filtered rows.
    selected: usize,
    /// Current interaction mode.
    mode: Mode,
    /// Committed filter query that persists outside filter-input mode.
    committed_filter: String,
    /// Last status/result message shown in the footer.
    status: String,
}

/// Default footer hint shown in normal mode.
const HELP_HINT: &str =
    "j/k move  Tab switch list  Enter open  d describe  x remove  / filter  r refresh  q quit";

impl UiState {
    /// Builds a new state from an initial snapshot.
    pub fn new(snapshot: UiSnapshot) -> Self {
        Self {
            snapshot,
            tab: Tab::Worktrees,
            selected: 0,
            mode: Mode::Normal,
            committed_filter: String::new(),
            status: HELP_HINT.to_string(),
        }
    }

    /// Replaces the snapshot (after a refresh) and clamps the selection.
    pub fn set_snapshot(&mut self, snapshot: UiSnapshot) {
        self.snapshot = snapshot;
        self.clamp_selection();
    }

    /// Returns the active tab.
    pub fn tab(&self) -> Tab {
        self.tab
    }

    /// Returns the current mode.
    pub fn mode(&self) -> &Mode {
        &self.mode
    }

    /// Returns the footer status text.
    pub fn status(&self) -> &str {
        &self.status
    }

    /// Sets the footer status text.
    pub fn set_status(&mut self, status: impl Into<String>) {
        self.status = status.into();
    }

    /// Returns the underlying snapshot.
    pub fn snapshot(&self) -> &UiSnapshot {
        &self.snapshot
    }

    /// Returns the current selection index (within filtered rows).
    pub fn selected(&self) -> usize {
        self.selected
    }

    /// Returns the query used for filtering rows.
    ///
    /// While typing a filter the live query is used; otherwise the committed
    /// query persists so the filtered view stays put after pressing Enter.
    fn active_query(&self) -> &str {
        match &self.mode {
            Mode::Filter(q) => q.as_str(),
            _ => &self.committed_filter,
        }
    }

    /// Returns the indices of worktree rows matching the active filter.
    ///
    /// Matching is case-insensitive across branch, description, and path.
    pub fn filtered_worktrees(&self) -> Vec<usize> {
        let query = self.active_query().to_lowercase();
        self.snapshot
            .worktrees
            .iter()
            .enumerate()
            .filter(|(_, w)| {
                query.is_empty()
                    || w.branch.to_lowercase().contains(&query)
                    || w.path.to_lowercase().contains(&query)
                    || w.description
                        .as_deref()
                        .map(|d| d.to_lowercase().contains(&query))
                        .unwrap_or(false)
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// Returns the indices of session rows matching the active filter.
    pub fn filtered_sessions(&self) -> Vec<usize> {
        let query = self.active_query().to_lowercase();
        self.snapshot
            .sessions
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                query.is_empty()
                    || s.name.to_lowercase().contains(&query)
                    || s.branch
                        .as_deref()
                        .map(|b| b.to_lowercase().contains(&query))
                        .unwrap_or(false)
                    || s.description
                        .as_deref()
                        .map(|d| d.to_lowercase().contains(&query))
                        .unwrap_or(false)
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// Returns the number of rows currently visible in the active tab.
    pub fn visible_len(&self) -> usize {
        match self.tab {
            Tab::Worktrees => self.filtered_worktrees().len(),
            Tab::Sessions => self.filtered_sessions().len(),
        }
    }

    /// The target (branch/ticket) of the currently selected worktree, if any.
    pub fn selected_worktree_target(&self) -> Option<String> {
        let indices = self.filtered_worktrees();
        let idx = *indices.get(self.selected)?;
        self.snapshot.worktrees.get(idx).map(|w| w.branch.clone())
    }

    /// The selected worktree's current description, if any.
    fn selected_worktree_description(&self) -> Option<String> {
        let indices = self.filtered_worktrees();
        let idx = *indices.get(self.selected)?;
        self.snapshot
            .worktrees
            .get(idx)
            .and_then(|w| w.description.clone())
    }

    /// The selected session's branch (preferred) or name, if any.
    pub fn selected_session_target(&self) -> Option<String> {
        let indices = self.filtered_sessions();
        let idx = *indices.get(self.selected)?;
        let session = self.snapshot.sessions.get(idx)?;
        Some(
            session
                .branch
                .clone()
                .unwrap_or_else(|| session.name.clone()),
        )
    }

    /// The target for the active tab's current selection.
    fn selected_target(&self) -> Option<String> {
        match self.tab {
            Tab::Worktrees => self.selected_worktree_target(),
            Tab::Sessions => self.selected_session_target(),
        }
    }

    /// Moves the selection down by one, saturating at the last row.
    fn select_next(&mut self) {
        let len = self.visible_len();
        if len > 0 && self.selected + 1 < len {
            self.selected += 1;
        }
    }

    /// Moves the selection up by one, saturating at the first row.
    fn select_prev(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    /// Clamps the selection into the valid range for the active tab.
    fn clamp_selection(&mut self) {
        let len = self.visible_len();
        if len == 0 {
            self.selected = 0;
        } else if self.selected >= len {
            self.selected = len - 1;
        }
    }

    /// Handles a key, mutating state and returning an [`Action`] for the driver.
    ///
    /// All input routing lives here so the behavior is testable without a
    /// terminal.
    pub fn handle_key(&mut self, key: Key) -> Action {
        match self.mode.clone() {
            Mode::Normal => self.handle_normal(key),
            Mode::Filter(query) => self.handle_filter(key, query),
            Mode::Describe(draft) => self.handle_describe(key, draft),
            Mode::ConfirmRemove(target) => self.handle_confirm_remove(key, target),
        }
    }

    /// Handles a key in normal navigation mode.
    fn handle_normal(&mut self, key: Key) -> Action {
        match key {
            Key::Char('q') | Key::Esc => Action::Quit,
            Key::Char('j') | Key::Down => {
                self.select_next();
                Action::None
            }
            Key::Char('k') | Key::Up => {
                self.select_prev();
                Action::None
            }
            Key::Tab => {
                self.tab = self.tab.toggled();
                self.clamp_selection();
                Action::None
            }
            Key::Char('r') => Action::Refresh,
            Key::Char('/') => {
                self.mode = Mode::Filter(self.committed_filter.clone());
                Action::None
            }
            Key::Enter => match self.selected_target() {
                Some(target) => Action::Switch(target),
                None => {
                    self.set_status("nothing selected");
                    Action::None
                }
            },
            Key::Char('d') => {
                // Describe only applies to worktrees.
                if self.tab == Tab::Worktrees {
                    if self.selected_worktree_target().is_some() {
                        let draft = self.selected_worktree_description().unwrap_or_default();
                        self.mode = Mode::Describe(draft);
                    } else {
                        self.set_status("no worktree selected");
                    }
                } else {
                    self.set_status("describe applies to worktrees");
                }
                Action::None
            }
            Key::Char('x') => {
                if self.tab == Tab::Worktrees {
                    if let Some(target) = self.selected_worktree_target() {
                        self.mode = Mode::ConfirmRemove(target);
                    } else {
                        self.set_status("no worktree selected");
                    }
                } else {
                    self.set_status("remove applies to worktrees");
                }
                Action::None
            }
            _ => Action::None,
        }
    }

    /// Handles a key while typing an incremental filter.
    fn handle_filter(&mut self, key: Key, mut query: String) -> Action {
        match key {
            Key::Esc => {
                // Cancel: drop the in-progress query, keep the committed one.
                self.mode = Mode::Normal;
            }
            Key::Enter => {
                // Commit the query and return to navigation.
                self.committed_filter = query;
                self.mode = Mode::Normal;
                self.clamp_selection();
            }
            Key::Backspace => {
                query.pop();
                self.mode = Mode::Filter(query);
                self.selected = 0;
            }
            Key::Char(c) => {
                query.push(c);
                self.mode = Mode::Filter(query);
                self.selected = 0;
            }
            _ => {}
        }
        Action::None
    }

    /// Handles a key while editing a description.
    fn handle_describe(&mut self, key: Key, mut draft: String) -> Action {
        match key {
            Key::Esc => {
                self.mode = Mode::Normal;
                Action::None
            }
            Key::Enter => {
                self.mode = Mode::Normal;
                match self.selected_worktree_target() {
                    Some(target) => Action::SetDescription {
                        target,
                        description: draft,
                    },
                    None => Action::None,
                }
            }
            Key::Backspace => {
                draft.pop();
                self.mode = Mode::Describe(draft);
                Action::None
            }
            Key::Char(c) => {
                draft.push(c);
                self.mode = Mode::Describe(draft);
                Action::None
            }
            _ => Action::None,
        }
    }

    /// Handles a key while confirming a worktree removal.
    fn handle_confirm_remove(&mut self, key: Key, target: String) -> Action {
        match key {
            Key::Char('y') | Key::Char('Y') => {
                self.mode = Mode::Normal;
                Action::Remove(target)
            }
            _ => {
                self.mode = Mode::Normal;
                self.set_status("removal cancelled");
                Action::None
            }
        }
    }
}

/// Launches the interactive dashboard. Returns when the user quits.
///
/// This delegates to the terminal driver, which is the only part that touches
/// a TTY; all decision logic lives in [`UiState`].
pub fn run(fast: bool) -> Result<String> {
    driver::run(fast)
}

/// Terminal driver: crossterm input + ratatui rendering.
///
/// Isolated in its own module so the pure state machine above stays free of any
/// terminal dependencies.
mod driver {
    use super::{Action, Key, Mode, Tab, UiState};
    use crate::app;
    use crate::error::{GatError, Result};
    use crossterm::event::{self, Event, KeyCode, KeyEventKind};
    use crossterm::terminal::{
        disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
    };
    use crossterm::ExecutableCommand;
    use ratatui::backend::CrosstermBackend;
    use ratatui::layout::{Constraint, Direction, Layout, Rect};
    use ratatui::style::{Color, Modifier, Style};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Tabs};
    use ratatui::{Frame, Terminal};
    use std::io::{self, Stdout};
    use std::time::Duration;

    /// Maps a crossterm key code to our backend-independent [`Key`].
    fn map_key(code: KeyCode) -> Option<Key> {
        match code {
            KeyCode::Char(c) => Some(Key::Char(c)),
            KeyCode::Enter => Some(Key::Enter),
            KeyCode::Esc => Some(Key::Esc),
            KeyCode::Backspace => Some(Key::Backspace),
            KeyCode::Up => Some(Key::Up),
            KeyCode::Down => Some(Key::Down),
            KeyCode::Tab => Some(Key::Tab),
            _ => None,
        }
    }

    /// RAII guard that restores the terminal even if a panic unwinds.
    struct TerminalGuard;

    impl TerminalGuard {
        /// Enters raw mode and the alternate screen.
        fn enter() -> Result<Self> {
            enable_raw_mode()
                .map_err(|e| GatError::Io(format!("failed to enable raw mode: {e}")))?;
            io::stdout()
                .execute(EnterAlternateScreen)
                .map_err(|e| GatError::Io(format!("failed to enter alternate screen: {e}")))?;
            Ok(Self)
        }
    }

    impl Drop for TerminalGuard {
        fn drop(&mut self) {
            // Best-effort restore; nothing useful to do on failure during drop.
            let _ = disable_raw_mode();
            let _ = io::stdout().execute(LeaveAlternateScreen);
        }
    }

    /// Runs the dashboard event loop.
    pub fn run(fast: bool) -> Result<String> {
        let snapshot = app::ui_snapshot(fast)?;
        let mut state = UiState::new(snapshot);

        let _guard = TerminalGuard::enter()?;
        let backend = CrosstermBackend::new(io::stdout());
        let mut terminal = Terminal::new(backend)
            .map_err(|e| GatError::Io(format!("failed to initialize terminal: {e}")))?;

        // Guard restores the terminal on drop; the loop's result propagates.
        event_loop(&mut terminal, &mut state, fast)
    }

    /// The main draw/handle loop.
    fn event_loop(
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
        state: &mut UiState,
        fast: bool,
    ) -> Result<String> {
        loop {
            terminal
                .draw(|frame| render(frame, state))
                .map_err(|e| GatError::Io(format!("failed to draw frame: {e}")))?;

            // Poll so the loop can stay responsive; ignore non-key events.
            if !event::poll(Duration::from_millis(250))
                .map_err(|e| GatError::Io(format!("failed to poll input: {e}")))?
            {
                continue;
            }

            let Event::Key(key_event) =
                event::read().map_err(|e| GatError::Io(format!("failed to read input: {e}")))?
            else {
                continue;
            };

            // Ignore key-release events (Windows reports both press and release).
            if key_event.kind == KeyEventKind::Release {
                continue;
            }

            let Some(key) = map_key(key_event.code) else {
                continue;
            };

            match state.handle_key(key) {
                Action::None => {}
                Action::Quit => return Ok(String::new()),
                Action::Refresh => match app::ui_snapshot(fast) {
                    Ok(snapshot) => {
                        state.set_snapshot(snapshot);
                        state.set_status("refreshed");
                    }
                    Err(e) => state.set_status(format!("refresh failed: {e}")),
                },
                Action::Switch(target) => {
                    suspend_and_switch(terminal, state, &target, fast)?;
                }
                Action::SetDescription {
                    target,
                    description,
                } => match app::ui_set_description(&target, &description) {
                    Ok(_) => {
                        refresh_quietly(state, fast);
                        state.set_status(format!("described {target}"));
                    }
                    Err(e) => state.set_status(format!("describe failed: {e}")),
                },
                Action::Remove(target) => match app::ui_remove(&target) {
                    Ok(_) => {
                        refresh_quietly(state, fast);
                        state.set_status(format!("removed {target}"));
                    }
                    Err(e) => state.set_status(format!("remove failed: {e}")),
                },
            }
        }
    }

    /// Refreshes the snapshot, leaving a status message only on failure.
    fn refresh_quietly(state: &mut UiState, fast: bool) {
        match app::ui_snapshot(fast) {
            Ok(snapshot) => state.set_snapshot(snapshot),
            Err(e) => state.set_status(format!("refresh failed: {e}")),
        }
    }

    /// Suspends the TUI, switches to the target's tmux session, then resumes.
    ///
    /// Attaching to tmux needs the real terminal, so we leave the alternate
    /// screen and raw mode for the duration of the switch and restore them
    /// afterward. When gat runs inside tmux, the switch changes the client and
    /// returns immediately.
    fn suspend_and_switch(
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
        state: &mut UiState,
        target: &str,
        fast: bool,
    ) -> Result<()> {
        let _ = disable_raw_mode();
        let _ = io::stdout().execute(LeaveAlternateScreen);

        let outcome = app::ui_switch(target);

        enable_raw_mode()
            .map_err(|e| GatError::Io(format!("failed to re-enable raw mode: {e}")))?;
        io::stdout()
            .execute(EnterAlternateScreen)
            .map_err(|e| GatError::Io(format!("failed to re-enter alternate screen: {e}")))?;
        terminal
            .clear()
            .map_err(|e| GatError::Io(format!("failed to clear terminal: {e}")))?;

        match outcome {
            Ok(_) => {
                refresh_quietly(state, fast);
                state.set_status(format!("switched to {target}"));
            }
            Err(e) => state.set_status(format!("switch failed: {e}")),
        }
        Ok(())
    }

    /// Renders the whole dashboard for one frame.
    fn render(frame: &mut Frame, state: &UiState) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // header / tabs
                Constraint::Min(3),    // body list
                Constraint::Length(3), // footer / input
            ])
            .split(frame.area());

        render_tabs(frame, chunks[0], state);
        render_body(frame, chunks[1], state);
        render_footer(frame, chunks[2], state);
    }

    /// Renders the tab bar with the repo name as the title.
    fn render_tabs(frame: &mut Frame, area: Rect, state: &UiState) {
        let titles = vec![
            Line::from(format!("Worktrees ({})", state.snapshot().worktrees.len())),
            Line::from(format!("Sessions ({})", state.snapshot().sessions.len())),
        ];
        let selected = match state.tab() {
            Tab::Worktrees => 0,
            Tab::Sessions => 1,
        };
        let tabs = Tabs::new(titles)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!(" gat: {} ", state.snapshot().repo_name)),
            )
            .select(selected)
            .highlight_style(Style::default().fg(Color::Black).bg(Color::Cyan));
        frame.render_widget(tabs, area);
    }

    /// Renders the active list (worktrees or sessions).
    fn render_body(frame: &mut Frame, area: Rect, state: &UiState) {
        let (items, title): (Vec<ListItem>, &str) = match state.tab() {
            Tab::Worktrees => (
                state
                    .filtered_worktrees()
                    .into_iter()
                    .filter_map(|i| state.snapshot().worktrees.get(i))
                    .map(worktree_item)
                    .collect(),
                " Worktrees ",
            ),
            Tab::Sessions => (
                state
                    .filtered_sessions()
                    .into_iter()
                    .filter_map(|i| state.snapshot().sessions.get(i))
                    .map(session_item)
                    .collect(),
                " Sessions ",
            ),
        };

        let mut list_state = ListState::default();
        if state.visible_len() > 0 {
            list_state.select(Some(state.selected()));
        }

        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title(title))
            .highlight_style(
                Style::default()
                    .bg(Color::Blue)
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("> ");
        frame.render_stateful_widget(list, area, &mut list_state);
    }

    /// Builds a list item for a worktree row.
    fn worktree_item(w: &app::UiWorktreeRow) -> ListItem<'static> {
        let mut tags = Vec::new();
        if w.is_primary {
            tags.push("primary");
        }
        if w.dirty {
            tags.push("dirty");
        }
        if w.merged {
            tags.push("merged");
        }
        let state = if tags.is_empty() {
            "clean".to_string()
        } else {
            tags.join(",")
        };
        let changes = if w.changed_files == 0 {
            "-".to_string()
        } else {
            format!("{}f +{} -{}", w.changed_files, w.insertions, w.deletions)
        };
        let idle = w
            .idle_days
            .map(|d| format!("{d}d"))
            .unwrap_or_else(|| "-".to_string());
        let desc = w.description.clone().unwrap_or_default();

        let header = Line::from(vec![
            Span::styled(
                format!("{:<28}", truncate(&w.branch, 28)),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!(" {state:<14}"), Style::default().fg(Color::Yellow)),
            Span::styled(format!(" {changes:<12}"), Style::default().fg(Color::Green)),
            Span::styled(format!(" {idle:<5}"), Style::default().fg(Color::DarkGray)),
        ]);
        let detail = Line::from(vec![Span::styled(
            format!("  {desc}"),
            Style::default().fg(Color::Gray),
        )]);
        ListItem::new(vec![header, detail])
    }

    /// Builds a list item for a session row.
    fn session_item(s: &app::UiSessionRow) -> ListItem<'static> {
        let attached = if s.attached { "*" } else { " " };
        let branch = s.branch.clone().unwrap_or_else(|| "-".to_string());
        let info = s.description.clone().unwrap_or_default();
        let line = Line::from(vec![
            Span::raw(format!("{attached} ")),
            Span::styled(
                format!("{:<36}", truncate(&s.name, 36)),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" {:>3}win", s.windows),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(format!("  {branch:<15}"), Style::default().fg(Color::Cyan)),
            Span::styled(format!(" {info}"), Style::default().fg(Color::Gray)),
        ]);
        ListItem::new(line)
    }

    /// Renders the footer: either the mode's input line or the status hint.
    fn render_footer(frame: &mut Frame, area: Rect, state: &UiState) {
        let (label, body, style) = match state.mode() {
            Mode::Filter(q) => (
                " filter ",
                format!("/{q}"),
                Style::default().fg(Color::Yellow),
            ),
            Mode::Describe(d) => (
                " describe ",
                format!("{d}_"),
                Style::default().fg(Color::Cyan),
            ),
            Mode::ConfirmRemove(target) => (
                " confirm ",
                format!("remove {target}? (y/N)"),
                Style::default().fg(Color::Red),
            ),
            Mode::Normal => (
                " status ",
                state.status().to_string(),
                Style::default().fg(Color::DarkGray),
            ),
        };
        let paragraph = Paragraph::new(body)
            .style(style)
            .block(Block::default().borders(Borders::ALL).title(label));
        frame.render_widget(paragraph, area);
    }

    /// Truncates a string to `max` characters, appending a marker when cut.
    fn truncate(value: &str, max: usize) -> String {
        if value.chars().count() <= max {
            value.to_string()
        } else {
            let mut out: String = value.chars().take(max.saturating_sub(1)).collect();
            out.push('~');
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{UiSessionRow, UiSnapshot, UiWorktreeRow};

    fn wt(branch: &str, desc: Option<&str>) -> UiWorktreeRow {
        UiWorktreeRow {
            branch: branch.to_string(),
            path: format!("/repo/{branch}"),
            is_primary: false,
            dirty: false,
            merged: false,
            changed_files: 0,
            insertions: 0,
            deletions: 0,
            idle_days: Some(0),
            description: desc.map(str::to_string),
        }
    }

    fn session(name: &str, branch: Option<&str>) -> UiSessionRow {
        UiSessionRow {
            name: name.to_string(),
            attached: false,
            windows: 1,
            branch: branch.map(str::to_string),
            description: None,
        }
    }

    fn snapshot() -> UiSnapshot {
        UiSnapshot {
            repo_name: "repo".to_string(),
            worktrees: vec![
                wt("TICKET-1", Some("fix login")),
                wt("TICKET-2", Some("add cache")),
                wt("FEAT-9", None),
            ],
            sessions: vec![
                session("gat-TICKET-1", Some("TICKET-1")),
                session("gat-TICKET-2", Some("TICKET-2")),
            ],
        }
    }

    #[test]
    fn navigation_moves_within_bounds() {
        let mut state = UiState::new(snapshot());
        assert_eq!(state.selected(), 0);
        state.handle_key(Key::Char('j'));
        assert_eq!(state.selected(), 1);
        state.handle_key(Key::Char('j'));
        state.handle_key(Key::Char('j')); // saturates at last (3 rows)
        assert_eq!(state.selected(), 2);
        state.handle_key(Key::Char('k'));
        assert_eq!(state.selected(), 1);
        state.handle_key(Key::Char('k'));
        state.handle_key(Key::Char('k')); // saturates at 0
        assert_eq!(state.selected(), 0);
    }

    #[test]
    fn tab_toggles_active_list() {
        let mut state = UiState::new(snapshot());
        assert_eq!(state.tab(), Tab::Worktrees);
        state.handle_key(Key::Tab);
        assert_eq!(state.tab(), Tab::Sessions);
        state.handle_key(Key::Tab);
        assert_eq!(state.tab(), Tab::Worktrees);
    }

    #[test]
    fn enter_returns_switch_action_with_selected_target() {
        let mut state = UiState::new(snapshot());
        state.handle_key(Key::Char('j')); // select TICKET-2
        let action = state.handle_key(Key::Enter);
        assert_eq!(action, Action::Switch("TICKET-2".to_string()));
    }

    #[test]
    fn enter_on_session_tab_uses_session_branch() {
        let mut state = UiState::new(snapshot());
        state.handle_key(Key::Tab); // sessions
        let action = state.handle_key(Key::Enter);
        assert_eq!(action, Action::Switch("TICKET-1".to_string()));
    }

    #[test]
    fn filter_narrows_rows_and_clamps_selection() {
        let mut state = UiState::new(snapshot());
        state.handle_key(Key::Char('/'));
        for c in "cache".chars() {
            state.handle_key(Key::Char(c));
        }
        // Only TICKET-2 (description "add cache") matches.
        assert_eq!(state.filtered_worktrees(), vec![1]);
        state.handle_key(Key::Enter); // commit filter
        assert_eq!(state.visible_len(), 1);
        assert_eq!(state.selected_worktree_target().as_deref(), Some("TICKET-2"));
    }

    #[test]
    fn filter_escape_cancels_without_committing() {
        let mut state = UiState::new(snapshot());
        state.handle_key(Key::Char('/'));
        state.handle_key(Key::Char('z'));
        state.handle_key(Key::Esc);
        // Filter cancelled: all rows visible again.
        assert_eq!(state.visible_len(), 3);
    }

    #[test]
    fn describe_flow_emits_set_description_action() {
        let mut state = UiState::new(snapshot());
        state.handle_key(Key::Char('d')); // start describe on TICKET-1
        assert!(matches!(state.mode(), Mode::Describe(_)));
        for c in "X".chars() {
            state.handle_key(Key::Char(c));
        }
        let action = state.handle_key(Key::Enter);
        match action {
            Action::SetDescription {
                target,
                description,
            } => {
                assert_eq!(target, "TICKET-1");
                // Draft started from the existing description "fix login".
                assert_eq!(description, "fix loginX");
            }
            other => panic!("expected SetDescription, got {other:?}"),
        }
    }

    #[test]
    fn remove_requires_confirmation() {
        let mut state = UiState::new(snapshot());
        state.handle_key(Key::Char('x'));
        assert!(matches!(state.mode(), Mode::ConfirmRemove(_)));
        // Pressing a non-yes key cancels.
        let cancelled = state.handle_key(Key::Char('n'));
        assert_eq!(cancelled, Action::None);

        state.handle_key(Key::Char('x'));
        let confirmed = state.handle_key(Key::Char('y'));
        assert_eq!(confirmed, Action::Remove("TICKET-1".to_string()));
    }

    #[test]
    fn quit_keys_emit_quit() {
        let mut state = UiState::new(snapshot());
        assert_eq!(state.handle_key(Key::Char('q')), Action::Quit);
        let mut state2 = UiState::new(snapshot());
        assert_eq!(state2.handle_key(Key::Esc), Action::Quit);
    }

    #[test]
    fn refresh_key_emits_refresh() {
        let mut state = UiState::new(snapshot());
        assert_eq!(state.handle_key(Key::Char('r')), Action::Refresh);
    }

    #[test]
    fn describe_blocked_on_session_tab() {
        let mut state = UiState::new(snapshot());
        state.handle_key(Key::Tab); // sessions
        let action = state.handle_key(Key::Char('d'));
        assert_eq!(action, Action::None);
        assert!(matches!(state.mode(), Mode::Normal));
    }
}
