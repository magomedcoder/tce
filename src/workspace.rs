use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::document::Document;
use crate::keys::Key;
use crate::localization::{texts, Language};
use crate::recents;
use crate::terminal::{winsize_tty, TermSize};
use crate::tree::{self as filetree, TreeEntry};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Focus {
    Editor,
    Sidebar,
}

pub struct Workspace {
    pub project_root: PathBuf,
    tree: Vec<TreeEntry>,
    tree_sel: usize,
    tree_scroll: usize,
    pub doc: Document,
    sidebar_visible: bool,
    focus: Focus,
    tip: Option<String>,
    language: Language,
    language_picker: bool,
    language_sel: usize,
    hotkeys_help: bool,
}

impl Workspace {
    pub fn open_project(root: PathBuf) -> io::Result<Self> {
        let _ = recents::push_front(root.clone());
        let tree = filetree::build_tree(&root)?;
        let tree_sel = tree
            .iter()
            .position(|e| !e.is_dir)
            .unwrap_or(0)
            .min(tree.len().saturating_sub(1));
        Ok(Self {
            project_root: root,
            tree,
            tree_sel,
            tree_scroll: 0,
            doc: Document::empty(),
            sidebar_visible: true,
            focus: Focus::Editor,
            tip: None,
            language: Language::En,
            language_picker: false,
            language_sel: 0,
            hotkeys_help: false,
        })
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
        ws.doc = Document::open_file(file)?;
        ws.tree_sel = ws
            .tree
            .iter()
            .position(|e| e.path == file_canon)
            .unwrap_or(ws.tree_sel);
        ws.focus = Focus::Editor;
        Ok(ws)
    }

    pub fn open_dir(dir: PathBuf) -> io::Result<Self> {
        let root = dir.canonicalize().unwrap_or(dir);
        Self::open_project(root)
    }

    pub fn set_language(&mut self, language: Language) {
        self.language = language;
    }

    fn sidebar_width_cols(term_cols: usize) -> usize {
        if term_cols < 48 {
            return term_cols.min(20).max(12);
        }
        (term_cols / 4).clamp(18, 36)
    }

    fn editor_width(term_cols: usize, sidebar_visible: bool) -> usize {
        if !sidebar_visible {
            return term_cols.max(1);
        }
        let sw = Self::sidebar_width_cols(term_cols);
        term_cols.saturating_sub(sw + 1).max(12)
    }

    pub fn render(&mut self) -> io::Result<()> {
        if self.hotkeys_help {
            return self.render_hotkeys_help();
        }
        if self.language_picker {
            return self.render_language_picker();
        }
        self.doc.clamp_cursor();
        let size = winsize_tty().unwrap_or(TermSize { rows: 24, cols: 80 });
        let rows = size.rows.max(1) as usize;
        let cols = size.cols.max(1) as usize;
        let content_h = rows.saturating_sub(1).max(1);
        let sidebar_w = Self::sidebar_width_cols(cols);
        let editor_w = Self::editor_width(cols, self.sidebar_visible);

        self.doc.adjust_scroll(content_h, editor_w.max(1));
        self.adjust_tree_scroll(content_h);

        let mut out = String::with_capacity(rows * (cols + 32));
        out.push_str("\x1b[H\x1b[J");

        for row in 0..content_h {
            if self.sidebar_visible && cols > sidebar_w + 4 {
                let line = self.sidebar_line(row, content_h, sidebar_w);
                out.push_str(&line);
                out.push_str("\x1b[0m│");
                let text = self
                    .doc
                    .editor_line_display(self.doc.scroll_row + row, editor_w);
                let clipped: String = text.chars().take(editor_w).collect();
                out.push_str(&clipped);
            } else {
                let text = self
                    .doc
                    .editor_line_display(self.doc.scroll_row + row, cols);
                let clipped: String = text.chars().take(cols).collect();
                out.push_str(&clipped);
            }
            out.push_str("\r\n");
        }

        let proj = self.project_root.to_string_lossy();
        let dirty = if self.doc.dirty { " *" } else { "" };
        let tx = texts(self.language);
        let quit_hint = if self.doc.force_quit_pending {
            format!(" {} ", tx.hint_ctrl_q_again_quit)
        } else {
            format!(" {} ", tx.hint_ctrl_q_quit)
        };
        let tip = self
            .tip
            .as_deref()
            .unwrap_or(tx.hint_sidebar_focus);
        let status = format!(
            "\x1b[7m {} | {} | {}:{} |{}{}{} | {} {} {} {} |{}\x1b[m",
            truncate_str(&proj, 18),
            truncate_str(&self.doc.path_display(), 22),
            self.doc.row.saturating_add(1),
            self.doc.col.saturating_add(1),
            dirty,
            quit_hint,
            tx.hint_ctrl_s_save,
            tx.hint_ctrl_b,
            tx.hint_shift_tab,
            tx.hint_ctrl_l_lang,
            tx.hint_ctrl_k_help,
            truncate_str(tip, cols.saturating_sub(78))
        );
        let status: String = status.chars().take(cols).collect();
        out.push_str(&status);

        let (sr, sc) = self.cursor_screen_pos(content_h, cols, sidebar_w, editor_w);
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
        _editor_w: usize,
    ) -> (u32, u32) {
        if self.focus == Focus::Sidebar && self.sidebar_visible && cols > sidebar_w + 4 {
            let vis = self.tree_sel.saturating_sub(self.tree_scroll);
            let r = (vis.min(content_h.saturating_sub(1)) + 1) as u32;
            let c = (self.tree.get(self.tree_sel).map(|e| e.depth * 2 + 2).unwrap_or(2) as u32).min(sidebar_w as u32);
            (r, c.max(1))
        } else {
            let doc_row = self.doc.row.saturating_sub(self.doc.scroll_row);
            let r = (doc_row.min(content_h.saturating_sub(1)) + 1) as u32;
            let col_off = if self.sidebar_visible && cols > sidebar_w + 4 {
                (sidebar_w + 2) as u32
            } else {
                0
            };
            let c = col_off + (self.doc.col.saturating_sub(self.doc.hscroll) as u32) + 1;
            (r, c.max(1))
        }
    }

    fn sidebar_line(&self, row: usize, _content_h: usize, sidebar_w: usize) -> String {
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
        if self.tree_sel < self.tree_scroll {
            self.tree_scroll = self.tree_sel;
        }
        if self.tree_sel >= self.tree_scroll + content_h {
            self.tree_scroll = self.tree_sel + 1 - content_h;
        }
    }

    /// `true` = quit app
    pub fn handle_key(&mut self, key: Key) -> io::Result<bool> {
        if self.hotkeys_help {
            match key {
                Key::CtrlK => {
                    self.hotkeys_help = false;
                }
                Key::CtrlQ => return self.doc.handle_key(key),
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
                }
                Key::CtrlL => {
                    self.language_picker = false;
                }
                Key::CtrlQ => return self.doc.handle_key(key),
                _ => {}
            }
            return Ok(false);
        }

        self.tip = None;

        if matches!(key, Key::CtrlQ) {
            return self.doc.handle_key(key);
        }
        if matches!(key, Key::CtrlS) {
            self.doc.save()?;
            return Ok(false);
        }

        if matches!(key, Key::CtrlB) {
            self.sidebar_visible = !self.sidebar_visible;
            if !self.sidebar_visible {
                self.focus = Focus::Editor;
            }
            return Ok(false);
        }

        if matches!(key, Key::CtrlL) {
            self.language_picker = true;
            self.language_sel = if self.language == Language::En { 0 } else { 1 };
            return Ok(false);
        }
        if matches!(key, Key::CtrlK) {
            self.hotkeys_help = true;
            return Ok(false);
        }

        if matches!(key, Key::ShiftTab) {
            if self.sidebar_visible {
                self.focus = match self.focus {
                    Focus::Editor => Focus::Sidebar,
                    Focus::Sidebar => Focus::Editor,
                };
            }
            return Ok(false);
        }

        if self.sidebar_visible && self.focus == Focus::Sidebar {
            self.handle_sidebar_key(key);
            return Ok(false);
        }

        if self.sidebar_visible && matches!(key, Key::Tab) {
            self.focus = Focus::Editor;
            return Ok(false);
        }

        self.doc.handle_key(key)?;
        Ok(false)
    }

    fn handle_sidebar_key(&mut self, key: Key) {
        match key {
            Key::Tab => {
                self.focus = Focus::Editor;
            }
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
                if let Some(e) = self.tree.get(self.tree_sel) {
                    if e.is_dir {
                        return;
                    }

                    if self.doc.dirty {
                        self.tip = Some(texts(self.language).save_or_quit_double.into());
                        return;
                    }

                    if let Err(err) = self.doc.load_file(e.path.clone()) {
                        self.tip = Some(format!("{}: {err}", texts(self.language).error_prefix));
                    } else {
                        self.focus = Focus::Editor;
                    }
                }
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
