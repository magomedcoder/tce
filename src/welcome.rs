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

pub struct Welcome {
    pub recents: Vec<PathBuf>,
    pub selected: usize,
    /// `Some` = user typed path for a new / existing folder
    pub path_input: Option<String>,
    pub language: Language,
    pub language_picker: bool,
    pub language_sel: usize,
    pub hotkeys_help: bool,
}

impl Welcome {
    pub fn new() -> Self {
        Self {
            recents: recents::load(),
            selected: 0,
            path_input: None,
            language: Language::En,
            language_picker: false,
            language_sel: 0,
            hotkeys_help: false,
        }
    }

    pub fn render(&self) -> io::Result<()> {
        if self.hotkeys_help {
            return self.render_hotkeys_help();
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
        lines.push(String::new());
        lines.push(tx.welcome_open_new_quit.to_string());
        if self.path_input.is_some() {
            lines.push(String::new());
            lines.push(tx.welcome_path_prompt.to_string());
            if let Some(ref p) = self.path_input {
                lines.push(truncate_str(p, cols.saturating_sub(2)));
            }
        }

        while lines.len() < content_h {
            lines.push(String::new());
        }
        lines.truncate(content_h);

        let status = format!(
            "\x1b[7m welcome | {} {} |{} \x1b[m",
            tx.welcome_status_pos,
            self.selected.saturating_add(1),
            if self.path_input.is_some() {
                tx.welcome_status_enter_esc
            } else {
                " "
            }
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
                Key::Esc => {
                    self.language_picker = false;
                }
                Key::ArrowUp => {
                    self.language_sel = self.language_sel.saturating_sub(1);
                }
                Key::ArrowDown => {
                    self.language_sel = (self.language_sel + 1).min(1);
                }
                Key::Char('k') | Key::Char('K') => {
                    self.language_sel = self.language_sel.saturating_sub(1);
                }
                Key::Char('j') | Key::Char('J') => {
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
                Key::CtrlQ => return Ok(WelcomeAction::Quit),
                _ => {}
            }
            return Ok(WelcomeAction::None);
        }

        if self.hotkeys_help {
            match key {
                Key::Esc | Key::CtrlK => {
                    self.hotkeys_help = false;
                }
                Key::CtrlQ => return Ok(WelcomeAction::Quit),
                _ => {}
            }
            return Ok(WelcomeAction::None);
        }

        if self.path_input.is_some() {
            let mut buf = self.path_input.take().unwrap();
            let action = match key {
                Key::Esc => {
                    self.path_input = None;
                    WelcomeAction::None
                }
                Key::Enter => {
                    let raw = buf.trim();
                    if raw.is_empty() {
                        self.path_input = None;
                        WelcomeAction::None
                    } else {
                        let pb = PathBuf::from(raw);
                        if pb.exists() && !pb.is_dir() {
                            self.path_input = None;
                            WelcomeAction::None
                        } else {
                            if !pb.exists() {
                                std::fs::create_dir_all(&pb)?;
                            }
                            let canon = pb.canonicalize().unwrap_or(pb);
                            let _ = recents::push_front(canon.clone());
                            self.path_input = None;
                            WelcomeAction::OpenProject(canon, self.language)
                        }
                    }
                }
                Key::Backspace => {
                    buf.pop();
                    self.path_input = Some(buf);
                    WelcomeAction::None
                }
                Key::Char(ch) => {
                    buf.push(ch);
                    self.path_input = Some(buf);
                    WelcomeAction::None
                }
                Key::CtrlQ => return Ok(WelcomeAction::Quit),
                Key::CtrlL => {
                    self.language_picker = true;
                    self.language_sel = if self.language == Language::En { 0 } else { 1 };
                    self.path_input = Some(buf);
                    WelcomeAction::None
                }
                Key::CtrlK => {
                    self.hotkeys_help = true;
                    self.path_input = Some(buf);
                    WelcomeAction::None
                }
                _ => {
                    self.path_input = Some(buf);
                    WelcomeAction::None
                }
            };
            return Ok(action);
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
                self.path_input = Some(String::new());
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
