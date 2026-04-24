use std::io::{self, Write};
use std::path::PathBuf;

use crate::keys::Key;
use crate::recents;
use crate::terminal::{winsize_tty, TermSize};

pub enum WelcomeAction {
    OpenProject(PathBuf),
    Quit,
    None,
}

pub struct Welcome {
    pub recents: Vec<PathBuf>,
    pub selected: usize,
    /// `Some` = user typed path for a new / existing folder
    pub path_input: Option<String>,
}

impl Welcome {
    pub fn new() -> Self {
        Self {
            recents: recents::load(),
            selected: 0,
            path_input: None,
        }
    }

    pub fn render(&self) -> io::Result<()> {
        let size = winsize_tty().unwrap_or(TermSize { rows: 24, cols: 80 });
        let rows = size.rows.max(1) as usize;
        let cols = size.cols.max(1) as usize;
        let content_h = rows.saturating_sub(1).max(1);

        let mut lines: Vec<String> = Vec::new();
        lines.push("\x1b[1m Tce \x1b[0m - проекты".to_string());
        lines.push(String::new());
        lines.push("Недавние проекты:".to_string());

        if self.recents.is_empty() {
            lines.push("  (пусто - нажми N и введи путь к папке)".to_string());
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
        lines.push(
            "Enter - открыть  |  N - путь к папке  |  Ctrl+Q - выход".to_string(),
        );
        if self.path_input.is_some() {
            lines.push(String::new());
            lines.push("\x1b[7m Путь к папке проекта: \x1b[m".to_string());
            if let Some(ref p) = self.path_input {
                lines.push(truncate_str(p, cols.saturating_sub(2)));
            }
        }

        while lines.len() < content_h {
            lines.push(String::new());
        }
        lines.truncate(content_h);

        let status = format!(
            "\x1b[7m welcome | поз. {} |{} \x1b[m",
            self.selected.saturating_add(1),
            if self.path_input.is_some() {
                " Enter - открыть  Esc - отмена "
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
                            WelcomeAction::OpenProject(canon)
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
                _ => {
                    self.path_input = Some(buf);
                    WelcomeAction::None
                }
            };
            return Ok(action);
        }

        match key {
            Key::CtrlQ => return Ok(WelcomeAction::Quit),
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
                    return Ok(WelcomeAction::OpenProject(p));
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
