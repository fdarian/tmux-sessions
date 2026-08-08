mod create_session;
mod filter;
mod kill;
mod monitor;
mod move_window;
mod navigation;
mod persist;
mod pin_hide;
mod preview;
mod rename;
mod sessions;
mod util;

use std::collections::{HashMap, HashSet};
use std::io;

use ratatui::style::{Color, Style};
use ratatui::widgets::ListState;

use crate::config;
use crate::create::{CreateCandidate, CreateTab, WorktreeEntry, ZoxideEntry};
use crate::event::{Action, Mode};
use crate::history;
use crate::procs::{MonitorEntry, ProcessRow};
use crate::tmux;
use crate::tree::{self, DeadSessionRef, FlatEntry, NodeId};

pub use kill::ConfirmKillTarget;
pub use monitor::MonitorSort;
pub use move_window::MoveCandidate;
pub use preview::{CaptureRequest, PendingCaptureRequest, PreviewFullPane, PreviewPane};
pub use rename::RenameTarget;
pub use sessions::DeadSession;
pub use util::{create_match_result, current_unix_secs, extract_group_prefixes, extract_session_id, first_selectable_index, is_non_selectable, path_buf_to_string, recent_max_age_secs, resolve_shortcut_index};
pub use persist::{load_hidden, load_pins, save_hidden, save_pins};

pub struct App {
    pub config: Option<config::Config>,
    pub current_session_id: String,
    pub sessions: Vec<tmux::Session>,
    pub windows: Vec<tmux::Window>,
    pub panes: Vec<tmux::Pane>,
    pub flat_entries: Vec<FlatEntry>,
    pub opened: HashSet<NodeId>,
    pub seen_groups: HashSet<String>,
    pub list_state: ListState,
    pub preview_panes: Vec<PreviewPane>,
    pub preview_title: String,
    pub preview_notice: Option<String>,
    pub preview_full_panes: Vec<PreviewFullPane>,
    pub preview_full_index: usize,
    pub preview_generation: u64,
    pub mode: Mode,
    pub confirming_kill_target: Option<ConfirmKillTarget>,
    pub should_quit: bool,
    pub highlight_style: Style,
    pub primary_color: Color,
    pub filter_query: String,
    pub filter_cursor: usize,
    pub pinned: Vec<String>,
    pub hidden: Vec<String>,
    pub show_hidden: bool,
    pub renaming_target: Option<RenameTarget>,
    pub rename_buffer: String,
    pub rename_cursor: usize,
    pub marked_windows: Vec<String>,
    pub selecting: bool,
    pub selection_anchor: Option<usize>,
    pub move_query: String,
    pub move_cursor: usize,
    pub move_candidates: Vec<MoveCandidate>,
    pub move_selected: usize,
    pub move_source_session_cwd: String,
    pub create_query: String,
    pub create_cursor: usize,
    pub create_tab: CreateTab,
    pub create_available_tabs: Vec<CreateTab>,
    pub create_candidates: Vec<CreateCandidate>,
    pub create_selected: usize,
    pub create_worktrees: Vec<WorktreeEntry>,
    pub create_zoxide_entries: Vec<ZoxideEntry>,
    pub create_current_session_cwd: String,
    pub create_load_error: Option<String>,
    pub dead_sessions: Vec<DeadSession>,
    pub monitor_rows: Vec<ProcessRow>,
    pub monitor_entries: Vec<MonitorEntry>,
    pub monitor_collapsed: HashSet<u32>,
    pub monitor_selected: usize,
    pub monitor_sort: MonitorSort,
    pub monitor_list_state: ListState,
    pub confirming_process: Option<(u32, String)>,
    pending_preview_request: Option<PendingCaptureRequest>,
    pub preview_cache: HashMap<NodeId, Vec<PreviewPane>>,
    pub formatter_cache: HashMap<String, String>,
}

impl App {
    pub fn new() -> io::Result<Self> {
        let config = config::load_config()?;
        let current_session_id = tmux::get_current_session_id()?;
        let mut sessions = tmux::list_sessions(&current_session_id)?;
        sessions.sort_by(|a, b| b.activity.cmp(&a.activity));
        let windows = tmux::list_windows()?;
        let panes = tmux::list_panes()?;

        let mut history_entries = history::load_history();
        let now = current_unix_secs()?;
        history::upsert_live_sessions(&mut history_entries, &sessions, now);
        let dead_sessions = sessions::compute_dead_sessions(&history_entries, &sessions, &HashMap::new());

        let pinned = load_pins();
        let hidden = load_hidden();
        let show_hidden = false;
        let group_sep = config.as_ref().and_then(|c| c.group_name_separator.as_deref());
        let group_prefixes = extract_group_prefixes(&sessions, group_sep);
        let recent_age = recent_max_age_secs(config.as_ref());
        let mut opened: HashSet<NodeId> = group_prefixes.iter().map(|p| NodeId::Group(p.clone())).collect();
        if recent_age.is_some() {
            for prefix in group_prefixes.iter() {
                opened.insert(NodeId::Recent(Box::new(NodeId::Group(prefix.clone()))));
            }
        }
        let seen_groups: HashSet<String> = group_prefixes.into_iter().collect();
        let flat_entries = tree::flatten(&sessions, &windows, &panes, &opened, &pinned, &hidden, show_hidden, group_sep, recent_age);
        let mut list_state = ListState::default();
        let initial_index = flat_entries
            .iter()
            .position(|e| {
                sessions.iter().any(|s| {
                    s.attached
                        && windows.iter().any(|w| {
                            w.session_id == s.id
                                && w.active
                                && e.node_id.target() == &NodeId::Window(s.id.clone(), w.id.clone())
                        })
                })
            })
            .or_else(|| {
                flat_entries.iter().position(|e| {
                    sessions.iter().any(|s| s.attached && e.node_id.target() == &NodeId::Session(s.id.clone()))
                })
            })
            .or_else(|| if flat_entries.is_empty() { None } else { Some(0) });
        list_state.select(initial_index);

        let mode_style = tmux::get_mode_style().ok().map(|s| tmux::parse_style(&s)).unwrap_or_default();
        let primary_color = mode_style.bg.unwrap_or(Color::Yellow);
        let highlight_style = Style::default().bg(primary_color).fg(mode_style.fg.unwrap_or(Color::Black));

        let mut app = App {
            config,
            current_session_id,
            sessions,
            windows,
            panes,
            flat_entries,
            opened,
            seen_groups,
            list_state,
            preview_panes: Vec::new(),
            preview_title: String::new(),
            preview_notice: None,
            preview_full_panes: Vec::new(),
            preview_full_index: 0,
            preview_generation: 0,
            mode: Mode::Normal,
            confirming_kill_target: None,
            should_quit: false,
            highlight_style,
            primary_color,
            filter_query: String::new(),
            filter_cursor: 0,
            pinned,
            hidden,
            show_hidden,
            renaming_target: None,
            rename_buffer: String::new(),
            rename_cursor: 0,
            marked_windows: Vec::new(),
            selecting: false,
            selection_anchor: None,
            move_query: String::new(),
            move_cursor: 0,
            move_candidates: Vec::new(),
            move_selected: 0,
            move_source_session_cwd: String::new(),
            create_query: String::new(),
            create_cursor: 0,
            create_tab: CreateTab::History,
            create_available_tabs: Vec::new(),
            create_candidates: Vec::new(),
            create_selected: 0,
            create_worktrees: Vec::new(),
            create_zoxide_entries: Vec::new(),
            create_current_session_cwd: String::new(),
            create_load_error: None,
            dead_sessions,
            monitor_rows: Vec::new(),
            monitor_entries: Vec::new(),
            monitor_collapsed: HashSet::new(),
            monitor_selected: 0,
            monitor_sort: MonitorSort::Mem,
            monitor_list_state: ListState::default(),
            confirming_process: None,
            pending_preview_request: None,
            preview_cache: HashMap::new(),
            formatter_cache: HashMap::new(),
        };
        app.update_preview();
        Ok(app)
    }

    pub fn refresh(&mut self) -> io::Result<()> {
        let prev_node_id = self.list_state.selected().and_then(|i| self.flat_entries.get(i)).map(|e| e.node_id.clone());
        let prev_index = self.list_state.selected().unwrap_or(0);

        self.sessions = tmux::list_sessions(&self.current_session_id)?;
        for session in self.sessions.iter_mut() {
            if let Some(formatted) = self.formatter_cache.get(&session.name) {
                session.display_name = formatted.clone();
            }
        }
        self.sessions.sort_by(|a, b| b.activity.cmp(&a.activity));
        self.windows = tmux::list_windows()?;
        self.panes = tmux::list_panes()?;

        if self.sessions.is_empty() {
            self.should_quit = true;
            return Ok(());
        }

        let mut history_entries = history::load_history();
        let now = current_unix_secs()?;
        history::upsert_live_sessions(&mut history_entries, &self.sessions, now);
        self.dead_sessions = sessions::compute_dead_sessions(&history_entries, &self.sessions, &self.formatter_cache);

        let group_sep = self.config.as_ref().and_then(|c| c.group_name_separator.as_deref());
        let recent_age = recent_max_age_secs(self.config.as_ref());
        for prefix in extract_group_prefixes(&self.sessions, group_sep) {
            if !self.seen_groups.contains(&prefix) {
                self.opened.insert(NodeId::Group(prefix.clone()));
                if recent_age.is_some() {
                    self.opened.insert(NodeId::Recent(Box::new(NodeId::Group(prefix.clone()))));
                }
                self.seen_groups.insert(prefix);
            }
        }

        self.rebuild_flat_entries();

        let new_index = prev_node_id
            .and_then(|id| self.flat_entries.iter().position(|e| e.node_id == id))
            .unwrap_or_else(|| prev_index.min(self.flat_entries.len().saturating_sub(1)));
        self.list_state.select(Some(new_index));

        self.update_preview();
        Ok(())
    }

    fn rebuild_flat_entries(&mut self) {
        let sep = self.config.as_ref().and_then(|c| c.group_name_separator.as_deref());
        if self.filter_query.is_empty() {
            let sep = self.config.as_ref().and_then(|c| c.group_name_separator.as_deref());
            let recent_age = recent_max_age_secs(self.config.as_ref());
            self.flat_entries = tree::flatten(&self.sessions, &self.windows, &self.panes, &self.opened, &self.pinned, &self.hidden, self.show_hidden, sep, recent_age);
            return;
        }

        let matched_sessions: Vec<tmux::Session> = tree::match_live_sessions(&self.sessions, &self.filter_query).into_iter().cloned().collect();
        let recent_age = recent_max_age_secs(self.config.as_ref());
        let mut flat_entries = tree::flatten(
            &matched_sessions,
            &self.windows,
            &self.panes,
            &self.opened,
            &self.pinned,
            &self.hidden,
            self.show_hidden,
            sep,
            recent_age,
        );
        let dead_refs: Vec<DeadSessionRef<'_>> = self.dead_sessions.iter().map(|d| DeadSessionRef {
            name: &d.name,
            display_name: &d.display_name,
            last_seen: d.last_seen,
        }).collect();
        let matched_dead_entries = tree::match_dead_sessions(&dead_refs, &self.filter_query);
        flat_entries.extend(matched_dead_entries);
        self.flat_entries = flat_entries;
    }

    pub fn handle_action(&mut self, action: Action) {
        match action {
            Action::Quit => self.handle_quit(),
            Action::ClearMarksOrQuit => self.handle_clear_marks_or_quit(),
            Action::MoveUp => self.handle_move_up(),
            Action::MoveDown => self.handle_move_down(),
            Action::TogglePin => self.handle_toggle_pin(),
            Action::ToggleHide => self.handle_toggle_hide(),
            Action::ToggleShowHidden => self.handle_toggle_show_hidden(),
            Action::MovePinUp => self.handle_move_pin_up(),
            Action::MovePinDown => self.handle_move_pin_down(),
            Action::CollapseOrParent => self.handle_collapse_or_parent(),
            Action::ExpandOrChild => self.handle_expand_or_child(),
            Action::EnterFullPreview => self.handle_enter_full_preview(),
            Action::ExitFullPreview => self.handle_exit_full_preview(),
            Action::PreviewPrev => self.handle_preview_prev(),
            Action::PreviewNext => self.handle_preview_next(),
            Action::SelectPreviewPane => self.handle_select_preview_pane(),
            Action::Select => self.handle_select_dispatch(),
            Action::Kill => self.handle_kill(),
            Action::ConfirmKill => self.handle_confirm_kill(),
            Action::CancelKill => self.handle_cancel_kill(),
            Action::OpenAbout => {
                self.mode = Mode::About;
            }
            Action::CloseAbout => {
                self.mode = Mode::Normal;
            }
            Action::Refresh => {
                let _ = self.refresh();
            }
            Action::EnterFilter => self.handle_enter_filter(),
            Action::DetachFilter => self.handle_detach_filter(),
            Action::ToggleMarkWindow => self.handle_toggle_mark_window(),
            Action::EnterMoveWindow => self.handle_enter_move_window(),
            Action::EnterCreate => self.handle_enter_create(),
            Action::FilterChar(c) => self.handle_filter_char(c),
            Action::FilterBackspace => self.handle_filter_backspace(),
            Action::FilterDeleteForward => self.handle_filter_delete_forward(),
            Action::FilterKillWord => self.handle_filter_kill_word(),
            Action::FilterKillLine => self.handle_filter_kill_line(),
            Action::FilterKillLineForward => self.handle_filter_kill_line_forward(),
            Action::FilterCursorLeft => self.handle_filter_cursor_left(),
            Action::FilterCursorRight => self.handle_filter_cursor_right(),
            Action::FilterCursorWordLeft => self.handle_filter_cursor_word_left(),
            Action::FilterCursorWordRight => self.handle_filter_cursor_word_right(),
            Action::FilterCursorStart => self.handle_filter_cursor_start(),
            Action::FilterCursorEnd => self.handle_filter_cursor_end(),
            Action::ExitFilter => self.handle_exit_filter(),
            Action::MoveWindowChar(c) => self.handle_move_window_char(c),
            Action::MoveWindowBackspace => self.handle_move_window_backspace(),
            Action::MoveWindowDeleteForward => self.handle_move_window_delete_forward(),
            Action::MoveWindowKillWord => self.handle_move_window_kill_word(),
            Action::MoveWindowKillLine => self.handle_move_window_kill_line(),
            Action::MoveWindowKillLineForward => self.handle_move_window_kill_line_forward(),
            Action::MoveWindowCursorLeft => self.handle_move_window_cursor_left(),
            Action::MoveWindowCursorRight => self.handle_move_window_cursor_right(),
            Action::MoveWindowCursorWordLeft => self.handle_move_window_cursor_word_left(),
            Action::MoveWindowCursorWordRight => self.handle_move_window_cursor_word_right(),
            Action::MoveWindowCursorStart => self.handle_move_window_cursor_start(),
            Action::MoveWindowCursorEnd => self.handle_move_window_cursor_end(),
            Action::MoveWindowNext => self.handle_move_window_next(),
            Action::MoveWindowPrev => self.handle_move_window_prev(),
            Action::ConfirmMoveWindow => self.handle_confirm_move_window(),
            Action::CancelMoveWindow => self.handle_cancel_move_window(),
            Action::CreateChar(c) => self.handle_create_char(c),
            Action::CreateBackspace => self.handle_create_backspace(),
            Action::CreateDeleteForward => self.handle_create_delete_forward(),
            Action::CreateKillWord => self.handle_create_kill_word(),
            Action::CreateKillLine => self.handle_create_kill_line(),
            Action::CreateKillLineForward => self.handle_create_kill_line_forward(),
            Action::CreateCursorLeft => self.handle_create_cursor_left(),
            Action::CreateCursorRight => self.handle_create_cursor_right(),
            Action::CreateCursorWordLeft => self.handle_create_cursor_word_left(),
            Action::CreateCursorWordRight => self.handle_create_cursor_word_right(),
            Action::CreateCursorStart => self.handle_create_cursor_start(),
            Action::CreateCursorEnd => self.handle_create_cursor_end(),
            Action::CreateNext => self.handle_create_next(),
            Action::CreatePrev => self.handle_create_prev(),
            Action::CreateTabNext => self.handle_create_tab_next(),
            Action::CreateTabPrev => self.handle_create_tab_prev(),
            Action::ConfirmCreate => self.handle_confirm_create(),
            Action::CancelCreate => self.handle_cancel_create(),
            Action::SelectIndex(i) => self.handle_select_index(i),
            Action::StartRename => self.handle_start_rename(),
            Action::RenameChar(c) => self.handle_rename_char(c),
            Action::RenameBackspace => self.handle_rename_backspace(),
            Action::RenameDeleteForward => self.handle_rename_delete_forward(),
            Action::RenameKillWord => self.handle_rename_kill_word(),
            Action::RenameKillLine => self.handle_rename_kill_line(),
            Action::RenameKillLineForward => self.handle_rename_kill_line_forward(),
            Action::RenameCursorLeft => self.handle_rename_cursor_left(),
            Action::RenameCursorRight => self.handle_rename_cursor_right(),
            Action::RenameCursorWordLeft => self.handle_rename_cursor_word_left(),
            Action::RenameCursorWordRight => self.handle_rename_cursor_word_right(),
            Action::RenameCursorStart => self.handle_rename_cursor_start(),
            Action::RenameCursorEnd => self.handle_rename_cursor_end(),
            Action::ConfirmRename => self.handle_confirm_rename(),
            Action::CancelRename => self.handle_cancel_rename(),
            Action::EnterMonitor => self.handle_enter_monitor(),
            Action::ExitMonitor => self.handle_exit_monitor(),
            Action::ToggleMonitorSort => self.handle_toggle_monitor_sort(),
            Action::OpenProcessDetail => self.handle_open_process_detail(),
            Action::CloseProcessDetail => self.handle_close_process_detail(),
            Action::Tick => self.handle_tick(),
            Action::None => {}
        }
    }
}
