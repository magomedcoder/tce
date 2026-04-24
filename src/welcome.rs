use std::io::{self, Write};
use std::path::PathBuf;

use crate::keys::Key;
use crate::localization::{texts, Language};
use crate::recents;
use crate::terminal::{winsize_tty, TermSize};

pub enum WelcomeAction {
    OpenProject(PathBuf, Language),
    Quit,
    None,
}

enum FolderEntry {
    OpenCurrent,
    GoHome,
    GoRoot,
    GoUp,
    Dir(PathBuf),
}

pub struct Welcome {
    pub recents: Vec<PathBuf>,
    pub selected: usize,
    pub language: Language,
    pub language_picker: bool,
    pub language_sel: usize,
    pub hotkeys_help: bool,
    folder_browser: bool,
    path_input: Option<String>,
    path_suggestions: Vec<PathBuf>,
    path_suggestion_sel: usize,
    browse_dir: PathBuf,
    browse_items: Vec<FolderEntry>,
    browse_sel: usize,
    browse_scroll: usize,
}

impl Welcome {
    pub fn new() -> Self {
        let browse_dir = home_dir();
        let browse_items = build_folder_items(&browse_dir);
        Self {
            recents: recents::load(),
            selected: 0,
            language: Language::En,
            language_picker: false,
            language_sel: 0,
            hotkeys_help: false,
            folder_browser: false,
            path_input: None,
            path_suggestions: Vec::new(),
            path_suggestion_sel: 0,
            browse_dir,
            browse_items,
            browse_sel: 0,
            browse_scroll: 0,
        }
    }

    pub fn render(&self) -> io::Result<()> {
        if self.hotkeys_help {
            return self.render_hotkeys_help();
        }

        if self.folder_browser {
            return self.render_folder_browser();
        }

        if self.language_picker {
            return self.render_language_picker();
        }

        let size = winsize_tty().unwrap_or(TermSize { rows: 24, cols: 80 });
        let rows = size.rows.max(1) as usize;
        let cols = size.cols.max(1) as usize;
        let content_h = rows.saturating_sub(1).max(1);

        let tx = texts(self.language);
        let mut lines: Vec<String> = Vec::new();
        lines.push(tx.welcome_title.to_string());
        lines.push(String::new());
        lines.push(tx.welcome_recents.to_string());

        if self.recents.is_empty() {
            lines.push(tx.welcome_empty.to_string());
        } else {
            let max_list = content_h.saturating_sub(8).max(1);
            for (i, p) in self.recents.iter().enumerate().take(max_list) {
                let mark = if i == self.selected { "› " } else { "  " };
                let s = p.to_string_lossy();
                lines.push(format!(
                    "{mark}{}",
                    truncate_str(&s, cols.saturating_sub(4))
                ));
            }
        }
        lines.push(tx.welcome_open_new_quit.to_string());

        while lines.len() < content_h {
            lines.push(String::new());
        }
        lines.truncate(content_h);

        let status = format!(
            "\x1b[7m welcome | {} {} \x1b[m",
            tx.welcome_status_pos,
            self.selected.saturating_add(1),
        );
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

    pub fn handle_key(&mut self, key: Key) -> io::Result<WelcomeAction> {
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
                Key::CtrlQ => return Ok(WelcomeAction::Quit),
                _ => {}
            }
            return Ok(WelcomeAction::None);
        }

        if self.hotkeys_help {
            match key {
                Key::CtrlK => {
                    self.hotkeys_help = false;
                }
                Key::CtrlQ => return Ok(WelcomeAction::Quit),
                _ => {}
            }
            return Ok(WelcomeAction::None);
        }

        if self.folder_browser {
            match key {
                _ if self.path_input.is_some() => {
                    let mut buf = self.path_input.take().unwrap_or_default();
                    match key {
                        Key::Backspace => {
                            buf.pop();
                            self.path_input = Some(buf);
                            self.refresh_path_suggestions();
                        }
                        Key::ArrowUp => {
                            if !self.path_suggestions.is_empty() {
                                self.path_suggestion_sel =
                                    self.path_suggestion_sel.saturating_sub(1);
                            }
                            self.path_input = Some(buf);
                        }
                        Key::ArrowDown => {
                            if !self.path_suggestions.is_empty()
                                && self.path_suggestion_sel + 1 < self.path_suggestions.len()
                            {
                                self.path_suggestion_sel += 1;
                            }
                            self.path_input = Some(buf);
                        }
                        Key::Tab => {
                            if let Some(s) = self.path_suggestions.get(self.path_suggestion_sel) {
                                buf = s.to_string_lossy().to_string();
                            }
                            self.path_input = Some(buf);
                            self.refresh_path_suggestions();
                        }
                        Key::Enter => {
                            if let Some(s) = self.path_suggestions.get(self.path_suggestion_sel) {
                                buf = s.to_string_lossy().to_string();
                            }
                            let raw = buf.trim();
                            if !raw.is_empty() {
                                let pb = PathBuf::from(raw);
                                if pb.exists() && pb.is_dir() {
                                    let canon = pb.canonicalize().unwrap_or(pb);
                                    return Ok(WelcomeAction::OpenProject(canon, self.language));
                                }
                                if !pb.exists() {
                                    std::fs::create_dir_all(&pb)?;
                                    let canon = pb.canonicalize().unwrap_or(pb);
                                    return Ok(WelcomeAction::OpenProject(canon, self.language));
                                }
                            }
                            self.path_input = None;
                            self.path_suggestions.clear();
                            self.path_suggestion_sel = 0;
                        }
                        Key::CtrlN | Key::CtrlQ | Key::CtrlL | Key::CtrlK => {
                            self.path_input = Some(buf);
                        }
                        Key::Char(ch) => {
                            buf.push(ch);
                            self.path_input = Some(buf);
                            self.refresh_path_suggestions();
                        }
                        _ => {
                            self.path_input = Some(buf);
                        }
                    }
                    return Ok(WelcomeAction::None);
                }
                Key::CtrlN | Key::Char('n') | Key::Char('N') => {
                    self.folder_browser = false;
                    self.path_input = None;
                    self.path_suggestions.clear();
                    self.path_suggestion_sel = 0;
                }
                Key::ArrowUp => {
                    self.browse_sel = self.browse_sel.saturating_sub(1);
                    self.adjust_browse_scroll();
                }
                Key::ArrowDown => {
                    if self.browse_sel + 1 < self.browse_items.len() {
                        self.browse_sel += 1;
                    }
                    self.adjust_browse_scroll();
                }
                Key::Home => {
                    self.browse_sel = 0;
                    self.adjust_browse_scroll();
                }
                Key::End => {
                    if !self.browse_items.is_empty() {
                        self.browse_sel = self.browse_items.len() - 1;
                    }
                    self.adjust_browse_scroll();
                }
                Key::Enter => {
                    if let Some(item) = self.browse_items.get(self.browse_sel) {
                        match item {
                            FolderEntry::OpenCurrent => {
                                return Ok(WelcomeAction::OpenProject(
                                    self.browse_dir.clone(),
                                    self.language,
                                ));
                            }
                            FolderEntry::GoHome => {
                                self.browse_dir = home_dir();
                                self.browse_items = build_folder_items(&self.browse_dir);
                                self.browse_sel = 0;
                                self.browse_scroll = 0;
                            }
                            FolderEntry::GoRoot => {
                                self.browse_dir = PathBuf::from("/");
                                self.browse_items = build_folder_items(&self.browse_dir);
                                self.browse_sel = 0;
                                self.browse_scroll = 0;
                            }
                            FolderEntry::GoUp => {
                                if let Some(parent) = self.browse_dir.parent() {
                                    self.browse_dir = parent.to_path_buf();
                                    self.browse_items = build_folder_items(&self.browse_dir);
                                    self.browse_sel = 0;
                                    self.browse_scroll = 0;
                                }
                            }
                            FolderEntry::Dir(path) => {
                                self.browse_dir = path.clone();
                                self.browse_items = build_folder_items(&self.browse_dir);
                                self.browse_sel = 0;
                                self.browse_scroll = 0;
                            }
                        }
                    }
                }
                Key::CtrlQ => return Ok(WelcomeAction::Quit),
                Key::CtrlL => {
                    self.language_picker = true;
                    self.language_sel = if self.language == Language::En { 0 } else { 1 };
                }
                Key::CtrlK => {
                    self.hotkeys_help = true;
                }
                Key::Char('p') | Key::Char('P') | Key::Char('/') => {
                    self.path_input = Some(String::new());
                    self.refresh_path_suggestions();
                }
                _ => {}
            }
            return Ok(WelcomeAction::None);
        }

        match key {
            Key::CtrlQ => return Ok(WelcomeAction::Quit),
            Key::CtrlL => {
                self.language_picker = true;
                self.language_sel = if self.language == Language::En { 0 } else { 1 };
            }
            Key::CtrlK => {
                self.hotkeys_help = true;
            }
            Key::CtrlN | Key::Char('n') | Key::Char('N') => {
                self.folder_browser = true;
                self.adjust_browse_scroll();
            }
            Key::ArrowUp => {
                if self.selected > 0 {
                    self.selected -= 1;
                }
            }
            Key::ArrowDown => {
                if self.selected + 1 < self.recents.len() {
                    self.selected += 1;
                }
            }
            Key::Enter => {
                if let Some(p) = self.recents.get(self.selected).cloned() {
                    return Ok(WelcomeAction::OpenProject(p, self.language));
                }
            }
            Key::Home => self.selected = 0,
            Key::End => {
                if !self.recents.is_empty() {
                    self.selected = self.recents.len() - 1;
                }
            }
            _ => {}
        }
        Ok(WelcomeAction::None)
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

    fn render_folder_browser(&self) -> io::Result<()> {
        let size = winsize_tty().unwrap_or(TermSize { rows: 24, cols: 80 });
        let rows = size.rows.max(1) as usize;
        let cols = size.cols.max(1) as usize;
        let content_h = rows.saturating_sub(1).max(1);
        let tx = texts(self.language);
        let mut lines: Vec<String> = Vec::new();
        lines.push(format!("\x1b[1m Tce \x1b[0m - {}", tx.welcome_folders));
        lines.push(String::new());
        let max_folders = content_h.saturating_sub(6).max(3);
        let start = self.browse_scroll.min(self.browse_items.len());
        let end = (start + max_folders).min(self.browse_items.len());
        for (i, item) in self.browse_items[start..end].iter().enumerate() {
            let idx = start + i;
            let selected = idx == self.browse_sel;
            lines.push(render_folder_item(item, selected, tx, cols.saturating_sub(4)));
        }

        lines.push(String::new());
        lines.push(truncate_str(
            &self.browse_dir.to_string_lossy(),
            cols.saturating_sub(2),
        ));

        if let Some(ref path) = self.path_input {
            lines.push(format!(
                "{} {}",
                tx.welcome_path_prompt,
                truncate_str(path, cols.saturating_sub(8))
            ));

            let max_suggestions = 3usize;
            for (i, sug) in self.path_suggestions.iter().take(max_suggestions).enumerate() {
                let label = truncate_str(&sug.to_string_lossy(), cols.saturating_sub(6));
                if i == self.path_suggestion_sel {
                    lines.push(format!("\x1b[7m> {}\x1b[0m", label));
                } else {
                    lines.push(format!("  {}", label));
                }
            }
        } else {
            lines.push(tx.welcome_manual_path_hint.to_string());
        }

        lines.push(tx.welcome_folder_hint.to_string());
        while lines.len() < content_h {
            lines.push(String::new());
        }

        lines.truncate(content_h);
        let status = format!("\x1b[7m folders | {} \x1b[m", tx.welcome_status_enter_esc);
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

    fn adjust_browse_scroll(&mut self) {
        let size = winsize_tty().unwrap_or(TermSize { rows: 24, cols: 80 });
        let rows = size.rows.max(1) as usize;
        let content_h = rows.saturating_sub(1).max(1);
        let max_visible = content_h.saturating_sub(6).max(3);
        if self.browse_sel < self.browse_scroll {
            self.browse_scroll = self.browse_sel;
        }

        if self.browse_sel >= self.browse_scroll + max_visible {
            self.browse_scroll = self.browse_sel + 1 - max_visible;
        }
    }

    fn refresh_path_suggestions(&mut self) {
        let input = self.path_input.as_deref().unwrap_or("").trim();
        self.path_suggestions = build_path_suggestions(input, 12);
        self.path_suggestion_sel = 0;
    }
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
        .unwrap_or_else(|| PathBuf::from("/"))
}

fn build_folder_items(dir: &PathBuf) -> Vec<FolderEntry> {
    let mut items = vec![FolderEntry::OpenCurrent, FolderEntry::GoHome, FolderEntry::GoRoot];
    if dir.parent().is_some() {
        items.push(FolderEntry::GoUp);
    }

    let mut dirs: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()
        .into_iter()
        .flat_map(|rd| rd.filter_map(Result::ok))
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort_by(|a, b| {
        let an = a.file_name().and_then(|s| s.to_str()).unwrap_or("");
        let bn = b.file_name().and_then(|s| s.to_str()).unwrap_or("");
        an.to_lowercase().cmp(&bn.to_lowercase())
    });

    for p in dirs {
        items.push(FolderEntry::Dir(p));
    }

    items
}

fn build_path_suggestions(input: &str, limit: usize) -> Vec<PathBuf> {
    if input.is_empty() {
        return Vec::new();
    }

    let expanded = expand_tilde(input);
    let ends_with_sep = expanded.ends_with('/');
    let input_path = PathBuf::from(&expanded);
    let (base_dir, needle) = if ends_with_sep {
        (input_path, String::new())
    } else {
        let base = input_path
            .parent()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        let name = input_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        (base, name)
    };

    let mut out = Vec::new();
    let needle_low = needle.to_lowercase();
    if let Ok(rd) = std::fs::read_dir(&base_dir) {
        for entry in rd.filter_map(Result::ok) {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if !name.to_lowercase().starts_with(&needle_low) {
                continue;
            }

            out.push(path);
            if out.len() >= limit {
                break;
            }
        }
    }
    out.sort_by(|a, b| {
        let an = a.file_name().and_then(|s| s.to_str()).unwrap_or("");
        let bn = b.file_name().and_then(|s| s.to_str()).unwrap_or("");
        an.to_lowercase().cmp(&bn.to_lowercase())
    });
    out
}

fn expand_tilde(input: &str) -> String {
    if input == "~" {
        return home_dir().to_string_lossy().to_string();
    }

    if let Some(rest) = input.strip_prefix("~/") {
        let mut s = home_dir().to_string_lossy().to_string();
        s.push('/');
        s.push_str(rest);
        return s;
    }

    input.to_string()
}

fn render_folder_item(item: &FolderEntry, selected: bool, tx: &crate::localization::Texts, max: usize) -> String {
    let label = match item {
        FolderEntry::OpenCurrent => tx.welcome_folder_open_current.to_string(),
        FolderEntry::GoHome => tx.welcome_folder_home.to_string(),
        FolderEntry::GoRoot => tx.welcome_folder_root.to_string(),
        FolderEntry::GoUp => tx.welcome_folder_up.to_string(),
        FolderEntry::Dir(p) => {
            let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("/");
            format!("{name}/")
        }
    };
    
    let clipped = truncate_str(&label, max);
    if selected {
        format!("\x1b[7m> {clipped}\x1b[0m")
    } else {
        format!("  {clipped}")
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
