use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use regex::Regex;

use crate::agent_orchestrator::AgentOrchestrator;
use crate::agent_sandbox::AgentSandbox;
use crate::agent_tools::AgentToolExecutor;
use crate::document::Document;
use crate::keys::Key;
use crate::localization::{texts, Language};
use crate::recents;
use crate::session;
use crate::settings;
use crate::terminal::{winsize_tty, TermSize};
use crate::llm_api::{
    ChatMessage as LlmChatMessage, 
    ChatRequest as LlmChatRequest, 
    EditorContext as LlmEditorContext,
    GenerateParams as LlmGenerateParams, 
    TceLlmClient,
};
use crate::tree::{self as filetree, TreeEntry};
use crate::buffer::Buffer;
use crate::languages;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Focus {
    Editor,
    Sidebar,
    Tabs,
    RightPanel,
}

pub struct Workspace {
    pub project_root: PathBuf,
    tree: Vec<TreeEntry>,
    tree_sel: usize,
    tree_scroll: usize,
    docs: Vec<Document>,
    active_doc: usize,
    tab_sel: usize,
    sidebar_visible: bool,
    right_panel_visible: bool,
    right_panel_input: String,
    focus: Focus,
    tip: Option<String>,
    language: Language,
    language_picker: bool,
    language_sel: usize,
    hotkeys_help: bool,
    sidebar_menu_open: bool,
    sidebar_menu_sel: usize,
    sidebar_prompt: Option<SidebarPrompt>,
    quick_open: Option<QuickOpenState>,
    in_file_find: Option<InFileFindState>,
    project_search: Option<ProjectSearchState>,
    symbol_jump: Option<SymbolJumpState>,
    go_to_line: Option<GoToLineState>,
    llm_prompt: Option<LlmPromptState>,
    llm_history_view: Option<LlmHistoryViewState>,
    agent_events_view: Option<AgentEventsViewState>,
    agent_unsafe_confirm: bool,
    multi_edit: Option<MultiEditState>,
    sync_edit: Option<SyncEditState>,
    command_palette: Option<CommandPaletteState>,
    diagnostics: Option<DiagnosticsState>,
    git_view: Option<GitViewState>,
    nav_back: Vec<NavLocation>,
    nav_forward: Vec<NavLocation>,
    autosave_on_edit: bool,
    pending_delete_path: Option<PathBuf>,
    move_pick_path: Option<PathBuf>,
    dark_theme: bool,
    font_zoom: i8,
    line_spacing: bool,
    ligatures: bool,
    llm_health_checked: bool,
    llm_history: Vec<LlmHistoryEntry>,
    agent_events: Vec<String>,
    llm_status: String,
    llm_inflight: Option<LlmInFlight>,
    agent_inflight: Option<AgentInFlight>,
    agent_allow_unsafe_tools: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SidebarAction {
    Open,
    NewFile,
    NewFolder,
    Move,
    Rename,
    Delete,
    Refresh,
}

impl SidebarAction {
    fn all() -> &'static [SidebarAction] {
        &[
            SidebarAction::Open,
            SidebarAction::NewFile,
            SidebarAction::NewFolder,
            SidebarAction::Move,
            SidebarAction::Rename,
            SidebarAction::Delete,
            SidebarAction::Refresh,
        ]
    }

    fn label(self) -> &'static str {
        match self {
            SidebarAction::Open => "Open",
            SidebarAction::NewFile => "New file",
            SidebarAction::NewFolder => "New folder",
            SidebarAction::Move => "Move",
            SidebarAction::Rename => "Rename",
            SidebarAction::Delete => "Delete",
            SidebarAction::Refresh => "Refresh",
        }
    }

    fn shortcut(self) -> char {
        match self {
            SidebarAction::Open => 'O',
            SidebarAction::NewFile => 'F',
            SidebarAction::NewFolder => 'D',
            SidebarAction::Move => 'V',
            SidebarAction::Rename => 'R',
            SidebarAction::Delete => 'X',
            SidebarAction::Refresh => 'U',
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SidebarPromptKind {
    CreateFile,
    CreateFolder,
    Move,
    Rename,
}

#[derive(Clone, Debug)]
struct SidebarPrompt {
    kind: SidebarPromptKind,
    base_dir: PathBuf,
    target_path: Option<PathBuf>,
    input: String,
}

#[derive(Clone, Debug, Default)]
struct QuickOpenState {
    query: String,
    sel: usize,
}

#[derive(Clone, Debug, Default)]
struct InFileFindState {
    query: String,
    sel: usize,
}

#[derive(Clone, Debug)]
struct SearchMatch {
    path: PathBuf,
    line_idx: usize,
    col_idx: usize,
    preview: String,
}

#[derive(Clone, Debug, Default)]
struct ProjectSearchState {
    query: String,
    replacement: String,
    sel: usize,
    edit_replacement: bool,
    regex_mode: bool,
    confirm_replace_all: bool,
}

#[derive(Clone, Debug)]
struct SymbolItem {
    path: PathBuf,
    line_idx: usize,
    name: String,
    kind: String,
}

#[derive(Clone, Debug, Default)]
struct SymbolJumpState {
    query: String,
    sel: usize,
}

#[derive(Clone, Debug, Default)]
struct GoToLineState {
    input: String,
}

#[derive(Clone, Debug, Default)]
struct LlmPromptState {
    input: String,
}

#[derive(Clone, Debug)]
struct LlmHistoryEntry {
    role: String,
    content: String,
}

#[derive(Clone, Debug, Default)]
struct LlmHistoryViewState {
    scroll: usize,
    cursor: usize,
}

#[derive(Clone, Debug, Default)]
struct AgentEventsViewState {
    scroll: usize,
    cursor: usize,
}

struct LlmInFlight {
    cancel: Arc<AtomicBool>,
    rx: mpsc::Receiver<LlmTaskResult>,
}

enum LlmTaskResult {
    Delta(String),
    Ok(String),
    Err(String),
}

struct AgentInFlight {
    rx: mpsc::Receiver<AgentTaskResult>,
}

enum AgentTaskResult {
    Ok {
        summary: String,
        steps: usize,
        finished: bool,
        events: Vec<String>,
    },
    Err(String),
}

#[derive(Clone, Debug, Default)]
struct MultiEditState {
    target: String,
    replacement: String,
}

#[derive(Clone, Debug)]
struct SyncEditState {
    target: String,
    replacement: String,
    original_text: String,
    occurrences: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct NavLocation {
    path: PathBuf,
    row: usize,
    col: usize,
}

#[derive(Clone, Debug, Default)]
struct CommandPaletteState {
    query: String,
    sel: usize,
}

#[derive(Clone, Debug)]
struct DiagnosticItem {
    path: PathBuf,
    row: usize,
    col: usize,
    message: String,
    severity: DiagnosticSeverity,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DiagnosticSeverity {
    Error,
    Warning,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DiagnosticsFilter {
    All,
    Errors,
    Warnings,
}

#[derive(Clone, Debug)]
struct DiagnosticsState {
    items: Vec<DiagnosticItem>,
    sel: usize,
    open: bool,
    filter: DiagnosticsFilter,
}

impl Default for DiagnosticsState {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            sel: 0,
            open: false,
            filter: DiagnosticsFilter::All,
        }
    }
}

#[derive(Clone, Debug)]
struct GitViewState {
    title: String,
    lines: Vec<String>,
    /// Индекс первой видимой строки
    scroll: usize,
    /// Индекс выбранной строки в `lines`
    cursor: usize,
}

impl Workspace {
    fn doc(&self) -> &Document {
        self.docs.get(self.active_doc).or_else(|| self.docs.first()).expect("workspace always keeps at least one document")
    }

    fn doc_mut(&mut self) -> &mut Document {
        if self.docs.is_empty() {
            self.docs.push(Document::empty());
            self.active_doc = 0;
        } else if self.active_doc >= self.docs.len() {
            self.active_doc = self.docs.len().saturating_sub(1);
        }

        &mut self.docs[self.active_doc]
    }

    pub fn open_project(root: PathBuf) -> io::Result<Self> {
        let _ = recents::push_front(root.clone());
        let tree = filetree::build_tree(&root)?;
        let persisted = session::load_project_session(&root);
        let app_settings = settings::load_settings();
        let mut docs: Vec<Document> = persisted
            .tabs
            .iter()
            .filter_map(|path| Document::open_file(path.clone()).ok())
            .collect();
        for doc in &mut docs {
            if let Some(path) = &doc.path {
                doc.pinned = persisted.pinned.iter().any(|p| p == path);
            }
        }

        if docs.is_empty() {
            docs.push(Document::empty());
        }

        let active_doc = persisted
            .active
            .as_ref()
            .and_then(|active| {
                docs.iter().position(|d| d.path.as_ref().is_some_and(|p| p == active))
            })
            .unwrap_or(0)
            .min(docs.len().saturating_sub(1));
        let tab_sel = active_doc;
        let tree_sel = docs
            .get(active_doc)
            .and_then(|d| d.path.as_ref())
            .and_then(|active_path| tree.iter().position(|e| e.path == *active_path))
            .or_else(|| tree.iter().position(|e| !e.is_dir))
            .unwrap_or(0)
            .min(tree.len().saturating_sub(1));

        Ok(Self {
            project_root: root,
            tree,
            tree_sel,
            tree_scroll: 0,
            docs,
            active_doc,
            tab_sel,
            sidebar_visible: true,
            right_panel_visible: app_settings.llm_enabled && app_settings.right_panel_visible,
            right_panel_input: String::new(),
            focus: Focus::Editor,
            tip: None,
            language: app_settings.language,
            language_picker: false,
            language_sel: 0,
            hotkeys_help: false,
            sidebar_menu_open: false,
            sidebar_menu_sel: 0,
            sidebar_prompt: None,
            quick_open: None,
            in_file_find: None,
            project_search: None,
            symbol_jump: None,
            go_to_line: None,
            llm_prompt: None,
            llm_history_view: None,
            agent_events_view: None,
            agent_unsafe_confirm: false,
            multi_edit: None,
            sync_edit: None,
            command_palette: None,
            diagnostics: None,
            git_view: None,
            nav_back: Vec::new(),
            nav_forward: Vec::new(),
            autosave_on_edit: app_settings.autosave_on_edit,
            pending_delete_path: None,
            move_pick_path: None,
            dark_theme: app_settings.dark_theme,
            font_zoom: app_settings.font_zoom,
            line_spacing: app_settings.line_spacing,
            ligatures: app_settings.ligatures,
            llm_health_checked: false,
            llm_history: Vec::new(),
            agent_events: Vec::new(),
            llm_status: "idle".to_string(),
            llm_inflight: None,
            agent_inflight: None,
            agent_allow_unsafe_tools: false,
        })
    }

    fn gutter_color(&self) -> &'static str {
        if self.dark_theme { "\x1b[90m" } else { "\x1b[30m" }
    }

    fn current_line_bg(&self) -> &'static str {
        if self.dark_theme { "\x1b[48;5;236m" } else { "\x1b[48;5;254m" }
    }

    fn tab_bar_bg(&self) -> &'static str {
        if self.dark_theme { "\x1b[48;5;236m\x1b[37m" } else { "\x1b[48;5;252m\x1b[30m" }
    }

    fn tab_active_bg(&self) -> &'static str {
        if self.dark_theme { "\x1b[48;5;31m\x1b[97m" } else { "\x1b[48;5;27m\x1b[97m" }
    }

    fn tab_focus_bg(&self) -> &'static str {
        if self.dark_theme { "\x1b[48;5;75m\x1b[30m" } else { "\x1b[48;5;153m\x1b[30m" }
    }

    fn tab_inactive_focus_bg(&self) -> &'static str {
        if self.dark_theme { "\x1b[48;5;240m\x1b[97m" } else { "\x1b[48;5;250m\x1b[30m" }
    }

    pub fn open_file_in_project(file: PathBuf) -> io::Result<Self> {
        let mut root = file
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        if root.as_os_str().is_empty() {
            root = std::env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf());
        }

        let root = root.canonicalize().unwrap_or(root);
        let file_canon = file.canonicalize().unwrap_or_else(|_| file.clone());
        let mut ws = Self::open_project(root)?;
        ws.docs = vec![Document::open_file(file)?];
        ws.active_doc = 0;
        ws.tab_sel = 0;
        ws.tree_sel = ws.tree.iter().position(|e| e.path == file_canon).unwrap_or(ws.tree_sel);
        ws.focus = Focus::Editor;
        Ok(ws)
    }

    pub fn open_dir(dir: PathBuf) -> io::Result<Self> {
        let root = dir.canonicalize().unwrap_or(dir);
        Self::open_project(root)
    }

    pub fn set_language(&mut self, language: Language) {
        self.language = language;
        self.persist_settings();
    }

    fn persist_settings(&self) {
        let mut s = settings::load_settings();
        s.dark_theme = self.dark_theme;
        s.autosave_on_edit = self.autosave_on_edit;
        s.font_zoom = self.font_zoom;
        s.line_spacing = self.line_spacing;
        s.ligatures = self.ligatures;
        s.language = self.language;
        let _ = settings::save_settings(&s);
    }

    fn llm_enabled_in_settings() -> bool {
        settings::load_settings().llm_enabled
    }
    fn sidebar_width_cols(term_cols: usize) -> usize {
        if term_cols < 48 {
            return term_cols.min(20).max(12);
        }
        (term_cols / 4).clamp(18, 36)
    }

    fn right_panel_width_cols(term_cols: usize) -> usize {
        if term_cols < 64 {
            return 22;
        }
        (term_cols / 4).clamp(22, 34)
    }

    fn editor_width(term_cols: usize, sidebar_visible: bool, right_panel_visible: bool) -> usize {
        let sidebar_w = if sidebar_visible {
            Self::sidebar_width_cols(term_cols) + 1
        } else {
            0
        };
        let right_w = if right_panel_visible {
            Self::right_panel_width_cols(term_cols) + 1
        } else {
            0
        };
        term_cols.saturating_sub(sidebar_w + right_w).max(12)
    }

    fn editor_gutter_width(&self) -> usize {
        let digits = self.doc().buffer.line_count().max(1).to_string().chars().count();
        (digits + 2).clamp(4, 8)
    }

    fn editor_text_width(&self, available: usize) -> usize {
        let adjust = (self.font_zoom.max(0) as usize).saturating_mul(6);
        let expanded = available.saturating_add(((-self.font_zoom).max(0) as usize).saturating_mul(4));
        if self.font_zoom >= 0 {
            available.saturating_sub(adjust).max(1)
        } else {
            expanded.max(1)
        }
    }

    fn dirty_docs_count(&self) -> usize {
        self.docs.iter().filter(|d| d.dirty).count()
    }

    fn diagnostic_for_current_line(&self) -> Option<String> {
        let state = self.diagnostics.as_ref()?;
        let path = self.doc().path.as_ref()?;
        let row = self.doc().row;
        state.items.iter().find(|d| &d.path == path && d.row == row).map(|d| {
            let sev = match d.severity {
                DiagnosticSeverity::Error => "E",
                DiagnosticSeverity::Warning => "W",
            };
            format!("[{sev}] {}", d.message)
        })
    }

    fn line_diagnostic_marker(&self, line_idx: usize) -> Option<char> {
        let Some(state) = self.diagnostics.as_ref() else {
            return None;
        };

        let Some(path) = self.doc().path.as_ref() else {
            return None;
        };

        if state.items.iter().any(|d| &d.path == path && d.row == line_idx && d.severity == DiagnosticSeverity::Error) {
            return Some('E');
        }

        if state.items.iter().any(|d| &d.path == path && d.row == line_idx && d.severity == DiagnosticSeverity::Warning) {
            return Some('W');
        }
        None
    }

    pub fn render(&mut self) -> io::Result<()> {
        if self.hotkeys_help {
            return self.render_hotkeys_help();
        }

        if self.language_picker {
            return self.render_language_picker();
        }

        self.doc_mut().clamp_cursor();
        let size = winsize_tty().unwrap_or(TermSize { rows: 24, cols: 80 });
        let rows = size.rows.max(1) as usize;
        let cols = size.cols.max(1) as usize;
        let tabs_h = 1usize;
        let content_h = rows.saturating_sub(1 + tabs_h).max(1);
        let line_stride = if self.line_spacing { 2 } else { 1 };
        let logical_content_h = (content_h / line_stride).max(1);
        let sidebar_w = Self::sidebar_width_cols(cols);
        let sidebar_on = self.sidebar_visible && cols > sidebar_w + 4;
        let right_panel_w = Self::right_panel_width_cols(cols);
        let llm_enabled = Self::llm_enabled_in_settings();
        let right_panel_on = llm_enabled && self.right_panel_visible && cols > right_panel_w + 8;
        let editor_w = Self::editor_width(cols, sidebar_on, right_panel_on);
        let gutter_w = self.editor_gutter_width();

        self.doc_mut().adjust_scroll(logical_content_h, editor_w.max(1));
        self.adjust_tree_scroll(logical_content_h);

        let mut out = String::with_capacity(rows * (cols + 32));
        out.push_str("\x1b[H\x1b[J");
        let tabs = self.tabs_line(cols);
        out.push_str(&tabs);
        out.push_str("\r\n");

        for row in 0..content_h {
            if self.line_spacing && row % 2 == 1 {
                if sidebar_on {
                    out.push_str(&" ".repeat(sidebar_w));
                    out.push_str("\x1b[0m│");
                }
                out.push_str(&" ".repeat(editor_w.min(cols)));
                if right_panel_on {
                    out.push_str("│");
                    out.push_str(&" ".repeat(right_panel_w));
                }
                out.push_str("\r\n");
                continue;
            }

            let doc = self.doc();
            let logical_row = row / line_stride;
            let line_idx = doc.scroll_row + logical_row;
            let editor_text_w = self.editor_text_width(editor_w.saturating_sub(gutter_w).max(1));
            let mut editor_line = format!(
                "{}{:>width$}{} \x1b[0m",
                self.gutter_color(),
                line_idx.saturating_add(1),
                self.line_diagnostic_marker(line_idx).unwrap_or(' '),
                width = gutter_w.saturating_sub(1)
            );
            let text = doc.editor_line_display(line_idx, editor_text_w);
            let clipped_raw: String = text.chars().take(editor_text_w).collect();
            let clipped_raw = apply_ligatures(&clipped_raw, self.ligatures);
            let clipped = languages::syntax_highlight_line(doc.path.as_ref(), &clipped_raw);

            if line_idx == doc.row {
                editor_line.push_str(self.current_line_bg());
                editor_line.push_str(&clipped);
                editor_line.push_str("\x1b[0m");
            } else {
                editor_line.push_str(&clipped);
            }

            if sidebar_on {
                let line = self.sidebar_line(logical_row, sidebar_w);
                out.push_str(&line);
                out.push_str("\x1b[0m│");
            } else {
            }
            let editor_segment = pad_ansi_to_width(&editor_line, editor_w);
            out.push_str(&editor_segment);
            if right_panel_on {
                out.push_str("│");
                let panel_line = self.right_chat_panel_line(logical_row, logical_content_h, right_panel_w);
                out.push_str(&panel_line);
            }
            out.push_str("\r\n");
        }

        let proj = self.project_root.to_string_lossy();
        let dirty_count = self.dirty_docs_count();
        let dirty = if self.doc().dirty { " *" } else { "" };
        let dirty_badge = if dirty_count > 0 {
            format!("unsaved:{dirty_count}")
        } else {
            "saved".to_string()
        };

        let tx = texts(self.language);
        let focus_hint = match self.focus {
            Focus::Sidebar => tx.hint_sidebar_focus_actions,
            Focus::Tabs => tx.hint_tabs_focus_actions,
            Focus::Editor => tx.hint_sidebar_focus,
            Focus::RightPanel => "right panel: type prompt | Enter send | Ctrl+C cancel",
        };

        let quit_hint = if self.doc().force_quit_pending {
            format!(" {} ", tx.hint_ctrl_q_again_quit)
        } else {
            format!(" {} ", tx.hint_ctrl_q_quit)
        };

        let tip = self.tip.as_deref().unwrap_or(focus_hint);
        let diag_tip = self.diagnostic_for_current_line();
        let tip = diag_tip.as_deref().unwrap_or(tip);
        let status = format!(
            "\x1b[7m {} | {} | {}:{} |{}{}{} | {} {} {} {} {} | {} | llm:{} | {}\x1b[m",
            truncate_str(&proj, 18),
            truncate_str(&self.doc().path_display(), 22),
            self.doc().row.saturating_add(1),
            self.doc().col.saturating_add(1),
            dirty,
            quit_hint,
            tx.hint_ctrl_s_save,
            tx.hint_ctrl_b,
            tx.hint_ctrl_r,
            tx.hint_shift_tab,
            tx.hint_ctrl_l_lang,
            tx.hint_ctrl_k_help,
            dirty_badge,
            self.llm_status,
            truncate_str(tip, cols.saturating_sub(90))
        );

        let status: String = status.chars().take(cols).collect();
        out.push_str(&status);

        if self.sidebar_menu_open {
            self.render_sidebar_menu_overlay(&mut out, cols, rows);
        } else if self.agent_unsafe_confirm {
            self.render_agent_unsafe_confirm_overlay(&mut out, cols, rows);
        } else if self.sidebar_prompt.is_some() {
            self.render_sidebar_prompt_overlay(&mut out, cols, rows);
        } else if self.go_to_line.is_some() {
            self.render_go_to_line_overlay(&mut out, cols, rows);
        } else if self.llm_prompt.is_some() {
            self.render_llm_prompt_overlay(&mut out, cols, rows);
        } else if self.multi_edit.is_some() {
            self.render_multi_edit_overlay(&mut out, cols, rows);
        } else if self.sync_edit.is_some() {
            self.render_sync_edit_overlay(&mut out, cols, rows);
        } else if self.command_palette.is_some() {
            self.render_command_palette_overlay(&mut out, cols, rows);
        } else if self.llm_history_view.is_some() {
            self.render_llm_history_overlay(&mut out, cols, rows);
        } else if self.agent_events_view.is_some() {
            self.render_agent_events_overlay(&mut out, cols, rows);
        } else if self.diagnostics.as_ref().is_some_and(|d| d.open) {
            self.render_diagnostics_overlay(&mut out, cols, rows);
        } else if self.git_view.is_some() {
            self.render_git_view_overlay(&mut out, cols, rows);
        } else if self.symbol_jump.is_some() {
            self.render_symbol_jump_overlay(&mut out, cols, rows);
        } else if self.project_search.is_some() {
            self.render_project_search_overlay(&mut out, cols, rows);
        } else if self.in_file_find.is_some() {
            self.render_in_file_find_overlay(&mut out, cols, rows);
        } else if self.quick_open.is_some() {
            self.render_quick_open_overlay(&mut out, cols, rows);
        }

        let (sr, sc) = self.cursor_screen_pos(content_h, cols, sidebar_w, right_panel_on, right_panel_w);
        out.push_str(&format!("\x1b[{};{}H", sr, sc));

        let mut stdout = io::stdout().lock();
        stdout.write_all(out.as_bytes())?;
        stdout.flush()?;
        Ok(())
    }

    fn cursor_screen_pos(
        &self,
        content_h: usize,
        cols: usize,
        sidebar_w: usize,
        right_panel_on: bool,
        right_panel_w: usize,
    ) -> (u32, u32) {
        let line_stride = if self.line_spacing { 2usize } else { 1usize };
        let logical_content_h = (content_h / line_stride).max(1);
        if self.focus == Focus::Tabs {
            let col = self.tab_cursor_col(cols);
            return (1, col.max(1));
        }

        if let Some(state) = &self.in_file_find {
            let tx = texts(self.language);
            let prompt = format!("{} {}", tx.find_in_file_prompt, state.query);
            let col = prompt.chars().count().saturating_add(1) as u32;
            return (rows_to_u32(content_h + 2), col.max(1));
        }

        if let Some(state) = &self.quick_open {
            let prompt = format!("Quick open: {}", state.query);
            let col = prompt.chars().count().saturating_add(1) as u32;
            return (rows_to_u32(content_h + 2), col.max(1));
        }

        if let Some(state) = &self.project_search {
            let mode = if state.regex_mode { "regex" } else { "text" };
            let prefix = if state.edit_replacement {
                format!("[{mode}] replace* Search: {} | Replace: ", state.query)
            } else {
                format!("[{mode}] search* Search: ",)
            };

            let typed_len = if state.edit_replacement {
                state.replacement.chars().count()
            } else {
                state.query.chars().count()
            };

            let col = prefix.chars().count().saturating_add(typed_len).saturating_add(1) as u32;
            return (rows_to_u32(content_h + 2), col.max(1));
        }

        if let Some(state) = &self.symbol_jump {
            let prompt = format!("Symbols: {}", state.query);
            let col = prompt.chars().count().saturating_add(1) as u32;
            return (rows_to_u32(content_h + 2), col.max(1));
        }

        if let Some(state) = &self.go_to_line {
            let prompt = format!("Go to line: {}", state.input);
            let col = prompt.chars().count().saturating_add(1) as u32;
            return (rows_to_u32(content_h + 2), col.max(1));
        }

        if let Some(state) = &self.llm_prompt {
            let prompt = format!("{} {}", texts(self.language).llm_ask_prefix, state.input);
            let col = prompt.chars().count().saturating_add(1) as u32;
            return (rows_to_u32(content_h + 2), col.max(1));
        }

        if let Some(state) = &self.command_palette {
            let prompt = format!("Command: {}", state.query);
            let col = prompt.chars().count().saturating_add(1) as u32;
            return (rows_to_u32(content_h + 2), col.max(1));
        }

        if self.diagnostics.as_ref().is_some_and(|d| d.open) {
            return (rows_to_u32(content_h + 2), 1);
        }

        if self.git_view.is_some() {
            return (rows_to_u32(content_h + 2), 1);
        }

        if let Some(state) = &self.multi_edit {
            let prompt = format!("Multi-edit '{}' -> {}", state.target, state.replacement);
            let col = prompt.chars().count().saturating_add(1) as u32;
            return (rows_to_u32(content_h + 2), col.max(1));
        }

        if let Some(state) = &self.sync_edit {
            let prompt = format!("Sync-edit '{}' -> {}", state.target, state.replacement);
            let col = prompt.chars().count().saturating_add(1) as u32;
            return (rows_to_u32(content_h + 2), col.max(1));
        }

        if self.focus == Focus::Sidebar && self.sidebar_visible && cols > sidebar_w + 4 {
            if self.sidebar_menu_open {
                return (3, 2);
            }

            if let Some(prompt) = &self.sidebar_prompt {
                let prefix_len = match prompt.kind {
                    SidebarPromptKind::CreateFile => 10usize,
                    SidebarPromptKind::CreateFolder => 12usize,
                    SidebarPromptKind::Move => 9usize,
                    SidebarPromptKind::Rename => 11usize,
                };

                let col = prompt.input.chars().count().saturating_add(prefix_len) as u32;
                return (rows_to_u32(content_h + 2), col.max(1));
            }

            let vis = self.tree_sel.saturating_sub(self.tree_scroll);
            let tree_h = logical_content_h.max(1);
            let r = (vis.min(tree_h.saturating_sub(1)) * line_stride + 2) as u32;
            let c = (self.tree.get(self.tree_sel).map(|e| e.depth * 2 + 2).unwrap_or(2) as u32).min(sidebar_w as u32);
            (r, c.max(1))
        } else if self.focus == Focus::RightPanel && right_panel_on {
            let prompt = format!("> {}", self.right_panel_input);
            let col_in_panel = prompt.chars().count().saturating_add(1);
            let left = if self.sidebar_visible && cols > sidebar_w + 4 {
                sidebar_w + 1
            } else {
                0
            };
            let editor_w = Self::editor_width(cols, self.sidebar_visible && cols > sidebar_w + 4, right_panel_on);
            let panel_row = logical_content_h.saturating_sub(1) * line_stride + 2;
            let panel_col = left + editor_w + 2 + col_in_panel;
            (
                rows_to_u32(panel_row),
                (panel_col.min(left + editor_w + 1 + right_panel_w)).max(1) as u32,
            )
        } else {
            let doc = self.doc();
            let doc_row = doc.row.saturating_sub(doc.scroll_row);
            let r = (doc_row.min(logical_content_h.saturating_sub(1)) * line_stride + 2) as u32;
            let gutter_w = self.editor_gutter_width() as u32;
            let col_off = if self.sidebar_visible && cols > sidebar_w + 4 {
                (sidebar_w + 2) as u32 + gutter_w
            } else {
                gutter_w
            };

            let c = col_off + (doc.col.saturating_sub(doc.hscroll) as u32) + 1;
            (r, c.max(1))
        }
    }

    fn sidebar_line(&self, row: usize, sidebar_w: usize) -> String {
        let idx = self.tree_scroll + row;
        let mut s = String::new();
        if let Some(e) = self.tree.get(idx) {
            let prefix = "  ".repeat(e.depth);
            let mark = if e.is_dir { "+ " } else { "  " };
            let sel = self.focus == Focus::Sidebar && idx == self.tree_sel;
            if sel {
                s.push_str("\x1b[7m");
            }

            let body = format!("{prefix}{mark}{}", e.label);
            let clipped: String = body.chars().take(sidebar_w.saturating_sub(1)).collect();
            s.push_str(&clipped);
            if sel {
                s.push_str("\x1b[0m");
            }
            
            while s.chars().count() < sidebar_w {
                s.push(' ');
            }
        } else {
            while s.chars().count() < sidebar_w {
                s.push(' ');
            }
        }

        let total: String = s.chars().take(sidebar_w).collect();
        total
    }

    fn adjust_tree_scroll(&mut self, content_h: usize) {
        let tree_h = content_h.max(1);
        if self.tree_sel < self.tree_scroll {
            self.tree_scroll = self.tree_sel;
        }

        if self.tree_sel >= self.tree_scroll + tree_h {
            self.tree_scroll = self.tree_sel + 1 - tree_h;
        }
    }

    fn right_chat_panel_line(&self, row: usize, content_h: usize, panel_w: usize) -> String {
        let mut s = String::new();
        if row == 0 {
            s.push_str(" LLM Chat");
        } else if row + 1 >= content_h {
            s.push_str(&format!("> {}", self.right_panel_input));
        } else {
            let body_rows = content_h.saturating_sub(2);
            let total = self.llm_history.len();
            let start = total.saturating_sub(body_rows);
            let idx = start + row.saturating_sub(1);
            if let Some(item) = self.llm_history.get(idx) {
                let role = if item.role == "user" { "U" } else { "A" };
                let one_line = item.content.lines().next().unwrap_or("");
                s.push_str(&format!(" {role}: {}", one_line.trim()));
            }
        }

        let max_w = panel_w.saturating_sub(1);
        s = s.chars().take(max_w).collect();
        while s.chars().count() < panel_w {
            s.push(' ');
        }
        s.chars().take(panel_w).collect()
    }

    /// `true` = завершить приложение
    pub fn handle_key(&mut self, key: Key) -> io::Result<bool> {
        if self.hotkeys_help {
            match key {
                Key::CtrlH => {
                    self.hotkeys_help = false;
                }
                Key::CtrlQ => return self.doc_mut().handle_key(key),
                _ => {}
            }
            return Ok(false);
        }

        if self.language_picker {
            match key {
                Key::ArrowUp => {
                    self.language_sel = self.language_sel.saturating_sub(1);
                }
                Key::ArrowDown => {
                    self.language_sel = (self.language_sel + 1).min(1);
                }
                Key::Enter => {
                    self.language = if self.language_sel == 0 {
                        Language::En
                    } else {
                        Language::Ru
                    };
                    self.language_picker = false;
                    self.persist_settings();
                }
                Key::CtrlL => {
                    self.language_picker = false;
                }
                Key::CtrlQ => return self.doc_mut().handle_key(key),
                _ => {}
            }
            return Ok(false);
        }

        self.tip = None;
        self.poll_llm_inflight();
        self.poll_agent_inflight();
        if !Self::llm_enabled_in_settings() {
            self.right_panel_visible = false;
            if self.focus == Focus::RightPanel {
                self.focus = Focus::Editor;
            }
        }

        if matches!(key, Key::CtrlC) && self.llm_inflight.is_some() {
            self.cancel_llm_inflight();
            return Ok(false);
        }

        if self.agent_unsafe_confirm {
            self.handle_agent_unsafe_confirm_key(key);
            return Ok(false);
        }

        if self.sidebar_prompt.is_some() {
            self.handle_sidebar_prompt_key(key);
            return Ok(false);
        }

        if self.quick_open.is_some() {
            self.handle_quick_open_key(key);
            return Ok(false);
        }

        if self.in_file_find.is_some() {
            self.handle_in_file_find_key(key);
            return Ok(false);
        }
        
        if self.project_search.is_some() {
            self.handle_project_search_key(key);
            return Ok(false);
        }

        if self.symbol_jump.is_some() {
            self.handle_symbol_jump_key(key);
            return Ok(false);
        }

        if self.go_to_line.is_some() {
            self.handle_go_to_line_key(key);
            return Ok(false);
        }

        if self.llm_prompt.is_some() {
            self.handle_llm_prompt_key(key);
            return Ok(false);
        }

        if self.multi_edit.is_some() {
            self.handle_multi_edit_key(key);
            return Ok(false);
        }

        if self.sync_edit.is_some() {
            self.handle_sync_edit_key(key);
            return Ok(false);
        }

        if self.command_palette.is_some() {
            self.handle_command_palette_key(key);
            return Ok(false);
        }

        if self.llm_history_view.is_some() {
            self.handle_llm_history_view_key(key);
            return Ok(false);
        }

        if self.agent_events_view.is_some() {
            self.handle_agent_events_view_key(key);
            return Ok(false);
        }

        if self.diagnostics.as_ref().is_some_and(|d| d.open) {
            self.handle_diagnostics_key(key);
            return Ok(false);
        }

        if self.git_view.is_some() {
            self.handle_git_view_key(key);
            return Ok(false);
        }

        if self.sidebar_menu_open {
            self.handle_sidebar_menu_key(key);
            return Ok(false);
        }

        if matches!(key, Key::CtrlQ) {
            return self.doc_mut().handle_key(key);
        }

        if matches!(key, Key::CtrlS) {
            self.doc_mut().save()?;
            return Ok(false);
        }

        if matches!(key, Key::CtrlA) {
            self.navigate_back();
            return Ok(false);
        }

        if matches!(key, Key::CtrlZ) {
            self.navigate_forward();
            return Ok(false);
        }

        if matches!(key, Key::CtrlP) {
            self.next_tab();
            return Ok(false);
        }

        if matches!(key, Key::CtrlU) {
            self.prev_tab();
            return Ok(false);
        }

        if matches!(key, Key::CtrlW) {
            self.close_active_tab();
            return Ok(false);
        }

        if matches!(key, Key::CtrlX) {
            self.toggle_pin_active_tab();
            return Ok(false);
        }

        if matches!(key, Key::CtrlB) {
            self.sidebar_visible = !self.sidebar_visible;
            if !self.sidebar_visible {
                self.focus = Focus::Editor;
            }
            return Ok(false);
        }

        if matches!(key, Key::CtrlR) {
            if Self::llm_enabled_in_settings() {
                self.right_panel_visible = !self.right_panel_visible;
            } else {
                self.right_panel_visible = false;
                self.tip = Some(texts(self.language).llm_disabled.to_string());
            }
            if !self.right_panel_visible && self.focus == Focus::RightPanel {
                self.focus = Focus::Editor;
            }
            return Ok(false);
        }

        if matches!(key, Key::CtrlL) {
            self.language_picker = true;
            self.language_sel = if self.language == Language::En { 0 } else { 1 };
            return Ok(false);
        }

        if matches!(key, Key::CtrlH) {
            self.hotkeys_help = true;
            return Ok(false);
        }

        if matches!(key, Key::CtrlO) {
            self.quick_open = Some(QuickOpenState::default());
            return Ok(false);
        }

        if matches!(key, Key::CtrlF) {
            self.project_search = Some(ProjectSearchState::default());
            return Ok(false);
        }

        if matches!(key, Key::CtrlG) {
            self.llm_prompt = Some(LlmPromptState::default());
            self.focus = Focus::Editor;
            return Ok(false);
        }

        if matches!(key, Key::CtrlBackslash) {
            let w = self.word_under_cursor();
            let seed = if w.chars().count() > 80 {
                String::new()
            } else {
                w
            };
            
            self.in_file_find = Some(InFileFindState {
                query: seed,
                sel: 0,
            });

            return Ok(false);
        }

        if matches!(key, Key::CtrlT) {
            self.symbol_jump = Some(SymbolJumpState::default());
            return Ok(false);
        }

        if matches!(key, Key::CtrlY) {
            self.go_to_line = Some(GoToLineState::default());
            return Ok(false);
        }

        if matches!(key, Key::CtrlD) {
            let target = self.word_under_cursor();
            if target.is_empty() {
                self.tip = Some(texts(self.language).tip_no_word_under_cursor.to_string());
            } else {
                self.multi_edit = Some(MultiEditState {
                    target,
                    replacement: String::new(),
                });
            }

            return Ok(false);
        }

        if matches!(key, Key::CtrlE) {
            self.start_sync_edit();
            return Ok(false);
        }

        if matches!(key, Key::CtrlJ) && self.focus != Focus::Tabs {
            self.command_palette = Some(CommandPaletteState::default());
            return Ok(false);
        }

        if self.focus == Focus::Editor && self.docs.len() >= 2 {
            if matches!(key, Key::CtrlArrowLeft) {
                self.move_tab_left(self.active_doc);
                return Ok(false);
            }

            if matches!(key, Key::CtrlArrowRight) {
                self.move_tab_right(self.active_doc);
                return Ok(false);
            }
        }

        if matches!(key, Key::ShiftTab) {
            self.focus_prev();
            return Ok(false);
        }

        if matches!(key, Key::Tab) {
            self.focus_next();
            return Ok(false);
        }

        if self.sidebar_visible && self.focus == Focus::Sidebar {
            self.handle_sidebar_key(key);
            return Ok(false);
        }

        if self.focus == Focus::RightPanel {
            self.handle_right_panel_key(key);
            return Ok(false);
        }

        if self.focus == Focus::Tabs {
            self.handle_tabs_key(key);
            return Ok(false);
        }

        self.doc_mut().handle_key(key)?;
        if self.autosave_on_edit && is_edit_key(key) {
            let should_save = self.doc().dirty && self.doc().path.is_some();
            if should_save {
                if let Err(err) = self.doc_mut().save() {
                    self.tip = Some(format!("{}: {err}", texts(self.language).error_prefix));
                } else {
                    self.tip = Some(texts(self.language).tip_autosaved.to_string());
                }
            }
        }
        Ok(false)
    }

    fn handle_sidebar_key(&mut self, key: Key) {
        if !matches!(key, Key::Delete) {
            self.pending_delete_path = None;
        }

        match key {
            Key::ArrowUp => {
                if self.tree_sel > 0 {
                    self.tree_sel -= 1;
                }
            }
            Key::ArrowDown => {
                if self.tree_sel + 1 < self.tree.len() {
                    self.tree_sel += 1;
                }
            }
            Key::Home => self.tree_sel = 0,
            Key::End => {
                if !self.tree.is_empty() {
                    self.tree_sel = self.tree.len() - 1;
                }
            }
            Key::PageUp => {
                let step = 8usize;
                self.tree_sel = self.tree_sel.saturating_sub(step);
            }
            Key::PageDown => {
                let step = 8usize;
                self.tree_sel = (self.tree_sel + step).min(self.tree.len().saturating_sub(1));
            }
            Key::Enter => {
                self.open_selected_file();
            }
            Key::ArrowRight => {
                self.open_selected_file();
            }
            Key::Delete => {
                self.delete_selected_entry();
            }
            Key::CtrlN => {
                self.start_create_prompt(false);
            }
            Key::Char('M') | Key::Char('m') => {
                self.sidebar_menu_open = true;
                self.sidebar_menu_sel = 0;
            }
            Key::Char('P') | Key::Char('p') => {
                self.toggle_pick_drop_move();
            }
            Key::Char('G') | Key::Char('g') => {
                self.llm_prompt = Some(LlmPromptState::default());
                self.focus = Focus::Editor;
            }
            Key::Char('H') | Key::Char('h') => {
                self.llm_history_view = Some(LlmHistoryViewState::default());
            }
            Key::Char('E') | Key::Char('e') => {
                self.agent_events_view = Some(AgentEventsViewState::default());
            }
            Key::Char('U') | Key::Char('u') => {
                if self.agent_allow_unsafe_tools {
                    self.agent_allow_unsafe_tools = false;
                    self.tip = Some(texts(self.language).agent_unsafe_disabled.to_string());
                } else {
                    self.agent_unsafe_confirm = true;
                    self.tip = Some(texts(self.language).agent_unsafe_confirm_tip.to_string());
                }
            }
            Key::Char('L') | Key::Char('l') => {
                self.run_agent_loop_mvp();
            }
            Key::Char('C') | Key::Char('c') => {
                self.llm_history.clear();
                self.tip = Some(texts(self.language).llm_history_cleared.to_string());
            }
            Key::Char('I') | Key::Char('i') => {
                self.insert_last_llm_answer();
            }
            _ => {}
        }
    }

    fn toggle_pick_drop_move(&mut self) {
        let Some(selected) = self.tree.get(self.tree_sel) else {
            return;
        };

        let selected_path = selected.path.clone();
        if self.move_pick_path.is_none() {
            self.move_pick_path = Some(selected_path);
            self.tip = Some(texts(self.language).tip_move_picked.to_string());
            return;
        }

        let Some(from) = self.move_pick_path.as_ref().cloned() else {
            return;
        };

        if from == selected_path {
            self.move_pick_path = None;
            self.tip = Some(texts(self.language).tip_move_cancelled.to_string());
            return;
        }

        let mut target_dir = if selected.is_dir {
            selected.path.clone()
        } else {
            selected.path.parent().map(Path::to_path_buf).unwrap_or_else(|| self.project_root.clone())
        };

        if !target_dir.is_absolute() {
            target_dir = self.project_root.join(target_dir);
        }

        let Some(name) = from.file_name() else {
            self.move_pick_path = None;
            self.tip = Some(texts(self.language).tip_invalid_source.to_string());
            return;
        };

        let to = target_dir.join(name);
        if to == from {
            self.move_pick_path = None;
            self.tip = Some(texts(self.language).tip_source_target_same.to_string());
            return;
        }

        if to.exists() {
            self.move_pick_path = None;
            self.tip = Some(texts(self.language).tip_target_exists.to_string());
            return;
        }

        match fs::rename(&from, &to) {
            Ok(_) => {
                self.move_pick_path = None;
                self.remap_open_documents_after_move(&from, &to);
                if let Err(err) = self.refresh_tree(Some(to)) {
                    self.tip = Some(format!("{}: {err}", texts(self.language).error_prefix));
                } else {
                    self.tip = Some(texts(self.language).tip_moved.to_string());
                }
            }
            Err(err) => {
                self.move_pick_path = None;
                self.tip = Some(format!("{}: {err}", texts(self.language).error_prefix));
            }
        }
    }

    fn handle_tabs_key(&mut self, key: Key) {
        if self.docs.is_empty() {
            return;
        }

        match key {
            Key::ArrowLeft => {
                if self.tab_sel == 0 {
                    self.tab_sel = self.docs.len().saturating_sub(1);
                } else {
                    self.tab_sel -= 1;
                }
            }
            Key::ArrowRight => {
                self.tab_sel = (self.tab_sel + 1) % self.docs.len();
            }
            Key::Home => self.tab_sel = 0,
            Key::End => self.tab_sel = self.docs.len().saturating_sub(1),
            Key::Enter => {
                self.active_doc = self.tab_sel.min(self.docs.len().saturating_sub(1));
                self.persist_session();
            }
            Key::Delete | Key::CtrlW => {
                self.close_tab(self.tab_sel);
            }
            Key::CtrlJ => {
                self.move_tab_left(self.tab_sel);
            }
            Key::CtrlN => {
                self.move_tab_right(self.tab_sel);
            }
            _ => {}
        }
    }

    fn move_tab_left(&mut self, idx: usize) {
        if self.docs.len() < 2 || idx == 0 || idx >= self.docs.len() {
            return;
        }

        self.docs.swap(idx, idx - 1);
        if self.active_doc == idx {
            self.active_doc = idx - 1;
        } else if self.active_doc == idx - 1 {
            self.active_doc = idx;
        }

        self.tab_sel = idx - 1;
        self.persist_session();
    }

    fn move_tab_right(&mut self, idx: usize) {
        if self.docs.len() < 2 || idx + 1 >= self.docs.len() {
            return;
        }

        self.docs.swap(idx, idx + 1);
        if self.active_doc == idx {
            self.active_doc = idx + 1;
        } else if self.active_doc == idx + 1 {
            self.active_doc = idx;
        }

        self.tab_sel = idx + 1;
        self.persist_session();
    }

    fn handle_sidebar_menu_key(&mut self, key: Key) {
        let total = SidebarAction::all().len();
        match key {
            Key::Esc | Key::Tab | Key::ShiftTab => {
                self.sidebar_menu_open = false;
            }
            Key::ArrowUp => {
                self.sidebar_menu_sel = self.sidebar_menu_sel.saturating_sub(1);
            }
            Key::ArrowDown => {
                if self.sidebar_menu_sel + 1 < total {
                    self.sidebar_menu_sel += 1;
                }
            }
            Key::Home => self.sidebar_menu_sel = 0,
            Key::End => self.sidebar_menu_sel = total.saturating_sub(1),
            Key::Enter => {
                let action = SidebarAction::all()
                    .get(self.sidebar_menu_sel)
                    .copied()
                    .unwrap_or(SidebarAction::Open);
                self.sidebar_menu_open = false;
                self.run_sidebar_action(action);
            }
            Key::Char('o') | Key::Char('O') => {
                self.sidebar_menu_open = false;
                self.run_sidebar_action(SidebarAction::Open);
            }
            Key::Char('f') | Key::Char('F') => {
                self.sidebar_menu_open = false;
                self.run_sidebar_action(SidebarAction::NewFile);
            }
            Key::Char('d') | Key::Char('D') => {
                self.sidebar_menu_open = false;
                self.run_sidebar_action(SidebarAction::NewFolder);
            }
            Key::Char('v') | Key::Char('V') => {
                self.sidebar_menu_open = false;
                self.run_sidebar_action(SidebarAction::Move);
            }
            Key::Char('r') | Key::Char('R') => {
                self.sidebar_menu_open = false;
                self.run_sidebar_action(SidebarAction::Rename);
            }
            Key::Char('x') | Key::Char('X') => {
                self.sidebar_menu_open = false;
                self.run_sidebar_action(SidebarAction::Delete);
            }
            Key::Char('u') | Key::Char('U') => {
                self.sidebar_menu_open = false;
                self.run_sidebar_action(SidebarAction::Refresh);
            }
            _ => {}
        }
    }

    fn run_sidebar_action(&mut self, action: SidebarAction) {
        match action {
            SidebarAction::Open => self.open_selected_file(),
            SidebarAction::NewFile => self.start_create_prompt(false),
            SidebarAction::NewFolder => self.start_create_prompt(true),
            SidebarAction::Move => self.start_move_prompt(),
            SidebarAction::Rename => self.start_rename_prompt(),
            SidebarAction::Delete => self.delete_selected_entry(),
            SidebarAction::Refresh => {
                if let Err(err) = self.refresh_tree(None) {
                    self.tip = Some(format!("{}: {err}", texts(self.language).error_prefix));
                }
            }
        }
    }

    fn handle_sidebar_prompt_key(&mut self, key: Key) {
        let Some(prompt) = self.sidebar_prompt.as_mut() else {
            return;
        };

        match key {
            Key::Esc => {
                self.sidebar_prompt = None;
            }
            Key::Backspace => {
                prompt.input.pop();
            }
            Key::Enter => {
                let kind = prompt.kind;
                let input = prompt.input.trim().to_string();
                let base_dir = prompt.base_dir.clone();
                let target_path = prompt.target_path.clone();
                self.sidebar_prompt = None;

                if input.is_empty() {
                    self.tip = Some(texts(self.language).tip_name_empty.to_string());
                    return;
                }

                let result = match kind {
                    SidebarPromptKind::CreateFile => {
                        let p = base_dir.join(&input);
                        fs::File::create(&p).map(|_| p)
                    }
                    SidebarPromptKind::CreateFolder => {
                        let p = base_dir.join(&input);
                        fs::create_dir_all(&p).map(|_| p)
                    }
                    SidebarPromptKind::Move => {
                        let Some(from) = target_path else {
                            self.tip = Some(texts(self.language).tip_no_selected_target.to_string());
                            return;
                        };

                        let mut to = PathBuf::from(&input);
                        if !to.is_absolute() {
                            to = self.project_root.join(to);
                        }

                        if to.is_dir() {
                            if let Some(name) = from.file_name() {
                                to = to.join(name);
                            }
                        }

                        if to.exists() {
                            self.tip = Some(texts(self.language).tip_target_exists.to_string());
                            return;
                        }

                        fs::rename(&from, &to).map(|_| {
                            self.remap_open_documents_after_move(&from, &to);
                            to
                        })
                    }
                    SidebarPromptKind::Rename => {
                        let Some(from) = target_path else {
                            self.tip = Some(texts(self.language).tip_no_selected_target.to_string());
                            return;
                        };
                        let to = base_dir.join(&input);
                        fs::rename(&from, &to).map(|_| to)
                    }
                };

                match result {
                    Ok(select_path) => {
                        if let Err(err) = self.refresh_tree(Some(select_path.clone())) {
                            self.tip = Some(format!("{}: {err}", texts(self.language).error_prefix));
                        } else if kind == SidebarPromptKind::CreateFile {
                            if let Err(err) = self.open_file_in_tab(select_path.clone()) {
                                self.tip = Some(format!("{}: {err}", texts(self.language).error_prefix));
                            } else {
                                self.focus = Focus::Editor;
                            }
                        }
                    }
                    Err(err) => {
                        self.tip = Some(format!("{}: {err}", texts(self.language).error_prefix));
                    }
                }
            }
            Key::Char(ch) => {
                prompt.input.push(ch);
            }
            _ => {}
        }
    }

    fn open_selected_file(&mut self) {
        if let Some(e) = self.tree.get(self.tree_sel) {
            if e.is_dir {
                return;
            }

            if self.doc().dirty {
                self.tip = Some(texts(self.language).save_or_quit_double.into());
                return;
            }

            if let Err(err) = self.open_file_in_tab(e.path.clone()) {
                self.tip = Some(format!("{}: {err}", texts(self.language).error_prefix));
            } else {
                self.focus = Focus::Editor;
            }
        }
    }

    fn open_file_in_tab(&mut self, path: PathBuf) -> io::Result<()> {
        if let Some(idx) = self.docs.iter().position(|d| d.path.as_ref().is_some_and(|p| p == &path)) {
            self.active_doc = idx;
            self.tab_sel = idx;
            self.persist_session();
            return Ok(());
        }

        let doc = Document::open_file(path)?;
        self.docs.push(doc);
        self.active_doc = self.docs.len().saturating_sub(1);
        self.tab_sel = self.active_doc;
        self.persist_session();
        Ok(())
    }

    fn next_tab(&mut self) {
        if self.docs.len() < 2 {
            return;
        }

        self.active_doc = (self.active_doc + 1) % self.docs.len();
        self.tab_sel = self.active_doc;
        self.persist_session();
    }

    fn prev_tab(&mut self) {
        if self.docs.len() < 2 {
            return;
        }

        self.active_doc = if self.active_doc == 0 {
            self.docs.len() - 1
        } else {
            self.active_doc - 1
        };

        self.tab_sel = self.active_doc;
        self.persist_session();
    }

    fn close_active_tab(&mut self) {
        self.close_tab(self.active_doc);
    }

    fn close_tab(&mut self, idx: usize) {
        if self.docs.is_empty() {
            self.docs.push(Document::empty());
            self.active_doc = 0;
            self.tab_sel = 0;
            return;
        }

        if idx >= self.docs.len() {
            return;
        }

        if self.docs[idx].pinned {
            self.tip = Some(texts(self.language).tip_tab_is_pinned.to_string());
            return;
        }

        if self.docs[idx].dirty {
            self.tip = Some(texts(self.language).save_or_quit_double.into());
            return;
        }

        let was_active = idx == self.active_doc;
        let old_active = self.active_doc;
        let old_tab_sel = self.tab_sel;
        self.docs.remove(idx);
        if self.docs.is_empty() {
            self.docs.push(Document::empty());
            self.active_doc = 0;
            self.tab_sel = 0;
        } else {
            // При закрытии активного таба предпочитаем правого соседа, иначе оставляем текущий активный таб
            self.active_doc = if was_active {
                idx.min(self.docs.len().saturating_sub(1))
            } else if idx < old_active {
                old_active.saturating_sub(1)
            } else {
                old_active.min(self.docs.len().saturating_sub(1))
            };

            self.tab_sel = if old_tab_sel == idx {
                self.active_doc
            } else if idx < old_tab_sel {
                old_tab_sel.saturating_sub(1)
            } else {
                old_tab_sel.min(self.docs.len().saturating_sub(1))
            };
        }
        self.persist_session();
    }

    fn close_document_by_path(&mut self, path: &Path) {
        if let Some(idx) = self.docs.iter().position(|d| d.path.as_ref().is_some_and(|p| p == path)) {
            let old_active = self.active_doc;
            let old_tab_sel = self.tab_sel;
            self.docs.remove(idx);
            if self.docs.is_empty() {
                self.docs.push(Document::empty());
                self.active_doc = 0;
                self.tab_sel = 0;
            } else {
                self.active_doc = if idx < old_active {
                    old_active.saturating_sub(1)
                } else {
                    old_active.min(self.docs.len().saturating_sub(1))
                };

                self.tab_sel = if idx < old_tab_sel {
                    old_tab_sel.saturating_sub(1)
                } else {
                    old_tab_sel.min(self.docs.len().saturating_sub(1))
                };
            }
            self.persist_session();
        }
    }

    fn persist_session(&self) {
        let tabs: Vec<PathBuf> = self.docs.iter().filter_map(|d| d.path.clone()).collect();
        let active = self.docs.get(self.active_doc).and_then(|d| d.path.as_ref());
        let pinned: Vec<PathBuf> = self.docs.iter().filter(|d| d.pinned).filter_map(|d| d.path.clone()).collect();
        let _ = session::save_project_session(&self.project_root, &tabs, active, &pinned);
    }

    fn toggle_pin_active_tab(&mut self) {
        if self.docs.is_empty() {
            return;
        }

        let idx = self.active_doc.min(self.docs.len().saturating_sub(1));
        let doc = &mut self.docs[idx];
        doc.pinned = !doc.pinned;
        self.tip = Some(if doc.pinned {
            texts(self.language).tip_tab_pinned.to_string()
        } else {
            texts(self.language).tip_tab_unpinned.to_string()
        });

        self.persist_session();
    }

    fn current_nav_location(&self) -> Option<NavLocation> {
        let doc = self.docs.get(self.active_doc)?;
        let path = doc.path.clone()?;
        Some(NavLocation {
            path,
            row: doc.row,
            col: doc.col,
        })
    }

    fn push_current_to_back_history(&mut self) {
        let Some(loc) = self.current_nav_location() else {
            return;
        };

        if self.nav_back.last() == Some(&loc) {
            return;
        }

        self.nav_back.push(loc);
        if self.nav_back.len() > 256 {
            let _ = self.nav_back.remove(0);
        }
    }

    fn jump_to_path_position(&mut self, path: PathBuf, row: usize, col: usize, record_history: bool) {
        if record_history {
            self.push_current_to_back_history();
            self.nav_forward.clear();
        }

        if let Err(err) = self.open_file_in_tab(path.clone()) {
            self.tip = Some(format!("{}: {err}", texts(self.language).error_prefix));
            return;
        }

        self.focus = Focus::Editor;
        {
            let doc = self.doc_mut();
            doc.row = row;
            doc.col = col;
            doc.clamp_cursor();
        }

        let _ = self.refresh_tree(Some(path));
    }

    fn jump_in_active_doc(&mut self, row: usize, col: usize, record_history: bool) {
        if record_history {
            self.push_current_to_back_history();
            self.nav_forward.clear();
        }

        let doc = self.doc_mut();
        doc.row = row.min(doc.buffer.line_count().saturating_sub(1));
        doc.col = col;
        doc.clamp_cursor();
        self.focus = Focus::Editor;
    }

    fn navigate_back(&mut self) {
        let Some(target) = self.nav_back.pop() else {
            self.tip = Some(texts(self.language).tip_no_back_history.to_string());
            return;
        };

        if let Some(cur) = self.current_nav_location() {
            if self.nav_forward.last() != Some(&cur) {
                self.nav_forward.push(cur);
            }
        }

        self.jump_to_path_position(target.path, target.row, target.col, false);
    }

    fn navigate_forward(&mut self) {
        let Some(target) = self.nav_forward.pop() else {
            self.tip = Some(texts(self.language).tip_no_forward_history.to_string());
            return;
        };

        if let Some(cur) = self.current_nav_location() {
            if self.nav_back.last() != Some(&cur) {
                self.nav_back.push(cur);
            }
        }

        self.jump_to_path_position(target.path, target.row, target.col, false);
    }

    fn start_create_prompt(&mut self, folder: bool) {
        let Some(base_dir) = self.selected_parent_dir() else {
            self.tip = Some(texts(self.language).tip_folder_not_found.to_string());
            return;
        };

        self.sidebar_prompt = Some(SidebarPrompt {
            kind: if folder {
                SidebarPromptKind::CreateFolder
            } else {
                SidebarPromptKind::CreateFile
            },
            base_dir,
            target_path: None,
            input: String::new(),
        });
    }

    fn start_rename_prompt(&mut self) {
        let Some(selected) = self.tree.get(self.tree_sel) else {
            return;
        };

        let base_dir = selected
            .path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| self.project_root.clone());
        self.sidebar_prompt = Some(SidebarPrompt {
            kind: SidebarPromptKind::Rename,
            base_dir,
            target_path: Some(selected.path.clone()),
            input: selected.label.clone(),
        });
    }

    fn start_move_prompt(&mut self) {
        let Some(selected) = self.tree.get(self.tree_sel) else {
            return;
        };

        let default_input = selected
            .path
            .strip_prefix(&self.project_root)
            .ok()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| selected.path.to_string_lossy().to_string());

        self.sidebar_prompt = Some(SidebarPrompt {
            kind: SidebarPromptKind::Move,
            base_dir: self.project_root.clone(),
            target_path: Some(selected.path.clone()),
            input: default_input,
        });
    }

    fn remap_open_documents_after_move(&mut self, from: &Path, to: &Path) {
        for doc in &mut self.docs {
            let Some(path) = doc.path.as_ref().cloned() else {
                continue;
            };

            if &path == from {
                doc.path = Some(to.to_path_buf());
                continue;
            }

            if let Ok(rel) = path.strip_prefix(from) {
                doc.path = Some(to.join(rel));
            }
        }
    }

    fn delete_selected_entry(&mut self) {
        let Some(selected) = self.tree.get(self.tree_sel) else {
            return;
        };

        let selected_path = selected.path.clone();
        let is_dir = selected.is_dir;
        if self.pending_delete_path.as_ref() != Some(&selected_path) {
            self.pending_delete_path = Some(selected_path.clone());
            self.tip = Some(texts(self.language).tip_delete_confirm.to_string());
            return;
        }

        if self.doc().dirty && self.doc().path.as_ref() == Some(&selected_path) {
            self.tip = Some(texts(self.language).save_or_quit_double.into());
            return;
        }

        let res = if is_dir {
            fs::remove_dir_all(&selected_path)
        } else {
            fs::remove_file(&selected_path)
        };

        match res {
            Ok(_) => {
                self.pending_delete_path = None;
                if self.doc().path.as_ref() == Some(&selected_path) {
                    self.close_document_by_path(&selected_path);
                }

                if let Err(err) = self.refresh_tree(None) {
                    self.tip = Some(format!("{}: {err}", texts(self.language).error_prefix));
                }
            }
            Err(err) => {
                self.pending_delete_path = None;
                self.tip = Some(format!("{}: {err}", texts(self.language).error_prefix));
            }
        }
    }

    fn refresh_tree(&mut self, select_path: Option<PathBuf>) -> io::Result<()> {
        let old_selected = self.tree.get(self.tree_sel).map(|e| e.path.clone());
        self.tree = filetree::build_tree(&self.project_root)?;

        if self.tree.is_empty() {
            self.tree_sel = 0;
            self.tree_scroll = 0;
            return Ok(());
        }

        let target = select_path.or(old_selected);
        if let Some(target) = target {
            if let Some(idx) = self.tree.iter().position(|e| e.path == target) {
                self.tree_sel = idx;
            } else {
                self.tree_sel = self.tree_sel.min(self.tree.len().saturating_sub(1));
            }
        } else {
            self.tree_sel = self.tree_sel.min(self.tree.len().saturating_sub(1));
        }

        Ok(())
    }

    fn selected_parent_dir(&self) -> Option<PathBuf> {
        if let Some(sel) = self.tree.get(self.tree_sel) {
            if sel.is_dir {
                Some(sel.path.clone())
            } else {
                sel.path.parent().map(Path::to_path_buf)
            }
        } else {
            Some(self.project_root.clone())
        }
    }

    fn handle_quick_open_key(&mut self, key: Key) {
        if self.quick_open.is_none() {
            return;
        }
        match key {
            Key::Esc | Key::CtrlO => {
                self.quick_open = None;
                return;
            }
            Key::Backspace => {
                let Some(state) = self.quick_open.as_mut() else {
                    return;
                };
                state.query.pop();
                state.sel = 0;
                return;
            }
            Key::Char(ch) => {
                let Some(state) = self.quick_open.as_mut() else {
                    return;
                };
                state.query.push(ch);
                state.sel = 0;
                return;
            }
            _ => {}
        }

        let matches = self.quick_open_matches();
        if matches.is_empty() {
            if matches!(key, Key::Enter) {
                self.quick_open = None;
            }
            return;
        }

        match key {
            Key::ArrowUp => {
                let Some(state) = self.quick_open.as_mut() else {
                    return;
                };
                state.sel = state.sel.saturating_sub(1);
            }
            Key::ArrowDown => {
                let Some(state) = self.quick_open.as_mut() else {
                    return;
                };
                state.sel = (state.sel + 1).min(matches.len().saturating_sub(1));
            }
            Key::Home => {
                let Some(state) = self.quick_open.as_mut() else {
                    return;
                };
                state.sel = 0;
            }
            Key::End => {
                let Some(state) = self.quick_open.as_mut() else {
                    return;
                };
                state.sel = matches.len().saturating_sub(1);
            }
            Key::Enter => {
                let Some(state) = self.quick_open.as_ref() else {
                    return;
                };
                let idx = state.sel.min(matches.len().saturating_sub(1));
                let path = matches[idx].clone();
                self.jump_to_path_position(path, 0, 0, true);
                self.quick_open = None;
            }
            _ => {}
        }
    }

    fn handle_in_file_find_key(&mut self, key: Key) {
        if self.in_file_find.is_none() {
            return;
        }

        match key {
            Key::Esc | Key::CtrlBackslash => {
                self.in_file_find = None;
                return;
            }
            Key::Backspace => {
                if let Some(st) = self.in_file_find.as_mut() {
                    st.query.pop();
                    st.sel = 0;
                }
                return;
            }
            Key::Char(ch) => {
                if let Some(st) = self.in_file_find.as_mut() {
                    st.query.push(ch);
                    st.sel = 0;
                }
                return;
            }
            _ => {}
        }

        let matches = self.in_file_find_matches();
        match key {
            Key::Tab | Key::ArrowDown => {
                if let Some(st) = self.in_file_find.as_mut() {
                    if matches.is_empty() {
                        return;
                    }
                    st.sel = (st.sel + 1) % matches.len();
                }
            }
            Key::ShiftTab | Key::ArrowUp => {
                if let Some(st) = self.in_file_find.as_mut() {
                    if matches.is_empty() {
                        return;
                    }
                    st.sel = st.sel.checked_sub(1).unwrap_or(matches.len().saturating_sub(1));
                }
            }
            Key::Home => {
                if let Some(st) = self.in_file_find.as_mut() {
                    st.sel = 0;
                }
            }
            Key::End => {
                if let Some(st) = self.in_file_find.as_mut() {
                    st.sel = matches.len().saturating_sub(1);
                }
            }
            Key::Enter => {
                if matches.is_empty() {
                    if self
                        .in_file_find
                        .as_ref()
                        .is_some_and(|s| s.query.is_empty())
                    {
                        self.in_file_find = None;
                    } else {
                        self.tip = Some(texts(self.language).tip_no_in_file_matches.to_string());
                    }
                    return;
                }
                let idx = self
                    .in_file_find
                    .as_ref()
                    .map(|s| s.sel)
                    .unwrap_or(0)
                    .min(matches.len().saturating_sub(1));
                let (row, col) = matches[idx];
                self.in_file_find = None;
                self.jump_in_active_doc(row, col, true);
            }
            _ => {}
        }
    }

    fn in_file_find_matches(&self) -> Vec<(usize, usize)> {
        let Some(state) = self.in_file_find.as_ref() else {
            return Vec::new();
        };

        Self::buffer_match_positions(self.doc(), &state.query)
    }

    fn buffer_match_positions(doc: &Document, query: &str) -> Vec<(usize, usize)> {
        if query.is_empty() {
            return Vec::new();
        }

        let needle: Vec<char> = query.chars().flat_map(|c| c.to_lowercase()).collect();
        let n = needle.len();
        if n == 0 {
            return Vec::new();
        }

        let mut out = Vec::new();
        for (ri, line) in doc.buffer.lines().iter().enumerate() {
            let line_l: Vec<char> = line.chars().flat_map(|c| c.to_lowercase()).collect();
            if line_l.len() < n {
                continue;
            }

            let last = line_l.len() - n;
            for i in 0..=last {
                if line_l[i..i + n] == needle[..] {
                    out.push((ri, i));
                    if out.len() >= 2000 {
                        return out;
                    }
                }
            }
        }
        out
    }

    fn quick_open_matches(&self) -> Vec<PathBuf> {
        let mut files: Vec<PathBuf> = self
            .tree
            .iter()
            .filter(|e| !e.is_dir)
            .map(|e| e.path.clone())
            .collect();
        if let Some(state) = &self.quick_open {
            let q = state.query.to_lowercase();
            if !q.is_empty() {
                files.retain(|path| {
                    let p = path.to_string_lossy().to_lowercase();
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_lowercase())
                        .unwrap_or_default();
                    p.contains(&q) || name.contains(&q)
                });
            }
            files.sort_by_key(|path| {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_lowercase())
                    .unwrap_or_default();
                let starts = if q.is_empty() || name.starts_with(&q) { 0 } else { 1 };
                (starts, name)
            });
        }
        files.truncate(8);
        files
    }

    fn handle_project_search_key(&mut self, key: Key) {
        if self.project_search.is_none() {
            return;
        }
        match key {
            Key::Esc | Key::CtrlF => {
                self.project_search = None;
                return;
            }
            Key::Backspace => {
                let Some(state) = self.project_search.as_mut() else {
                    return;
                };
                if state.edit_replacement {
                    state.replacement.pop();
                } else {
                    state.query.pop();
                }
                state.sel = 0;
                state.confirm_replace_all = false;
                return;
            }
            Key::Char(ch) => {
                let Some(state) = self.project_search.as_mut() else {
                    return;
                };
                if state.edit_replacement {
                    state.replacement.push(ch);
                } else {
                    state.query.push(ch);
                }
                state.sel = 0;
                state.confirm_replace_all = false;
                return;
            }
            Key::Tab => {
                let Some(state) = self.project_search.as_mut() else {
                    return;
                };
                state.edit_replacement = !state.edit_replacement;
                state.confirm_replace_all = false;
                return;
            }
            Key::CtrlO => {
                let Some(state) = self.project_search.as_mut() else {
                    return;
                };
                state.regex_mode = !state.regex_mode;
                state.sel = 0;
                state.confirm_replace_all = false;
                return;
            }
            Key::CtrlR => {}
            Key::CtrlG => {}
            _ => {}
        }

        let matches = self.project_search_matches();
        if matches!(key, Key::CtrlG) {
            let Some(state) = self.project_search.as_ref() else {
                return;
            };
            if state.replacement.is_empty() {
                self.tip = Some(texts(self.language).tip_replace_text_first.to_string());
                return;
            }
            if matches.is_empty() {
                self.tip = Some(texts(self.language).tip_no_search_matches_replace.to_string());
                return;
            }
            let idx = state.sel.min(matches.len().saturating_sub(1));
            let hit = matches[idx].clone();
            let changed = self.apply_replace_current(&hit);
            if changed {
                self.tip = Some(texts(self.language).tip_replaced_current_match.to_string());
            } else {
                self.tip = Some(texts(self.language).tip_current_not_replaced.to_string());
            }
            return;
        }
        if matches!(key, Key::CtrlR) {
            let Some(state) = self.project_search.as_mut() else {
                return;
            };
            if state.replacement.is_empty() {
                state.edit_replacement = true;
                self.tip = Some(texts(self.language).tip_replace_first_then_ctrl_r.to_string());
                return;
            }
            if !state.confirm_replace_all {
                state.confirm_replace_all = true;
                self.tip = Some(texts(self.language).tip_press_enter_confirm_replace_all.to_string());
                return;
            }
        }

        match key {
            Key::ArrowUp => {
                let Some(state) = self.project_search.as_mut() else {
                    return;
                };
                state.sel = state.sel.saturating_sub(1);
            }
            Key::ArrowDown => {
                let Some(state) = self.project_search.as_mut() else {
                    return;
                };
                state.sel = (state.sel + 1).min(matches.len().saturating_sub(1));
            }
            Key::Home => {
                let Some(state) = self.project_search.as_mut() else {
                    return;
                };
                state.sel = 0;
            }
            Key::End => {
                let Some(state) = self.project_search.as_mut() else {
                    return;
                };
                state.sel = matches.len().saturating_sub(1);
            }
            Key::Enter => {
                let Some(state) = self.project_search.as_ref() else {
                    return;
                };
                if state.confirm_replace_all {
                    let (replaced, skipped_dirty) = self.apply_project_replace_all();
                    self.tip = Some(format!(
                        "Replaced {replaced} matches (skipped dirty tabs: {skipped_dirty})"
                    ));
                    self.project_search = None;
                    return;
                }
                if matches.is_empty() {
                    self.project_search = None;
                    return;
                }
                let idx = state.sel.min(matches.len().saturating_sub(1));
                let hit = &matches[idx];
                self.jump_to_path_position(hit.path.clone(), hit.line_idx, hit.col_idx, true);
                self.project_search = None;
            }
            _ => {}
        }
    }

    fn project_search_matches(&self) -> Vec<SearchMatch> {
        let Some(state) = &self.project_search else {
            return Vec::new();
        };
        if state.query.is_empty() {
            return Vec::new();
        }

        let regex = if state.regex_mode {
            match Regex::new(&state.query) {
                Ok(re) => Some(re),
                Err(_) => return Vec::new(),
            }
        } else {
            None
        };
        let q = state.query.to_lowercase();
        let mut out = Vec::<SearchMatch>::new();
        for entry in self.tree.iter().filter(|e| !e.is_dir) {
            let Ok(content) = fs::read_to_string(&entry.path) else {
                continue;
            };
            for (line_idx, line) in content.lines().enumerate() {
                let pos = if let Some(re) = &regex {
                    re.find(line).map(|m| line[..m.start()].chars().count())
                } else {
                    let line_lower = line.to_lowercase();
                    line_lower.find(&q).map(|idx| line[..idx].chars().count())
                };
                if let Some(col_idx) = pos {
                    out.push(SearchMatch {
                        path: entry.path.clone(),
                        line_idx,
                        col_idx,
                        preview: line.trim().to_string(),
                    });
                    if out.len() >= 40 {
                        return out;
                    }
                }
            }
        }
        out
    }

    fn apply_project_replace_all(&mut self) -> (usize, usize) {
        let Some(state) = self.project_search.as_ref() else {
            return (0, 0);
        };
        if state.query.is_empty() || state.replacement.is_empty() {
            return (0, 0);
        }
        let regex = if state.regex_mode {
            match Regex::new(&state.query) {
                Ok(re) => Some(re),
                Err(_) => return (0, 0),
            }
        } else {
            None
        };

        let mut replaced_total = 0usize;
        let mut skipped_dirty = 0usize;
        let file_paths: Vec<PathBuf> = self
            .tree
            .iter()
            .filter(|e| !e.is_dir)
            .map(|e| e.path.clone())
            .collect();
        for path in file_paths {
            if self
                .docs
                .iter()
                .any(|d| d.dirty && d.path.as_ref() == Some(&path))
            {
                skipped_dirty += 1;
                continue;
            }
            let Ok(content) = fs::read_to_string(&path) else {
                continue;
            };
            let (new_content, count) = if let Some(re) = &regex {
                let cnt = re.find_iter(&content).count();
                if cnt == 0 {
                    (content, 0)
                } else {
                    (re.replace_all(&content, state.replacement.as_str()).to_string(), cnt)
                }
            } else {
                let cnt = content.matches(&state.query).count();
                if cnt == 0 {
                    (content, 0)
                } else {
                    (content.replace(&state.query, &state.replacement), cnt)
                }
            };
            if count == 0 {
                continue;
            }
            if fs::write(&path, new_content).is_ok() {
                replaced_total += count;
                if let Some(idx) = self
                    .docs
                    .iter()
                    .position(|d| !d.dirty && d.path.as_ref() == Some(&path))
                {
                    if let Ok(reloaded) = Document::open_file(path.clone()) {
                        self.docs[idx] = reloaded;
                    }
                }
            }
        }
        (replaced_total, skipped_dirty)
    }

    fn apply_replace_current(&mut self, hit: &SearchMatch) -> bool {
        let Some(state) = self.project_search.as_ref() else {
            return false;
        };
        if state.query.is_empty() || state.replacement.is_empty() {
            return false;
        }
        if self
            .docs
            .iter()
            .any(|d| d.dirty && d.path.as_ref() == Some(&hit.path))
        {
            return false;
        }
        let Ok(content) = fs::read_to_string(&hit.path) else {
            return false;
        };
        let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
        if hit.line_idx >= lines.len() {
            return false;
        }
        let line = &mut lines[hit.line_idx];
        let updated = if state.regex_mode {
            match Regex::new(&state.query) {
                Ok(re) => re.replacen(line, 1, state.replacement.as_str()).to_string(),
                Err(_) => return false,
            }
        } else if let Some(pos) = line.find(&state.query) {
            let mut next = String::new();
            next.push_str(&line[..pos]);
            next.push_str(&state.replacement);
            next.push_str(&line[pos + state.query.len()..]);
            next
        } else {
            return false;
        };
        if *line == updated {
            return false;
        }
        *line = updated;
        let body = lines.join("\n");
        if fs::write(&hit.path, body).is_err() {
            return false;
        }
        if let Some(idx) = self
            .docs
            .iter()
            .position(|d| !d.dirty && d.path.as_ref() == Some(&hit.path))
        {
            if let Ok(reloaded) = Document::open_file(hit.path.clone()) {
                self.docs[idx] = reloaded;
            }
        }
        true
    }

    fn handle_symbol_jump_key(&mut self, key: Key) {
        if self.symbol_jump.is_none() {
            return;
        }
        match key {
            Key::Esc | Key::CtrlT => {
                self.symbol_jump = None;
                return;
            }
            Key::Backspace => {
                if let Some(state) = self.symbol_jump.as_mut() {
                    state.query.pop();
                    state.sel = 0;
                }
                return;
            }
            Key::Char(ch) => {
                if let Some(state) = self.symbol_jump.as_mut() {
                    state.query.push(ch);
                    state.sel = 0;
                }
                return;
            }
            _ => {}
        }

        let matches = self.symbol_jump_matches();
        match key {
            Key::ArrowUp => {
                if let Some(state) = self.symbol_jump.as_mut() {
                    state.sel = state.sel.saturating_sub(1);
                }
            }
            Key::ArrowDown => {
                if let Some(state) = self.symbol_jump.as_mut() {
                    state.sel = (state.sel + 1).min(matches.len().saturating_sub(1));
                }
            }
            Key::Home => {
                if let Some(state) = self.symbol_jump.as_mut() {
                    state.sel = 0;
                }
            }
            Key::End => {
                if let Some(state) = self.symbol_jump.as_mut() {
                    state.sel = matches.len().saturating_sub(1);
                }
            }
            Key::Enter => {
                if matches.is_empty() {
                    self.symbol_jump = None;
                    return;
                }
                let idx = self
                    .symbol_jump
                    .as_ref()
                    .map(|s| s.sel.min(matches.len().saturating_sub(1)))
                    .unwrap_or(0);
                let item = &matches[idx];
                self.jump_to_path_position(item.path.clone(), item.line_idx, 0, true);
                self.symbol_jump = None;
            }
            _ => {}
        }
    }

    fn handle_go_to_line_key(&mut self, key: Key) {
        let Some(state) = self.go_to_line.as_mut() else {
            return;
        };
        match key {
            Key::Esc | Key::CtrlY => {
                self.go_to_line = None;
            }
            Key::Backspace => {
                state.input.pop();
            }
            Key::Char(ch) => {
                if ch.is_ascii_digit() {
                    state.input.push(ch);
                }
            }
            Key::Enter => {
                let line = state.input.trim().parse::<usize>().ok().unwrap_or(1);
                let target = line.saturating_sub(1);
                self.jump_in_active_doc(target, 0, true);
                self.go_to_line = None;
            }
            _ => {}
        }
    }

    fn handle_llm_prompt_key(&mut self, key: Key) {
        let Some(state) = self.llm_prompt.as_mut() else {
            return;
        };
        match key {
            Key::Esc | Key::CtrlY => {
                self.llm_prompt = None;
                self.focus = Focus::Editor;
            }
            Key::Backspace => {
                state.input.pop();
            }
            Key::Char(ch) => {
                state.input.push(ch);
            }
            Key::Enter => {
                let prompt = state.input.trim().to_string();
                self.llm_prompt = None;
                self.focus = Focus::Editor;
                if prompt.is_empty() {
                    self.tip = Some(texts(self.language).llm_prompt_empty.to_string());
                    return;
                }
                self.run_llm_prompt(prompt);
            }
            _ => {}
        }
    }

    fn handle_multi_edit_key(&mut self, key: Key) {
        let Some(state) = self.multi_edit.as_mut() else {
            return;
        };
        match key {
            Key::Esc | Key::CtrlD => {
                self.multi_edit = None;
            }
            Key::Backspace => {
                state.replacement.pop();
            }
            Key::Char(ch) => {
                state.replacement.push(ch);
            }
            Key::Enter => {
                let target = state.target.clone();
                let replacement = state.replacement.clone();
                let count = self.replace_word_in_current_doc(&target, &replacement);
                if count == 0 {
                    self.tip = Some(texts(self.language).tip_no_occurrences_replaced.to_string());
                } else {
                    self.tip = Some(texts(self.language).tip_multi_edit_replaced_fmt.replace("{}", &count.to_string()));
                }
                self.multi_edit = None;
            }
            _ => {}
        }
    }

    fn word_under_cursor(&self) -> String {
        let doc = self.doc();
        let line = doc
            .buffer
            .lines()
            .get(doc.row)
            .map(String::as_str)
            .unwrap_or("");
        let chars: Vec<char> = line.chars().collect();
        if chars.is_empty() {
            return String::new();
        }
        let cur = doc.col.min(chars.len().saturating_sub(1));
        let mut left = cur;
        while left > 0 && is_word_char(chars[left - 1]) {
            left -= 1;
        }
        let mut right = cur;
        if !is_word_char(chars[right]) && right + 1 < chars.len() && is_word_char(chars[right + 1]) {
            right += 1;
            left = right;
            while left > 0 && is_word_char(chars[left - 1]) {
                left -= 1;
            }
        }
        while right < chars.len() && is_word_char(chars[right]) {
            right += 1;
        }
        if left >= right {
            return String::new();
        }
        chars[left..right].iter().collect()
    }

    fn replace_word_in_current_doc(&mut self, target: &str, replacement: &str) -> usize {
        if target.is_empty() {
            return 0;
        }
        let doc = self.doc_mut();
        let mut replaced = 0usize;
        let mut lines = Vec::<String>::new();
        for line in doc.buffer.lines() {
            replaced += line.matches(target).count();
            lines.push(line.replace(target, replacement));
        }
        if replaced > 0 {
            doc.buffer = Buffer::from_file(&lines.join("\n"));
            doc.dirty = true;
            doc.clamp_cursor();
        }
        replaced
    }

    fn start_sync_edit(&mut self) {
        let target = self.word_under_cursor();
        if target.is_empty() {
            self.tip = Some(texts(self.language).tip_no_word_under_cursor.to_string());
            return;
        }
        let original_text = self.doc().buffer.to_file_string();
        let re = whole_word_regex(&target);
        let Some(re) = re else {
            self.tip = Some(texts(self.language).tip_cannot_start_sync_edit.to_string());
            return;
        };
        let occurrences = re.find_iter(&original_text).count();
        if occurrences == 0 {
            self.tip = Some(texts(self.language).tip_no_occurrences_found.to_string());
            return;
        }
        self.sync_edit = Some(SyncEditState {
            replacement: target.clone(),
            target,
            original_text,
            occurrences,
        });
    }

    fn handle_sync_edit_key(&mut self, key: Key) {
        let Some(state) = self.sync_edit.as_mut() else {
            return;
        };
        match key {
            Key::Esc => {
                let original = state.original_text.clone();
                {
                    let doc = self.doc_mut();
                    doc.buffer = Buffer::from_file(&original);
                    doc.clamp_cursor();
                }
                self.sync_edit = None;
            }
            Key::Enter | Key::CtrlE => {
                self.sync_edit = None;
                self.doc_mut().dirty = true;
            }
            Key::Backspace => {
                state.replacement.pop();
                self.apply_sync_edit_preview();
            }
            Key::Char(ch) => {
                state.replacement.push(ch);
                self.apply_sync_edit_preview();
            }
            _ => {}
        }
    }

    fn apply_sync_edit_preview(&mut self) {
        let Some(state) = self.sync_edit.as_ref() else {
            return;
        };
        let Some(re) = whole_word_regex(&state.target) else {
            return;
        };
        let replaced = re
            .replace_all(&state.original_text, state.replacement.as_str())
            .to_string();
        let doc = self.doc_mut();
        doc.buffer = Buffer::from_file(&replaced);
        doc.clamp_cursor();
    }

    fn command_palette_items(&self) -> Vec<(&'static str, &'static str)> {
        let llm_enabled = Self::llm_enabled_in_settings();
        let mut items = vec![
            (texts(self.language).cmd_toggle_sidebar, "toggle_sidebar"),
            (texts(self.language).cmd_toggle_right_panel, "toggle_right_panel"),
            (texts(self.language).cmd_toggle_theme, "toggle_theme"),
            (texts(self.language).cmd_toggle_autosave, "toggle_autosave"),
            (texts(self.language).cmd_rust_check_current, "rust_check_current"),
            (texts(self.language).cmd_show_diagnostics, "show_diagnostics"),
            (texts(self.language).cmd_increase_font, "font_plus"),
            (texts(self.language).cmd_decrease_font, "font_minus"),
            (texts(self.language).cmd_toggle_line_spacing, "toggle_line_spacing"),
            (texts(self.language).cmd_toggle_ligatures, "toggle_ligatures"),
            (texts(self.language).cmd_quick_open_file, "quick_open"),
            (texts(self.language).cmd_search_in_project, "project_search"),
            (texts(self.language).cmd_go_to_symbol, "go_symbol"),
            (texts(self.language).cmd_go_to_line, "go_line"),
            (texts(self.language).cmd_toggle_pin_tab, "toggle_pin"),
            (texts(self.language).cmd_show_hotkeys, "show_help"),
            (texts(self.language).cmd_language_picker, "language_picker"),
            (texts(self.language).cmd_lsp_wave, "lsp_wave_extensions"),
            (texts(self.language).cmd_find_in_file, "in_file_find"),
            (texts(self.language).cmd_git_status, "git_status"),
            (texts(self.language).cmd_git_diff_unstaged, "git_diff_unstaged"),
            (texts(self.language).cmd_git_diff_staged, "git_diff_staged"),
            (texts(self.language).cmd_git_recent_commits, "git_log"),
        ];
        if llm_enabled {
            items.extend_from_slice(&[
                (texts(self.language).cmd_llm_ask, "llm_ask"),
                (texts(self.language).cmd_llm_show_history, "llm_history"),
                (texts(self.language).cmd_agent_show_events, "agent_events"),
                (texts(self.language).cmd_agent_toggle_unsafe_tools, "agent_toggle_unsafe_tools"),
                (texts(self.language).cmd_llm_clear_history, "llm_history_clear"),
                (texts(self.language).cmd_llm_insert_last_answer, "llm_insert_last_answer"),
                (texts(self.language).cmd_llm_health_check, "llm_health"),
                (texts(self.language).cmd_llm_explain_current_line, "llm_explain_current_line"),
                (texts(self.language).cmd_agent_run_loop, "agent_run_loop"),
            ]);
        }
        if let Some(state) = &self.command_palette {
            let q = state.query.to_lowercase();
            if !q.is_empty() {
                items.retain(|(title, _)| title.to_lowercase().contains(&q));
            }
        }
        items
    }

    fn handle_command_palette_key(&mut self, key: Key) {
        if self.command_palette.is_none() {
            return;
        }
        match key {
            Key::Esc | Key::CtrlJ => {
                self.command_palette = None;
                return;
            }
            Key::Backspace => {
                let Some(state) = self.command_palette.as_mut() else {
                    return;
                };
                state.query.pop();
                state.sel = 0;
                return;
            }
            Key::Char(ch) => {
                let Some(state) = self.command_palette.as_mut() else {
                    return;
                };
                state.query.push(ch);
                state.sel = 0;
                return;
            }
            _ => {}
        }
        let items = self.command_palette_items();
        match key {
            Key::ArrowUp => {
                if let Some(state) = self.command_palette.as_mut() {
                    state.sel = state.sel.saturating_sub(1);
                }
            }
            Key::ArrowDown => {
                if let Some(state) = self.command_palette.as_mut() {
                    state.sel = (state.sel + 1).min(items.len().saturating_sub(1));
                }
            }
            Key::Home => {
                if let Some(state) = self.command_palette.as_mut() {
                    state.sel = 0;
                }
            }
            Key::End => {
                if let Some(state) = self.command_palette.as_mut() {
                    state.sel = items.len().saturating_sub(1);
                }
            }
            Key::Enter => {
                if items.is_empty() {
                    self.command_palette = None;
                    return;
                }
                let idx = self
                    .command_palette
                    .as_ref()
                    .map(|s| s.sel.min(items.len().saturating_sub(1)))
                    .unwrap_or(0);
                let cmd = items[idx].1;
                self.command_palette = None;
                self.run_palette_command(cmd);
            }
            _ => {}
        }
    }

    fn run_palette_command(&mut self, cmd: &str) {
        match cmd {
            "toggle_sidebar" => {
                self.sidebar_visible = !self.sidebar_visible;
                if !self.sidebar_visible {
                    self.focus = Focus::Editor;
                }
            }
            "toggle_right_panel" => {
                if Self::llm_enabled_in_settings() {
                    self.right_panel_visible = !self.right_panel_visible;
                } else {
                    self.right_panel_visible = false;
                    self.tip = Some(texts(self.language).llm_disabled.to_string());
                }
                if !self.right_panel_visible && self.focus == Focus::RightPanel {
                    self.focus = Focus::Editor;
                }
            }
            "toggle_theme" => self.dark_theme = !self.dark_theme,
            "toggle_autosave" => {
                self.autosave_on_edit = !self.autosave_on_edit;
                self.tip = Some(if self.autosave_on_edit {
                    texts(self.language).tip_autosave_enabled.to_string()
                } else {
                    texts(self.language).tip_autosave_disabled.to_string()
                });
            }
            "rust_check_current" => {
                self.run_rust_check_current_file();
            }
            "show_diagnostics" => {
                if let Some(d) = self.diagnostics.as_mut() {
                    d.open = true;
                } else {
                    self.tip = Some(texts(self.language).tip_no_diagnostics_yet.to_string());
                }
            }
            "font_plus" => {
                self.font_zoom = (self.font_zoom + 1).min(4);
                self.tip = Some(texts(self.language).tip_font_zoom_fmt.replace("{}", &self.font_zoom.to_string()));
            }
            "font_minus" => {
                self.font_zoom = (self.font_zoom - 1).max(-2);
                self.tip = Some(texts(self.language).tip_font_zoom_fmt.replace("{}", &self.font_zoom.to_string()));
            }
            "toggle_line_spacing" => {
                self.line_spacing = !self.line_spacing;
                self.tip = Some(if self.line_spacing {
                    texts(self.language).tip_line_spacing_comfortable.to_string()
                } else {
                    texts(self.language).tip_line_spacing_compact.to_string()
                });
            }
            "toggle_ligatures" => {
                self.ligatures = !self.ligatures;
                self.tip = Some(if self.ligatures {
                    texts(self.language).tip_ligatures_on.to_string()
                } else {
                    texts(self.language).tip_ligatures_off.to_string()
                });
            }
            "quick_open" => self.quick_open = Some(QuickOpenState::default()),
            "project_search" => self.project_search = Some(ProjectSearchState::default()),
            "go_symbol" => self.symbol_jump = Some(SymbolJumpState::default()),
            "go_line" => self.go_to_line = Some(GoToLineState::default()),
            "llm_ask" => self.llm_prompt = Some(LlmPromptState::default()),
            "llm_history" => self.llm_history_view = Some(LlmHistoryViewState::default()),
            "agent_events" => self.agent_events_view = Some(AgentEventsViewState::default()),
            "agent_toggle_unsafe_tools" => {
                if self.agent_allow_unsafe_tools {
                    self.agent_allow_unsafe_tools = false;
                    self.tip = Some(texts(self.language).agent_unsafe_disabled.to_string());
                } else {
                    self.agent_unsafe_confirm = true;
                    self.tip = Some(texts(self.language).agent_unsafe_confirm_tip.to_string());
                }
            }
            "llm_history_clear" => {
                self.llm_history.clear();
                self.tip = Some(texts(self.language).llm_history_cleared.to_string());
            }
            "llm_insert_last_answer" => self.insert_last_llm_answer(),
            "toggle_pin" => self.toggle_pin_active_tab(),
            "show_help" => self.hotkeys_help = true,
            "language_picker" => {
                self.language_picker = true;
                self.language_sel = if self.language == Language::En { 0 } else { 1 };
            }
            "lsp_wave_extensions" => {
                let cur = self
                    .doc()
                    .path
                    .as_deref()
                    .is_some_and(languages::is_first_wave_path);
                self.tip = Some(
                    texts(self.language)
                        .tip_lsp_wave_fmt
                        .replacen("{}", &languages::FIRST_WAVE_EXTENSIONS.join(", "), 1)
                        .replacen(
                            "{}",
                            if cur { texts(self.language).yes } else { texts(self.language).no },
                            1,
                        ),
                );
            }
            "in_file_find" => {
                let w = self.word_under_cursor();
                let seed = if w.chars().count() > 80 {
                    String::new()
                } else {
                    w
                };
                self.in_file_find = Some(InFileFindState {
                    query: seed,
                    sel: 0,
                });
            }
            "git_status" => self.open_git_output(texts(self.language).git_title_status, &["status", "-sb"]),
            "git_diff_unstaged" => self.open_git_output(texts(self.language).git_title_diff_unstaged, &["diff", "--stat"]),
            "git_diff_staged" => self.open_git_output(texts(self.language).git_title_diff_staged, &["diff", "--cached", "--stat"]),
            "git_log" => self.open_git_output(
                texts(self.language).git_title_log,
                &["log", "-n", "24", "--oneline", "--decorate"],
            ),
            "llm_health" => self.run_llm_health_check(),
            "llm_explain_current_line" => self.run_llm_explain_current_line(),
            "agent_run_loop" => self.run_agent_loop_mvp(),
            _ => {}
        }
        self.persist_settings();
    }

    fn run_llm_health_check(&mut self) {
        let s = settings::load_settings();
        if !s.llm_enabled {
            self.tip = Some(texts(self.language).llm_disabled.to_string());
            self.llm_status = "disabled".to_string();
            return;
        }

        self.llm_status = "checking".to_string();
        let client = TceLlmClient::from_settings(&s);
        match client.check_health() {
            Ok(h) => {
                self.llm_health_checked = h.ok;
                self.llm_status = if h.ok {
                    "ready".to_string()
                } else {
                    "error".to_string()
                };
                self.tip = Some(if h.ok {
                    texts(self.language).llm_service_ok.to_string()
                } else {
                    texts(self.language).llm_service_not_ok.to_string()
                });
            }
            Err(e) => {
                self.llm_status = "error".to_string();
                self.tip = Some(e.user_message());
            }
        }
    }

    fn run_llm_explain_current_line(&mut self) {
        let s = settings::load_settings();
        if !s.llm_enabled {
            self.tip = Some(texts(self.language).llm_disabled.to_string());
            return;
        }

        if !self.ensure_llm_health(&s) {
            return;
        }

        let row = self.doc().row;
        let line = self
            .doc()
            .buffer
            .lines()
            .get(row)
            .map(|v| v.trim().to_string())
            .unwrap_or_default();
        if line.is_empty() {
            self.tip = Some(texts(self.language).llm_current_line_empty.to_string());
            return;
        }

        let user_prompt = format!("Объясни строку кода:\n{line}");
        self.run_llm_prompt(user_prompt);
    }

    fn run_llm_prompt(&mut self, user_prompt: String) {
        let s = settings::load_settings();
        if !s.llm_enabled {
            self.tip = Some(texts(self.language).llm_disabled.to_string());
            self.llm_status = "disabled".to_string();
            return;
        }

        if self.llm_inflight.is_some() {
            self.tip = Some(texts(self.language).llm_busy.to_string());
            return;
        }

        if !self.ensure_llm_health(&s) {
            return;
        }

        self.push_llm_history("user", user_prompt.clone());
        self.push_llm_history("assistant", String::new());
        self.llm_status = "generating".to_string();
        self.tip = Some(texts(self.language).llm_running.to_string());

        let req = LlmChatRequest {
            stream: true,
            system: s.llm_system_prompt.clone(),
            messages: vec![LlmChatMessage {
                role: "user".to_string(),
                content: user_prompt,
            }],
            editor: self.build_llm_editor_context(&s),
            generate: Some(LlmGenerateParams {
                max_tokens: s.llm_generate_max_tokens,
                temperature: s.llm_generate_temperature,
            }),
        };

        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_bg = Arc::clone(&cancel);
        let (tx, rx) = mpsc::channel::<LlmTaskResult>();
        std::thread::spawn(move || {
            let client = TceLlmClient::from_settings(&s);
            let tx_delta = tx.clone();
            let result = client.send_chat_streaming(&req, Arc::clone(&cancel_bg), move |delta| {
                let _ = tx_delta.send(LlmTaskResult::Delta(delta.to_string()));
            });
            if cancel_bg.load(Ordering::Relaxed) {
                return;
            }
            let _ = match result {
                Ok(resp) => tx.send(LlmTaskResult::Ok(resp.message.content)),
                Err(e) => tx.send(LlmTaskResult::Err(e.user_message())),
            };
        });
        self.llm_inflight = Some(LlmInFlight { cancel, rx });
    }

    fn poll_llm_inflight(&mut self) {
        let Some(inflight) = &self.llm_inflight else {
            return;
        };
        match inflight.rx.try_recv() {
            Ok(LlmTaskResult::Delta(chunk)) => {
                if let Some(last) = self.llm_history.last_mut() {
                    if last.role == "assistant" {
                        last.content.push_str(&chunk);
                    }
                }
                self.tip = Some(format!("LLM: {}", truncate_str(chunk.trim(), 100)));
            }
            Ok(LlmTaskResult::Ok(content)) => {
                self.llm_inflight = None;
                if let Some(last) = self.llm_history.last_mut() {
                    if last.role == "assistant" {
                        last.content = content.clone();
                    } else {
                        self.push_llm_history("assistant", content.clone());
                    }
                } else {
                    self.push_llm_history("assistant", content.clone());
                }
                self.llm_status = "ready".to_string();
                self.tip = Some(format!("LLM: {}", truncate_str(content.trim(), 160)));
            }
            Ok(LlmTaskResult::Err(message)) => {
                self.llm_inflight = None;
                self.llm_status = "error".to_string();
                self.tip = Some(message);
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                self.llm_inflight = None;
                self.llm_status = "error".to_string();
                self.tip = Some(texts(self.language).llm_stream_failed.to_string());
            }
        }
    }

    fn cancel_llm_inflight(&mut self) {
        if let Some(inflight) = self.llm_inflight.take() {
            inflight.cancel.store(true, Ordering::Relaxed);
            self.llm_status = "cancelled".to_string();
            self.tip = Some(texts(self.language).llm_cancelled.to_string());
        }
    }

    fn run_agent_loop_mvp(&mut self) {
        let s = settings::load_settings();
        if !s.llm_enabled {
            self.tip = Some(texts(self.language).llm_disabled.to_string());
            self.llm_status = "disabled".to_string();
            return;
        }
        if self.llm_inflight.is_some() || self.agent_inflight.is_some() {
            self.tip = Some(texts(self.language).tip_agent_busy.to_string());
            return;
        }
        if !self.ensure_llm_health(&s) {
            return;
        }

        let goal = if let Some(path) = self.doc().path.as_ref() {
            format!(
                "Проанализируй файл `{}` и предложи следующие действия через инструменты.",
                path.to_string_lossy()
            )
        } else {
            "Проанализируй текущий контекст проекта и предложи следующий шаг".to_string()
        };
        let session_id = format!("tce-session-{}", std::process::id());
        let root = self.project_root.clone();
        let allow_unsafe_tools = self.agent_allow_unsafe_tools;
        let (tx, rx) = mpsc::channel::<AgentTaskResult>();

        self.llm_status = "agent-running".to_string();
        self.tip = Some(texts(self.language).agent_running.to_string());
        self.push_llm_history("user", format!("[agent] {goal}"));

        std::thread::spawn(move || {
            let sandbox = match AgentSandbox::new(root, 64 * 1024) {
                Ok(sandbox) => sandbox,
                Err(e) => {
                    let _ = tx.send(AgentTaskResult::Err(e.to_string()));
                    return;
                }
            };
            let tools = AgentToolExecutor::new(sandbox, allow_unsafe_tools);
            let client = TceLlmClient::from_settings(&s);
            let orchestrator = AgentOrchestrator::new(&client, &tools, 6);
            let _ = match orchestrator.run(&session_id, &goal) {
                Ok(result) => tx.send(AgentTaskResult::Ok {
                    summary: result.final_summary,
                    steps: result.steps,
                    finished: result.finished,
                    events: result.events,
                }),
                Err(err) => tx.send(AgentTaskResult::Err(err)),
            };
        });

        self.agent_inflight = Some(AgentInFlight { rx });
    }

    fn poll_agent_inflight(&mut self) {
        let Some(inflight) = &self.agent_inflight else {
            return;
        };
        match inflight.rx.try_recv() {
            Ok(AgentTaskResult::Ok {
                summary,
                steps,
                finished,
                events,
            }) => {
                self.agent_inflight = None;
                self.llm_status = if finished {
                    "agent-ready".to_string()
                } else {
                    "agent-limit".to_string()
                };
                let msg = texts(self.language)
                    .agent_steps_summary_fmt
                    .replacen("{}", &steps.to_string(), 1)
                    .replacen("{}", &finished.to_string(), 1)
                    .replacen("{}", &summary, 1);
                self.push_llm_history("assistant", msg.clone());
                self.agent_events.extend(events);
                if self.agent_events.len() > 500 {
                    let drain = self.agent_events.len() - 500;
                    self.agent_events.drain(0..drain);
                }
                self.tip = Some(msg);
            }
            Ok(AgentTaskResult::Err(message)) => {
                self.agent_inflight = None;
                self.llm_status = "agent-error".to_string();
                self.push_llm_history(
                    "assistant",
                    format!("{}{}", texts(self.language).agent_error_prefix, message),
                );
                self.tip = Some(message);
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                self.agent_inflight = None;
                self.llm_status = "agent-error".to_string();
                self.tip = Some(texts(self.language).agent_stream_failed.to_string());
            }
        }
    }

    fn push_llm_history(&mut self, role: &str, content: String) {
        self.llm_history.push(LlmHistoryEntry {
            role: role.to_string(),
            content,
        });
        if self.llm_history.len() > 200 {
            let drain = self.llm_history.len() - 200;
            self.llm_history.drain(0..drain);
        }
    }

    fn insert_last_llm_answer(&mut self) {
        let last = self
            .llm_history
            .iter()
            .rev()
            .find(|entry| entry.role == "assistant" && !entry.content.trim().is_empty())
            .map(|entry| entry.content.clone());

        let Some(answer) = last else {
            self.tip = Some(texts(self.language).no_assistant_answer.to_string());
            return;
        };

        self.doc_mut().insert_text(&answer);
        self.tip = Some(texts(self.language).tip_assistant_answer_inserted.to_string());
    }

    fn ensure_llm_health(&mut self, s: &settings::AppSettings) -> bool {
        if self.llm_health_checked {
            return true;
        }

        let client = TceLlmClient::from_settings(s);
        match client.check_health() {
            Ok(h) if h.ok => {
                self.llm_health_checked = true;
                self.llm_status = "ready".to_string();
                true
            }
            Ok(_) => {
                self.tip = Some(texts(self.language).llm_service_not_ok.to_string());
                self.llm_status = "error".to_string();
                false
            }
            Err(e) => {
                self.tip = Some(e.user_message());
                self.llm_status = "error".to_string();
                false
            }
        }
    }

    fn build_llm_editor_context(&self, s: &settings::AppSettings) -> Option<LlmEditorContext> {
        if !s.llm_attach_editor {
            return None;
        }

        let doc = self.doc();
        let path = doc.path.as_ref()?;
        let path_text = path.to_string_lossy().to_string();
        let language = path
            .extension()
            .and_then(|v| v.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        let lines = doc.buffer.lines();
        if lines.is_empty() {
            return None;
        }

        let around = s.llm_snippet_lines / 2;
        let start = doc.row.saturating_sub(around);
        let end = (doc.row + around + 1).min(lines.len());
        let mut snippet = lines[start..end].join("\n");
        if snippet.len() > s.llm_snippet_max_bytes {
            snippet.truncate(s.llm_snippet_max_bytes);
        }

        Some(LlmEditorContext {
            path: path_text,
            language,
            snippet,
            cursor_line: doc.row,
            cursor_column: doc.col,
        })
    }

    fn run_rust_check_current_file(&mut self) {
        let Some(path) = self.doc().path.as_ref().cloned() else {
            self.tip = Some(texts(self.language).tip_open_file_first.to_string());
            return;
        };

        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            self.tip = Some(texts(self.language).tip_rust_diagnostics_rs_only.to_string());
            return;
        }
        let output = Command::new("rustc")
            .arg("--error-format=short")
            .arg("--emit=metadata")
            .arg(&path)
            .output();

        let Ok(output) = output else {
            self.tip = Some(texts(self.language).tip_failed_run_rustc.to_string());
            return;
        };
        let stderr = String::from_utf8_lossy(&output.stderr);
        let mut items = Vec::<DiagnosticItem>::new();

        for line in stderr.lines() {
            let mut parts = line.splitn(4, ':');
            let p0 = parts.next().unwrap_or_default().trim();
            let p1 = parts.next().unwrap_or_default().trim();
            let p2 = parts.next().unwrap_or_default().trim();
            let p3 = parts.next().unwrap_or_default().trim();
            if p0.is_empty() || p1.is_empty() || p2.is_empty() || p3.is_empty() {
                continue;
            }

            let line_num = p1.parse::<usize>().ok().unwrap_or(1).saturating_sub(1);
            let col_num = p2.parse::<usize>().ok().unwrap_or(1).saturating_sub(1);
            let diag_path = PathBuf::from(p0);
            if !diag_path.exists() {
                continue;
            }

            let severity = if p3.to_lowercase().contains("warning") {
                DiagnosticSeverity::Warning
            } else {
                DiagnosticSeverity::Error
            };

            items.push(DiagnosticItem {
                path: diag_path,
                row: line_num,
                col: col_num,
                message: p3.to_string(),
                severity,
            });
        }

        if items.is_empty() {
            self.tip = Some(texts(self.language).tip_rust_check_no_diagnostics.to_string());
            self.diagnostics = None;
            return;
        }

        self.diagnostics = Some(DiagnosticsState {
            items,
            sel: 0,
            open: true,
            filter: DiagnosticsFilter::All,
        });

        self.tip = Some(texts(self.language).tip_rust_diagnostics_ready.to_string());
    }

    fn diagnostics_visible_items(&self) -> Vec<DiagnosticItem> {
        let Some(state) = self.diagnostics.as_ref() else {
            return Vec::new();
        };

        state
            .items
            .iter()
            .filter(|d| match state.filter {
                DiagnosticsFilter::All => true,
                DiagnosticsFilter::Errors => d.severity == DiagnosticSeverity::Error,
                DiagnosticsFilter::Warnings => d.severity == DiagnosticSeverity::Warning,
            })
            .cloned()
            .collect()
    }

    fn handle_diagnostics_key(&mut self, key: Key) {
        if self.diagnostics.is_none() {
            return;
        }

        let mut visible_len = self.diagnostics_visible_items().len();
        match key {
            Key::Esc => {
                if let Some(state) = self.diagnostics.as_mut() {
                    state.open = false;
                }
            }
            Key::ArrowUp => {
                if let Some(state) = self.diagnostics.as_mut() {
                    state.sel = state.sel.saturating_sub(1);
                }
            }
            Key::ArrowDown => {
                if let Some(state) = self.diagnostics.as_mut() {
                    state.sel = (state.sel + 1).min(visible_len.saturating_sub(1));
                }
            }
            Key::Home => {
                if let Some(state) = self.diagnostics.as_mut() {
                    state.sel = 0;
                }
            }
            Key::End => {
                if let Some(state) = self.diagnostics.as_mut() {
                    state.sel = visible_len.saturating_sub(1);
                }
            }
            Key::Char('f') | Key::Char('F') => {
                if let Some(state) = self.diagnostics.as_mut() {
                    state.filter = match state.filter {
                        DiagnosticsFilter::All => DiagnosticsFilter::Errors,
                        DiagnosticsFilter::Errors => DiagnosticsFilter::Warnings,
                        DiagnosticsFilter::Warnings => DiagnosticsFilter::All,
                    };
                    state.sel = 0;
                }

                visible_len = self.diagnostics_visible_items().len();
                if visible_len == 0 {
                    self.tip = Some(texts(self.language).tip_no_diagnostics_for_filter.to_string());
                }
            }
            Key::Enter => {
                let items = self.diagnostics_visible_items();
                if items.is_empty() {
                    return;
                }

                let idx = self
                    .diagnostics
                    .as_ref()
                    .map(|s| s.sel.min(items.len().saturating_sub(1)))
                    .unwrap_or(0);
                let item = items[idx].clone();
                self.jump_to_path_position(item.path, item.row, item.col, true);
                self.diagnostics = None;
            }
            _ => {}
        }
    }

    fn open_git_output(&mut self, title: &str, args: &[&str]) {
        let out = match Command::new("git").current_dir(&self.project_root).args(args).output(){
            Ok(o) => o,
            Err(e) => {
                self.tip = Some(format!("{}: {e}", texts(self.language).error_prefix));
                return;
            }
        };
        let mut combined = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );

        if combined.is_empty() {
            combined.push_str(texts(self.language).tip_empty_output);
        }

        let lines: Vec<String> = combined.lines().map(|s| s.to_string()).collect();
        let title = if out.status.success() {
            title.to_string()
        } else {
            format!("{} (git exit {})", title, out.status)
        };

        self.git_view = Some(GitViewState {
            title,
            lines,
            scroll: 0,
            cursor: 0,
        });
    }

    fn handle_git_view_key(&mut self, key: Key) {
        let Some(state) = self.git_view.as_mut() else {
            return;
        };
        let total = state.lines.len();
        let size = winsize_tty().unwrap_or(TermSize { rows: 24, cols: 80 });
        let rows = size.rows.max(1) as usize;
        let body_h = rows.saturating_sub(2).max(1);

        match key {
            Key::Esc => {
                self.git_view = None;
            }
            Key::ArrowUp => {
                state.cursor = state.cursor.saturating_sub(1);
                if state.cursor < state.scroll {
                    state.scroll = state.cursor;
                }
            }
            Key::ArrowDown => {
                if total > 0 {
                    state.cursor = (state.cursor + 1).min(total - 1);
                    let max_scroll = total.saturating_sub(body_h);
                    if state.cursor >= state.scroll + body_h {
                        state.scroll = state.cursor + 1 - body_h;
                    }
                    if state.scroll > max_scroll {
                        state.scroll = max_scroll;
                    }
                }
            }
            Key::PageUp => {
                let step = body_h.max(1);
                state.cursor = state.cursor.saturating_sub(step);
                state.scroll = state.scroll.saturating_sub(step);
            }
            Key::PageDown => {
                let step = body_h.max(1);
                if total > 0 {
                    state.cursor = (state.cursor + step).min(total - 1);
                    let max_scroll = total.saturating_sub(body_h);
                    state.scroll = (state.scroll + step).min(max_scroll);
                    if state.cursor < state.scroll {
                        state.scroll = state.cursor;
                    }
                }
            }
            Key::Home => {
                state.cursor = 0;
                state.scroll = 0;
            }
            Key::End => {
                if total > 0 {
                    state.cursor = total - 1;
                }
                state.scroll = total.saturating_sub(body_h);
            }
            Key::Enter => {
                let line = state.lines.get(state.cursor).cloned().unwrap_or_default();
                let root = self.project_root.clone();
                if let Some(path) = path_from_git_short_status_line(&root, &line) {
                    self.git_view = None;
                    self.jump_to_path_position(path, 0, 0, true);
                }
            }
            _ => {}
        }
    }

    fn handle_llm_history_view_key(&mut self, key: Key) {
        let Some(state) = self.llm_history_view.as_mut() else {
            return;
        };

        let total = self.llm_history.len();
        let size = winsize_tty().unwrap_or(TermSize { rows: 24, cols: 80 });
        let rows = size.rows.max(1) as usize;
        let body_h = rows.saturating_sub(2).max(1);

        match key {
            Key::Esc => {
                self.llm_history_view = None;
            }
            Key::ArrowUp => {
                state.cursor = state.cursor.saturating_sub(1);
                if state.cursor < state.scroll {
                    state.scroll = state.cursor;
                }
            }
            Key::ArrowDown => {
                if total > 0 {
                    state.cursor = (state.cursor + 1).min(total - 1);
                    let max_scroll = total.saturating_sub(body_h);
                    if state.cursor >= state.scroll + body_h {
                        state.scroll = state.cursor + 1 - body_h;
                    }
                    if state.scroll > max_scroll {
                        state.scroll = max_scroll;
                    }
                }
            }
            Key::PageUp => {
                let step = body_h.max(1);
                state.cursor = state.cursor.saturating_sub(step);
                state.scroll = state.scroll.saturating_sub(step);
            }
            Key::PageDown => {
                let step = body_h.max(1);
                if total > 0 {
                    state.cursor = (state.cursor + step).min(total - 1);
                    let max_scroll = total.saturating_sub(body_h);
                    state.scroll = (state.scroll + step).min(max_scroll);
                    if state.cursor < state.scroll {
                        state.scroll = state.cursor;
                    }
                }
            }
            Key::Home => {
                state.cursor = 0;
                state.scroll = 0;
            }
            Key::End => {
                if total > 0 {
                    state.cursor = total - 1;
                }
                state.scroll = total.saturating_sub(body_h);
            }
            _ => {}
        }
    }

    fn handle_agent_events_view_key(&mut self, key: Key) {
        let Some(state) = self.agent_events_view.as_mut() else {
            return;
        };

        let total = self.agent_events.len();
        let size = winsize_tty().unwrap_or(TermSize { rows: 24, cols: 80 });
        let rows = size.rows.max(1) as usize;
        let body_h = rows.saturating_sub(2).max(1);

        match key {
            Key::Esc => {
                self.agent_events_view = None;
            }
            Key::ArrowUp => {
                state.cursor = state.cursor.saturating_sub(1);
                if state.cursor < state.scroll {
                    state.scroll = state.cursor;
                }
            }
            Key::ArrowDown => {
                if total > 0 {
                    state.cursor = (state.cursor + 1).min(total - 1);
                    let max_scroll = total.saturating_sub(body_h);
                    if state.cursor >= state.scroll + body_h {
                        state.scroll = state.cursor + 1 - body_h;
                    }
                    if state.scroll > max_scroll {
                        state.scroll = max_scroll;
                    }
                }
            }
            Key::PageUp => {
                let step = body_h.max(1);
                state.cursor = state.cursor.saturating_sub(step);
                state.scroll = state.scroll.saturating_sub(step);
            }
            Key::PageDown => {
                let step = body_h.max(1);
                if total > 0 {
                    state.cursor = (state.cursor + step).min(total - 1);
                    let max_scroll = total.saturating_sub(body_h);
                    state.scroll = (state.scroll + step).min(max_scroll);
                    if state.cursor < state.scroll {
                        state.scroll = state.cursor;
                    }
                }
            }
            Key::Home => {
                state.cursor = 0;
                state.scroll = 0;
            }
            Key::End => {
                if total > 0 {
                    state.cursor = total - 1;
                }
                state.scroll = total.saturating_sub(body_h);
            }
            _ => {}
        }
    }

    fn handle_agent_unsafe_confirm_key(&mut self, key: Key) {
        match key {
            Key::Char('y') | Key::Char('Y') => {
                self.agent_allow_unsafe_tools = true;
                self.agent_unsafe_confirm = false;
                self.tip = Some(texts(self.language).agent_unsafe_enabled.to_string());
            }
            Key::Char('n') | Key::Char('N') | Key::Esc => {
                self.agent_unsafe_confirm = false;
                self.tip = Some(texts(self.language).agent_unsafe_cancelled.to_string());
            }
            _ => {}
        }
    }

    fn symbol_jump_matches(&self) -> Vec<SymbolItem> {
        let Some(state) = &self.symbol_jump else {
            return Vec::new();
        };
        let q = state.query.to_lowercase();
        let mut out = Vec::<SymbolItem>::new();
        for entry in self.tree.iter().filter(|e| !e.is_dir) {
            let Ok(content) = fs::read_to_string(&entry.path) else {
                continue;
            };

            for (line_idx, line) in content.lines().enumerate() {
                if let Some((kind, name)) = extract_symbol_from_line(line) {
                    let key = format!("{kind} {name} {}", entry.path.to_string_lossy()).to_lowercase();
                    if q.is_empty() || key.contains(&q) {
                        out.push(SymbolItem {
                            path: entry.path.clone(),
                            line_idx,
                            name,
                            kind,
                        });
                        if out.len() >= 120 {
                            return out;
                        }
                    }
                }
            }
        }
        out
    }

    fn render_sidebar_menu_overlay(&self, out: &mut String, cols: usize, rows: usize) {
        let actions = SidebarAction::all();
        let max_label = actions
            .iter()
            .map(|a| format!("[{}] {}", a.shortcut(), a.label()).chars().count())
            .max()
            .unwrap_or(10);
        let width = (max_label + 6).min(cols.saturating_sub(2)).max(12);
        let start_col = 2usize;
        let start_row = 2usize;

        for (i, action) in actions.iter().enumerate() {
            if start_row + i >= rows {
                break;
            }

            let mut line = String::new();
            line.push(' ');
            line.push_str(&format!("[{}] {}", action.shortcut(), action.label()));
            while line.chars().count() < width {
                line.push(' ');
            }

            if i == self.sidebar_menu_sel {
                out.push_str(&format!(
                    "\x1b[{};{}H\x1b[7m{}\x1b[0m",
                    start_row + i,
                    start_col,
                    line
                ));
            } else {
                out.push_str(&format!("\x1b[{};{}H{}", start_row + i, start_col, line));
            }
        }
    }

    fn render_sidebar_prompt_overlay(&self, out: &mut String, cols: usize, rows: usize) {
        let Some(prompt) = self.sidebar_prompt.as_ref() else {
            return;
        };
        if rows == 0 {
            return;
        }
        let label = match prompt.kind {
            SidebarPromptKind::CreateFile => "New file:",
            SidebarPromptKind::CreateFolder => "New folder:",
            SidebarPromptKind::Move => "Move to:",
            SidebarPromptKind::Rename => "Rename to:",
        };
        let mut line = format!("{label} {}", prompt.input);
        if line.chars().count() > cols {
            line = line
                .chars()
                .skip(line.chars().count().saturating_sub(cols))
                .collect();
        }
        out.push_str(&format!("\x1b[{};1H\x1b[7m", rows));
        out.push_str(&line);
        while line.chars().count() < cols {
            out.push(' ');
            line.push(' ');
        }
        out.push_str("\x1b[0m");
    }

    fn render_quick_open_overlay(&self, out: &mut String, cols: usize, rows: usize) {
        let Some(state) = self.quick_open.as_ref() else {
            return;
        };
        if rows < 2 || cols == 0 {
            return;
        }
        let prompt_row = rows;
        let mut prompt = format!("Quick open: {}", state.query);
        if prompt.chars().count() > cols {
            prompt = prompt
                .chars()
                .skip(prompt.chars().count().saturating_sub(cols))
                .collect();
        }
        out.push_str(&format!("\x1b[{};1H\x1b[7m", prompt_row));
        out.push_str(&prompt);
        while prompt.chars().count() < cols {
            out.push(' ');
            prompt.push(' ');
        }
        out.push_str("\x1b[0m");

        let matches = self.quick_open_matches();
        let mut row = rows.saturating_sub(1);
        for (i, path) in matches.iter().enumerate() {
            if row == 0 {
                break;
            }
            let rel = path
                .strip_prefix(&self.project_root)
                .ok()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| path.to_string_lossy().to_string());
            let mut line = format!("  {}", truncate_str(&rel, cols.saturating_sub(2)));
            while line.chars().count() < cols {
                line.push(' ');
            }
            if i == state.sel {
                out.push_str(&format!("\x1b[{};1H\x1b[7m{}\x1b[0m", row, line));
            } else {
                out.push_str(&format!("\x1b[{};1H{}", row, line));
            }
            row = row.saturating_sub(1);
        }
    }

    fn render_in_file_find_overlay(&self, out: &mut String, cols: usize, rows: usize) {
        let Some(state) = self.in_file_find.as_ref() else {
            return;
        };

        let tx = texts(self.language);
        if rows < 2 || cols == 0 {
            return;
        }

        let matches = self.in_file_find_matches();
        let meta = if matches.is_empty() {
            if state.query.is_empty() {
                String::new()
            } else {
                " - 0".to_string()
            }
        } else {
            format!(
                " - {}/{}",
                (state.sel + 1).min(matches.len()),
                matches.len()
            )
        };

        let prompt_row = rows;
        let mut prompt = format!("{} {}{}", tx.find_in_file_prompt, state.query, meta);
        if prompt.chars().count() > cols {
            prompt = prompt
                .chars()
                .skip(prompt.chars().count().saturating_sub(cols))
                .collect();
        }

        out.push_str(&format!("\x1b[{};1H\x1b[7m", prompt_row));
        out.push_str(&prompt);
        while prompt.chars().count() < cols {
            out.push(' ');
            prompt.push(' ');
        }
        out.push_str("\x1b[0m");

        if rows >= 3 {
            let hint = truncate_str(tx.find_in_file_hint, cols);
            out.push_str(&format!(
                "\x1b[{};1H\x1b[90m{}\x1b[0m",
                rows.saturating_sub(1),
                hint
            ));
        }
    }

    fn render_project_search_overlay(&self, out: &mut String, cols: usize, rows: usize) {
        let Some(state) = self.project_search.as_ref() else {
            return;
        };

        if rows < 2 || cols == 0 {
            return;
        }

        let prompt_row = rows;
        let mode = if state.regex_mode { "regex" } else { "text" };
        let marker = if state.edit_replacement { "replace*" } else { "search*" };
        let mut prompt = format!(
            "[{mode}] {marker} Search: {} | Replace: {}",
            state.query, state.replacement
        );

        if state.confirm_replace_all {
            prompt.push_str(" | Enter: confirm replace all");
        }

        prompt.push_str(" | Tab field | Ctrl+O regex | Ctrl+G replace current | Ctrl+R replace all");
        if prompt.chars().count() > cols {
            prompt = prompt
                .chars()
                .skip(prompt.chars().count().saturating_sub(cols))
                .collect();
        }

        out.push_str(&format!("\x1b[{};1H\x1b[7m", prompt_row));
        out.push_str(&prompt);
        while prompt.chars().count() < cols {
            out.push(' ');
            prompt.push(' ');
        }

        out.push_str("\x1b[0m");

        let matches = self.project_search_matches();
        let preview = if !state.replacement.is_empty() {
            if let Some(first) = matches.get(state.sel.min(matches.len().saturating_sub(1))) {
                self.render_search_replace_preview(first)
            } else {
                None
            }
        } else {
            None
        };

        let mut row = rows.saturating_sub(1);
        if let Some(preview_line) = preview {
            if row > 0 {
                let mut line = format!(" preview: {}", truncate_str(&preview_line, cols.saturating_sub(10)));
                while line.chars().count() < cols {
                    line.push(' ');
                }
                out.push_str(&format!("\x1b[{};1H\x1b[48;5;238m{}\x1b[0m", row, line));
                row = row.saturating_sub(1);
            }
        }
        for (i, hit) in matches.iter().enumerate() {
            if row == 0 {
                break;
            }

            let rel = hit
                .path
                .strip_prefix(&self.project_root)
                .ok()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| hit.path.to_string_lossy().to_string());
            let body = format!(
                " {}:{}  {}",
                rel,
                hit.line_idx + 1,
                truncate_str(&hit.preview, cols.saturating_sub(16))
            );

            let mut line = truncate_str(&body, cols);
            while line.chars().count() < cols {
                line.push(' ');
            }

            if i == state.sel {
                out.push_str(&format!("\x1b[{};1H\x1b[7m{}\x1b[0m", row, line));
            } else {
                out.push_str(&format!("\x1b[{};1H{}", row, line));
            }

            row = row.saturating_sub(1);
        }
    }

    fn render_symbol_jump_overlay(&self, out: &mut String, cols: usize, rows: usize) {
        let Some(state) = self.symbol_jump.as_ref() else {
            return;
        };

        if rows < 2 || cols == 0 {
            return;
        }

        let mut prompt = format!("Symbols: {}", state.query);
        if prompt.chars().count() > cols {
            prompt = prompt
                .chars()
                .skip(prompt.chars().count().saturating_sub(cols))
                .collect();
        }

        out.push_str(&format!("\x1b[{};1H\x1b[7m", rows));
        out.push_str(&prompt);
        while prompt.chars().count() < cols {
            out.push(' ');
            prompt.push(' ');
        }

        out.push_str("\x1b[0m");

        let matches = self.symbol_jump_matches();
        let mut row = rows.saturating_sub(1);
        for (i, item) in matches.iter().enumerate() {
            if row == 0 {
                break;
            }

            let rel = item
                .path
                .strip_prefix(&self.project_root)
                .ok()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| item.path.to_string_lossy().to_string());
            let mut line = format!(" [{}] {}  {}:{}", item.kind, item.name, rel, item.line_idx + 1);
            line = truncate_str(&line, cols);
            while line.chars().count() < cols {
                line.push(' ');
            }

            if i == state.sel {
                out.push_str(&format!("\x1b[{};1H\x1b[7m{}\x1b[0m", row, line));
            } else {
                out.push_str(&format!("\x1b[{};1H{}", row, line));
            }

            row = row.saturating_sub(1);
        }
    }

    fn render_go_to_line_overlay(&self, out: &mut String, cols: usize, rows: usize) {
        let Some(state) = self.go_to_line.as_ref() else {
            return;
        };

        if rows == 0 || cols == 0 {
            return;
        }

        let mut prompt = format!("Go to line: {}", state.input);
        if prompt.chars().count() > cols {
            prompt = prompt
                .chars()
                .skip(prompt.chars().count().saturating_sub(cols))
                .collect();
        }

        out.push_str(&format!("\x1b[{};1H\x1b[7m", rows));
        out.push_str(&prompt);
        while prompt.chars().count() < cols {
            out.push(' ');
            prompt.push(' ');
        }

        out.push_str("\x1b[0m");
    }

    fn render_llm_prompt_overlay(&self, out: &mut String, cols: usize, rows: usize) {
        let Some(state) = self.llm_prompt.as_ref() else {
            return;
        };

        if rows == 0 || cols == 0 {
            return;
        }

        let mut prompt = format!("{} {}", texts(self.language).llm_ask_prefix, state.input);
        if prompt.chars().count() > cols {
            prompt = prompt
                .chars()
                .skip(prompt.chars().count().saturating_sub(cols))
                .collect();
        }

        out.push_str(&format!("\x1b[{};1H\x1b[7m", rows));
        out.push_str(&prompt);
        while prompt.chars().count() < cols {
            out.push(' ');
            prompt.push(' ');
        }

        out.push_str("\x1b[0m");
    }

    fn render_multi_edit_overlay(&self, out: &mut String, cols: usize, rows: usize) {
        let Some(state) = self.multi_edit.as_ref() else {
            return;
        };

        if rows == 0 || cols == 0 {
            return;
        }

        let mut prompt = format!(
            "Multi-edit: '{}' -> '{}' (Enter apply, Esc cancel)",
            state.target, state.replacement
        );

        if prompt.chars().count() > cols {
            prompt = prompt
                .chars()
                .skip(prompt.chars().count().saturating_sub(cols))
                .collect();
        }

        out.push_str(&format!("\x1b[{};1H\x1b[7m", rows));
        out.push_str(&prompt);
        while prompt.chars().count() < cols {
            out.push(' ');
            prompt.push(' ');
        }
        out.push_str("\x1b[0m");
    }

    fn render_sync_edit_overlay(&self, out: &mut String, cols: usize, rows: usize) {
        let Some(state) = self.sync_edit.as_ref() else {
            return;
        };

        if rows == 0 || cols == 0 {
            return;
        }

        let mut prompt = format!(
            "Sync-edit {}x: '{}' -> '{}' (Enter apply, Esc cancel)",
            state.occurrences, state.target, state.replacement
        );

        if prompt.chars().count() > cols {
            prompt = prompt
                .chars()
                .skip(prompt.chars().count().saturating_sub(cols))
                .collect();
        }

        out.push_str(&format!("\x1b[{};1H\x1b[7m", rows));
        out.push_str(&prompt);
        while prompt.chars().count() < cols {
            out.push(' ');
            prompt.push(' ');
        }
        out.push_str("\x1b[0m");
    }

    fn render_command_palette_overlay(&self, out: &mut String, cols: usize, rows: usize) {
        let Some(state) = self.command_palette.as_ref() else {
            return;
        };

        if rows < 2 || cols == 0 {
            return;
        }

        let mut prompt = format!("Command: {}", state.query);
        if prompt.chars().count() > cols {
            prompt = prompt
                .chars()
                .skip(prompt.chars().count().saturating_sub(cols))
                .collect();
        }

        out.push_str(&format!("\x1b[{};1H\x1b[7m", rows));
        out.push_str(&prompt);
        while prompt.chars().count() < cols {
            out.push(' ');
            prompt.push(' ');
        }
        out.push_str("\x1b[0m");

        let items = self.command_palette_items();
        let mut row = 1usize;
        for (i, item) in items.iter().enumerate() {
            if row >= rows {
                break;
            }

            let mut line = format!(" {}", item.0);
            line = truncate_str(&line, cols);
            while line.chars().count() < cols {
                line.push(' ');
            }

            if i == state.sel {
                out.push_str(&format!("\x1b[{};1H\x1b[7m{}\x1b[0m", row, line));
            } else {
                out.push_str(&format!("\x1b[{};1H{}", row, line));
            }
            row = row.saturating_add(1);
        }
    }

    fn render_diagnostics_overlay(&self, out: &mut String, cols: usize, rows: usize) {
        let Some(state) = self.diagnostics.as_ref() else {
            return;
        };

        if rows < 2 || cols == 0 {
            return;
        }

        let visible = self.diagnostics_visible_items();
        let filter = match state.filter {
            DiagnosticsFilter::All => "All",
            DiagnosticsFilter::Errors => "Errors",
            DiagnosticsFilter::Warnings => "Warnings",
        };
        let mut title = format!(
            "Diagnostics {} / {} [{}] (F filter)",
            visible.len(),
            state.items.len(),
            filter
        );

        if title.chars().count() > cols {
            title = truncate_str(&title, cols);
        }

        out.push_str(&format!("\x1b[{};1H\x1b[7m", rows));
        out.push_str(&title);
        while title.chars().count() < cols {
            out.push(' ');
            title.push(' ');
        }

        out.push_str("\x1b[0m");

        let mut row = rows.saturating_sub(1);
        for (i, d) in visible.iter().enumerate() {
            if row == 0 {
                break;
            }

            let rel = d
                .path
                .strip_prefix(&self.project_root)
                .ok()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| d.path.to_string_lossy().to_string());
            let mut line = format!(
                " [{}] {}:{}:{} {}",
                match d.severity {
                    DiagnosticSeverity::Error => "E",
                    DiagnosticSeverity::Warning => "W",
                },
                rel,
                d.row.saturating_add(1),
                d.col.saturating_add(1),
                d.message
            );

            line = truncate_str(&line, cols);
            while line.chars().count() < cols {
                line.push(' ');
            }
            if i == state.sel {
                out.push_str(&format!("\x1b[{};1H\x1b[7m{}\x1b[0m", row, line));
            } else {
                out.push_str(&format!("\x1b[{};1H{}", row, line));
            }
            row = row.saturating_sub(1);
        }
    }

    fn render_git_view_overlay(&self, out: &mut String, cols: usize, rows: usize) {
        let Some(state) = self.git_view.as_ref() else {
            return;
        };

        if rows < 3 || cols == 0 {
            return;
        }

        let hint = "Esc close | arrows PgUp/Dn | Home/End | Enter=open file (from status)";
        let hint = truncate_str(hint, cols);
        out.push_str(&format!("\x1b[1;1H\x1b[90m{hint}\x1b[0m"));

        let body_h = rows.saturating_sub(2).max(1);
        let last_line = (state.scroll + body_h).min(state.lines.len().max(1));
        let mut title_line = format!(
            "{}  (lines {}-{} of {})",
            state.title,
            state.scroll + 1,
            last_line,
            state.lines.len()
        );

        if title_line.chars().count() > cols {
            title_line = truncate_str(&title_line, cols);
        }

        while title_line.chars().count() < cols {
            title_line.push(' ');
        }

        out.push_str(&format!("\x1b[{};1H\x1b[7m", rows));
        out.push_str(&title_line);
        out.push_str("\x1b[0m");

        let mut row = rows.saturating_sub(1);
        let start = state.scroll;
        let end = (start + body_h).min(state.lines.len());
        for line_idx in start..end {
            if row <= 1 {
                break;
            }

            let line = state.lines.get(line_idx).cloned().unwrap_or_default();
            let mut clipped = truncate_str(&line, cols);
            while clipped.chars().count() < cols {
                clipped.push(' ');
            }

            if line_idx == state.cursor {
                out.push_str(&format!("\x1b[{};1H\x1b[7m{}\x1b[0m", row, clipped));
            } else {
                out.push_str(&format!("\x1b[{};1H{}", row, clipped));
            }
            row = row.saturating_sub(1);
        }
    }

    fn render_llm_history_overlay(&self, out: &mut String, cols: usize, rows: usize) {
        let Some(state) = self.llm_history_view.as_ref() else {
            return;
        };

        if rows < 3 || cols == 0 {
            return;
        }

        let hint = "Esc close | arrows PgUp/Dn | Home/End";
        let hint = truncate_str(hint, cols);
        out.push_str(&format!("\x1b[1;1H\x1b[90m{hint}\x1b[0m"));

        let body_h = rows.saturating_sub(2).max(1);
        let last_line = (state.scroll + body_h).min(self.llm_history.len().max(1));
        let mut title_line = texts(self.language)
            .llm_history_overlay_title
            .replacen("{}", &(state.scroll + 1).to_string(), 1)
            .replacen("{}", &last_line.to_string(), 1)
            .replacen("{}", &self.llm_history.len().to_string(), 1);

        if title_line.chars().count() > cols {
            title_line = truncate_str(&title_line, cols);
        }

        while title_line.chars().count() < cols {
            title_line.push(' ');
        }

        out.push_str(&format!("\x1b[{};1H\x1b[7m", rows));
        out.push_str(&title_line);
        out.push_str("\x1b[0m");

        let mut row = rows.saturating_sub(1);
        let start = state.scroll;
        let end = (start + body_h).min(self.llm_history.len());
        for idx in start..end {
            if row <= 1 {
                break;
            }

            let item = &self.llm_history[idx];
            let text = format!("[{}] {}", item.role, item.content);
            let mut clipped = truncate_str(&text, cols);
            while clipped.chars().count() < cols {
                clipped.push(' ');
            }

            if idx == state.cursor {
                out.push_str(&format!("\x1b[{};1H\x1b[7m{}\x1b[0m", row, clipped));
            } else {
                out.push_str(&format!("\x1b[{};1H{}", row, clipped));
            }

            row = row.saturating_sub(1);
        }
    }

    fn render_agent_events_overlay(&self, out: &mut String, cols: usize, rows: usize) {
        let Some(state) = self.agent_events_view.as_ref() else {
            return;
        };

        if rows < 3 || cols == 0 {
            return;
        }

        let hint = "Esc close | arrows PgUp/Dn | Home/End";
        let hint = truncate_str(hint, cols);
        out.push_str(&format!("\x1b[1;1H\x1b[90m{hint}\x1b[0m"));

        let body_h = rows.saturating_sub(2).max(1);
        let last_line = (state.scroll + body_h).min(self.agent_events.len().max(1));
        let mut title_line = texts(self.language)
            .agent_events_overlay_title
            .replacen("{}", &(state.scroll + 1).to_string(), 1)
            .replacen("{}", &last_line.to_string(), 1)
            .replacen("{}", &self.agent_events.len().to_string(), 1);

        if title_line.chars().count() > cols {
            title_line = truncate_str(&title_line, cols);
        }

        while title_line.chars().count() < cols {
            title_line.push(' ');
        }

        out.push_str(&format!("\x1b[{};1H\x1b[7m", rows));
        out.push_str(&title_line);
        out.push_str("\x1b[0m");

        let mut row = rows.saturating_sub(1);
        let start = state.scroll;
        let end = (start + body_h).min(self.agent_events.len());
        for idx in start..end {
            if row <= 1 {
                break;
            }

            let mut clipped = truncate_str(&self.agent_events[idx], cols);
            while clipped.chars().count() < cols {
                clipped.push(' ');
            }

            if idx == state.cursor {
                out.push_str(&format!("\x1b[{};1H\x1b[7m{}\x1b[0m", row, clipped));
            } else {
                out.push_str(&format!("\x1b[{};1H{}", row, clipped));
            }
            row = row.saturating_sub(1);
        }
    }

    fn render_agent_unsafe_confirm_overlay(&self, out: &mut String, cols: usize, rows: usize) {
        if rows < 3 || cols == 0 {
            return;
        }

        let msg = texts(self.language).agent_unsafe_confirm_overlay;
        let mut line = truncate_str(msg, cols);
        while line.chars().count() < cols {
            line.push(' ');
        }

        let row = (rows / 2).max(1);
        out.push_str(&format!("\x1b[{};1H\x1b[7m{}\x1b[0m", row, line));
    }

    fn render_search_replace_preview(&self, hit: &SearchMatch) -> Option<String> {
        let state = self.project_search.as_ref()?;
        if state.query.is_empty() || state.replacement.is_empty() {
            return None;
        }

        let content = fs::read_to_string(&hit.path).ok()?;
        let line = content.lines().nth(hit.line_idx)?;
        let replaced = if state.regex_mode {
            let re = Regex::new(&state.query).ok()?;
            re.replacen(line, 1, state.replacement.as_str()).to_string()
        } else if let Some(pos) = line.find(&state.query) {
            let mut next = String::new();
            next.push_str(&line[..pos]);
            next.push_str(&state.replacement);
            next.push_str(&line[pos + state.query.len()..]);
            next
        } else {
            return None;
        };
        Some(format!("{} -> {}", line.trim(), replaced.trim()))
    }

    fn tabs_line(&self, cols: usize) -> String {
        let mut out = String::new();
        out.push_str(self.tab_bar_bg());
        if self.docs.is_empty() {
            out.push_str(" [new] ");
        } else {
            for (idx, doc) in self.docs.iter().enumerate() {
                let mut name = doc
                    .path
                    .as_ref()
                    .and_then(|p| p.file_name())
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "[new]".to_string());
                if doc.pinned {
                    name = format!("P:{name}");
                }

                if doc.dirty {
                    name.push('*');
                }

                let label = format!(" {}  x ", truncate_str(&name, 18));
                if idx == self.active_doc {
                    if self.focus == Focus::Tabs && idx == self.tab_sel {
                        out.push_str(self.tab_focus_bg());
                    } else {
                        out.push_str(self.tab_active_bg());
                    }
                    out.push_str(&label);
                    out.push_str(self.tab_bar_bg());
                } else if self.focus == Focus::Tabs && idx == self.tab_sel {
                    out.push_str(self.tab_inactive_focus_bg());
                    out.push_str(&label);
                    out.push_str(self.tab_bar_bg());
                } else {
                    out.push_str(&label);
                }
                out.push(' ');
            }
        }
        out.push_str("\x1b[m");
        let mut clipped: String = out.chars().take(cols).collect();
        while clipped.chars().count() < cols {
            clipped.push(' ');
        }
        clipped
    }

    fn tab_cursor_col(&self, cols: usize) -> u32 {
        if self.docs.is_empty() {
            return 2;
        }
        let mut x = 1usize;
        for (idx, doc) in self.docs.iter().enumerate() {
            let mut name = doc
                .path
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "[new]".to_string());
            if doc.pinned {
                name = format!("P:{name}");
            }
            if doc.dirty {
                name.push('*');
            }
            let w = format!(" {}  x ", truncate_str(&name, 18)).chars().count() + 1;
            if idx == self.tab_sel {
                return (x + 1).min(cols.max(1)) as u32;
            }
            x += w;
        }
        2
    }

    fn focus_next(&mut self) {
        self.focus = match self.focus {
            Focus::Sidebar => Focus::Tabs,
            Focus::Tabs => Focus::Editor,
            Focus::Editor => {
                if self.right_panel_visible {
                    Focus::RightPanel
                } else if self.sidebar_visible {
                    Focus::Sidebar
                } else {
                    Focus::Tabs
                }
            }
            Focus::RightPanel => {
                if self.sidebar_visible {
                    Focus::Sidebar
                } else {
                    Focus::Tabs
                }
            }
        };
    }

    fn focus_prev(&mut self) {
        self.focus = match self.focus {
            Focus::Sidebar => {
                if self.right_panel_visible {
                    Focus::RightPanel
                } else {
                    Focus::Editor
                }
            }
            Focus::Tabs => {
                if self.sidebar_visible {
                    Focus::Sidebar
                } else if self.right_panel_visible {
                    Focus::RightPanel
                } else {
                    Focus::Editor
                }
            }
            Focus::Editor => Focus::Tabs,
            Focus::RightPanel => Focus::Editor,
        };
    }

    fn handle_right_panel_key(&mut self, key: Key) {
        match key {
            Key::Backspace => {
                self.right_panel_input.pop();
            }
            Key::Char(ch) => {
                self.right_panel_input.push(ch);
            }
            Key::Enter => {
                let prompt = self.right_panel_input.trim().to_string();
                if prompt.is_empty() {
                    self.tip = Some(texts(self.language).llm_prompt_empty.to_string());
                    return;
                }
                self.right_panel_input.clear();
                self.run_llm_prompt(prompt);
            }
            _ => {}
        }
    }

    fn render_language_picker(&self) -> io::Result<()> {
        let size = winsize_tty().unwrap_or(TermSize { rows: 24, cols: 80 });
        let rows = size.rows.max(1) as usize;
        let cols = size.cols.max(1) as usize;
        let content_h = rows.saturating_sub(1).max(1);
        let tx = texts(self.language);

        let mut lines: Vec<String> = Vec::new();
        lines.push(format!("\x1b[1m Tce \x1b[0m - {}", tx.language_menu_title));
        lines.push(String::new());

        let options = [tx.language_option_en, tx.language_option_ru];
        for (idx, option) in options.iter().enumerate() {
            if idx == self.language_sel {
                lines.push(format!("\x1b[7m> {option}\x1b[0m"));
            } else {
                lines.push(format!("  {option}"));
            }
        }

        lines.push(String::new());
        lines.push(tx.language_menu_hint.to_string());

        while lines.len() < content_h {
            lines.push(String::new());
        }
        lines.truncate(content_h);

        let status = format!("\x1b[7m language | {} \x1b[m", tx.language_menu_hint);
        let status: String = status.chars().take(cols).collect();

        let mut out = String::with_capacity(rows * (cols + 24));
        out.push_str("\x1b[H\x1b[J");
        for ln in lines {
            let clipped: String = ln.chars().take(cols).collect();
            out.push_str(&clipped);
            out.push_str("\r\n");
        }
        out.push_str(&status);

        let mut stdout = io::stdout().lock();
        stdout.write_all(out.as_bytes())?;
        stdout.flush()?;
        Ok(())
    }

    fn render_hotkeys_help(&self) -> io::Result<()> {
        let size = winsize_tty().unwrap_or(TermSize { rows: 24, cols: 80 });
        let rows = size.rows.max(1) as usize;
        let cols = size.cols.max(1) as usize;
        let content_h = rows.saturating_sub(1).max(1);
        let tx = texts(self.language);

        let mut lines: Vec<String> = vec![
            format!("\x1b[1m Tce \x1b[0m - {}", tx.help_title),
            String::new(),
            tx.help_k1.to_string(),
            tx.help_k2.to_string(),
            tx.help_k3.to_string(),
            tx.help_k4.to_string(),
            tx.help_k5.to_string(),
            tx.help_k6.to_string(),
            tx.help_k7.to_string(),
            tx.help_k8.to_string(),
            tx.help_k9.to_string(),
            tx.help_k10.to_string(),
            String::new(),
            tx.help_hint.to_string(),
        ];
        while lines.len() < content_h {
            lines.push(String::new());
        }
        lines.truncate(content_h);

        let status = format!("\x1b[7m help | {} \x1b[m", tx.help_hint);
        let status: String = status.chars().take(cols).collect();

        let mut out = String::with_capacity(rows * (cols + 24));
        out.push_str("\x1b[H\x1b[J");
        for ln in lines {
            let clipped: String = ln.chars().take(cols).collect();
            out.push_str(&clipped);
            out.push_str("\r\n");
        }
        out.push_str(&status);

        let mut stdout = io::stdout().lock();
        stdout.write_all(out.as_bytes())?;
        stdout.flush()?;
        Ok(())
    }
}

fn rows_to_u32(row: usize) -> u32 {
    row.min(u32::MAX as usize) as u32
}

/// Строки `git status -sb`: `XY PATH` (два status-символа, пробел, путь)
fn path_from_git_short_status_line(project_root: &Path, line: &str) -> Option<PathBuf> {
    let t = line.trim_end();
    if t.starts_with("## ") || t.len() < 4 {
        return None;
    }

    let bytes = t.as_bytes();
    if bytes.get(2) != Some(&b' ') {
        return None;
    }

    let rest = t.get(3..)?.trim();
    if rest.is_empty() || rest.contains(" -> ") {
        return None;
    }

    let p = PathBuf::from(rest);
    let full = if p.is_absolute() {
        p
    } else {
        project_root.join(p)
    };
    
    if full.is_file() {
        Some(full)
    } else {
        None
    }
}

fn extract_symbol_from_line(line: &str) -> Option<(String, String)> {
    let t = line.trim_start();
    let patterns = [
        ("fn", "fn "),
        ("struct", "struct "),
        ("enum", "enum "),
        ("impl", "impl "),
        ("trait", "trait "),
        ("class", "class "),
        ("def", "def "),
        ("function", "function "),
    ];
    for (kind, prefix) in patterns {
        if let Some(rest) = t.strip_prefix(prefix) {
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == ':')
                .collect();
            if !name.is_empty() {
                return Some((kind.to_string(), name));
            }
        }

        let pref_pub = format!("pub {prefix}");
        if let Some(rest) = t.strip_prefix(&pref_pub) {
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == ':')
                .collect();
            if !name.is_empty() {
                return Some((kind.to_string(), name));
            }
        }
    }
    None
}

fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

fn whole_word_regex(word: &str) -> Option<Regex> {
    if word.is_empty() {
        return None;
    }

    let pat = format!(r"\b{}\b", regex::escape(word));
    Regex::new(&pat).ok()
}

fn apply_ligatures(line: &str, enabled: bool) -> String {
    if !enabled {
        return line.to_string();
    }
    line.replace("->", "→")
        .replace("=>", "⇒")
        .replace("!=", "≠")
        .replace(">=", "≥")
        .replace("<=", "≤")
}

fn is_edit_key(key: Key) -> bool {
    matches!(
        key,
        Key::Char(_) | Key::Enter | Key::Tab | Key::Backspace | Key::Delete
    )
}

fn truncate_str(s: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }

    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let t: String = s.chars().take(max_chars.saturating_sub(1)).collect();
        format!("{t}…")
    }
}

fn pad_ansi_to_width(s: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let visible = visible_char_count(s);
    if visible >= width {
        return s.to_string();
    }
    let mut out = s.to_string();
    out.push_str(&" ".repeat(width - visible));
    out
}

fn visible_char_count(s: &str) -> usize {
    let mut count = 0usize;
    let bytes = s.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == 0x1b {
            i += 1;
            if i < bytes.len() && bytes[i] == b'[' {
                i += 1;
                while i < bytes.len() {
                    let b = bytes[i];
                    i += 1;
                    if (0x40..=0x7e).contains(&b) {
                        break;
                    }
                }
                continue;
            }
        }
        if let Some(ch) = s[i..].chars().next() {
            i += ch.len_utf8();
            count += 1;
        } else {
            break;
        }
    }
    count
}
