use std::fs;
use std::io::{self, Read, Write};
use std::os::unix::io::AsRawFd;
use std::path::PathBuf;

use crate::buffer::Buffer;
use crate::terminal::{read_timeout, winsize_tty, RawMode, TermSize};

#[derive(Debug)]
pub enum Key {
    Char(char),
    Enter,
    Tab,
    Backspace,
    Delete,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Home,
    End,
    PageUp,
    PageDown,
    CtrlS,
    CtrlQ,
    Esc,
}

pub struct Editor {
    pub buffer: Buffer,
    pub row: usize,
    pub col: usize,
    pub scroll_row: usize,
    pub hscroll: usize,
    pub path: Option<PathBuf>,
    pub dirty: bool,
    /// After `Ctrl+Q` on a dirty buffer, set until another key or second `Ctrl+Q`
    force_quit_pending: bool,
}

impl Editor {
    pub fn new(path: Option<PathBuf>) -> io::Result<Self> {
        let (buffer, path) = match &path {
            Some(p) => (Buffer::from_file(&fs::read_to_string(p)?), Some(p.clone())),
            None => (Buffer::new(), None),
        };
        Ok(Self {
            buffer,
            row: 0,
            col: 0,
            scroll_row: 0,
            hscroll: 0,
            path,
            dirty: false,
            force_quit_pending: false,
        })
    }

    pub fn run(&mut self) -> io::Result<()> {
        let _raw = RawMode::enable_stdin()?;
        let stdin_fd = std::io::stdin().as_raw_fd();

        write!(
            io::stdout(),
            "\x1b[?25l\x1b[?7l"
        )?;
        io::stdout().flush()?;

        let result = (|| -> io::Result<()> {
            loop {
                self.clamp_cursor();
                self.adjust_scroll();
                self.render()?;

                let key = match read_key(stdin_fd)? {
                    Some(k) => k,
                    None => continue,
                };

                if self.handle_key(key)? {
                    break;
                }
            }
            Ok(())
        })();

        write!(io::stdout(), "\x1b[?25h\x1b[?7h\x1b[m\r\n")?;
        io::stdout().flush()?;

        result
    }

    fn clamp_cursor(&mut self) {
        let n = self.buffer.line_count();
        if self.row >= n {
            self.row = n.saturating_sub(1);
        }

        let len = self.buffer.line_len_chars(self.row);
        if self.col > len {
            self.col = len;
        }
    }

    fn adjust_scroll(&mut self) {
        let size = winsize_tty().unwrap_or(TermSize { rows: 24, cols: 80 });
        let content_h = size.rows.saturating_sub(1) as usize;
        if content_h == 0 {
            return;
        }

        if self.row < self.scroll_row {
            self.scroll_row = self.row;
        }

        if self.row >= self.scroll_row + content_h {
            self.scroll_row = self.row + 1 - content_h;
        }

        let w = size.cols as usize;
        if w == 0 {
            return;
        }

        if self.col < self.hscroll {
            self.hscroll = self.col;
        }

        if self.col >= self.hscroll + w {
            self.hscroll = self.col + 1 - w;
        }
    }

    fn render(&self) -> io::Result<()> {
        let size = winsize_tty().unwrap_or(TermSize { rows: 24, cols: 80 });
        let rows = size.rows.max(1);
        let cols = size.cols.max(1) as usize;
        let content_h = (rows - 1) as usize;

        let mut out = String::with_capacity((rows as usize) * (cols + 16));

        out.push_str("\x1b[H\x1b[J");

        for i in 0..content_h {
            let line_idx = self.scroll_row + i;
            let piece = self
                .buffer
                .lines()
                .get(line_idx)
                .map(|s| {
                    let skip = self.hscroll.min(s.chars().count());
                    let rest: String = s.chars().skip(skip).take(cols).collect();
                    rest
                })
                .unwrap_or_default();
            out.push_str(&piece);
            out.push_str("\r\n");
        }

        let path_str = self
            .path
            .as_ref()
            .and_then(|p| p.to_str())
            .unwrap_or("[new]");
        let dirty = if self.dirty { " *" } else { "" };
        let quit_hint = if self.force_quit_pending {
            " Ctrl+Q again quit "
        } else {
            " Ctrl+Q quit "
        };

        let status = format!(
            "\x1b[7m {} | {}:{} |{}{}Ctrl+S save\x1b[m",
            path_str,
            self.row.saturating_add(1),
            self.col.saturating_add(1),
            dirty,
            quit_hint
        );
        let status: String = status.chars().take(cols).collect();
        out.push_str(&status);

        let screen_row = 1 + (self.row.saturating_sub(self.scroll_row)) as u32;
        let screen_col = 1 + (self.col.saturating_sub(self.hscroll)) as u32;
        out.push_str(&format!("\x1b[{};{}H", screen_row, screen_col));

        let mut stdout = io::stdout().lock();
        stdout.write_all(out.as_bytes())?;
        stdout.flush()?;
        Ok(())
    }

    /// Returns `true` if should exit.
    fn handle_key(&mut self, key: Key) -> io::Result<bool> {
        match key {
            Key::CtrlQ => {
                if self.dirty && !self.force_quit_pending {
                    self.force_quit_pending = true;
                    return Ok(false);
                }
                return Ok(true);
            }
            Key::CtrlS => {
                if let Some(ref p) = self.path {
                    fs::write(p, self.buffer.to_file_string())?;
                    self.dirty = false;
                }
            }
            Key::Char(ch) => {
                let (r, c) = self.buffer.insert_char(self.row, self.col, ch);
                self.row = r;
                self.col = c;
                self.dirty = true;
            }
            Key::Enter => {
                let (r, c) = self.buffer.insert_char(self.row, self.col, '\n');
                self.row = r;
                self.col = c;
                self.dirty = true;
            }
            Key::Tab => {
                let (r, c) = self.buffer.insert_char(self.row, self.col, '\t');
                self.row = r;
                self.col = c;
                self.dirty = true;
            }
            Key::Backspace => {
                if let Some((r, c)) = self.buffer.backspace(self.row, self.col) {
                    self.row = r;
                    self.col = c;
                    self.dirty = true;
                }
            }
            Key::Delete => {
                if let Some((r, c)) = self.buffer.delete_forward(self.row, self.col) {
                    self.row = r;
                    self.col = c;
                    self.dirty = true;
                }
            }
            Key::ArrowLeft => {
                if self.col > 0 {
                    self.col -= 1;
                } else if self.row > 0 {
                    self.row -= 1;
                    self.col = self.buffer.line_len_chars(self.row);
                }
            }
            Key::ArrowRight => {
                let len = self.buffer.line_len_chars(self.row);
                if self.col < len {
                    self.col += 1;
                } else if self.row + 1 < self.buffer.line_count() {
                    self.row += 1;
                    self.col = 0;
                }
            }
            Key::ArrowUp => {
                if self.row > 0 {
                    self.row -= 1;
                    self.col = self.col.min(self.buffer.line_len_chars(self.row));
                }
            }
            Key::ArrowDown => {
                if self.row + 1 < self.buffer.line_count() {
                    self.row += 1;
                    self.col = self.col.min(self.buffer.line_len_chars(self.row));
                }
            }
            Key::Home => self.col = 0,
            Key::End => self.col = self.buffer.line_len_chars(self.row),
            Key::PageUp => {
                let size = winsize_tty().unwrap_or(TermSize { rows: 24, cols: 80 });
                let step = (size.rows.saturating_sub(1)).max(1) as usize;
                self.row = self.row.saturating_sub(step);
            }
            Key::PageDown => {
                let size = winsize_tty().unwrap_or(TermSize { rows: 24, cols: 80 });
                let step = (size.rows.saturating_sub(1)).max(1) as usize;
                self.row = (self.row + step).min(self.buffer.line_count().saturating_sub(1));
                self.col = self.col.min(self.buffer.line_len_chars(self.row));
            }
            Key::Esc => {}
        }
        self.force_quit_pending = false;
        Ok(false)
    }
}

fn read_key(stdin_fd: std::os::unix::io::RawFd) -> io::Result<Option<Key>> {
    let mut b = [0u8; 1];
    let n = std::io::stdin().read(&mut b)?;
    if n == 0 {
        return Ok(None);
    }
    let byte = b[0];

    if byte == 0x1b {
        return parse_escape(stdin_fd);
    }

    if byte == 127 || byte == 8 {
        return Ok(Some(Key::Backspace));
    }

    if byte == b'\r' || byte == b'\n' {
        return Ok(Some(Key::Enter));
    }

    if byte == 9 {
        return Ok(Some(Key::Tab));
    }

    if byte < 32 {
        if byte == 19 {
            return Ok(Some(Key::CtrlS));
        }
        
        if byte == 17 {
            return Ok(Some(Key::CtrlQ));
        }

        return Ok(None);
    }

    let ch = char::from_u32(byte as u32).unwrap_or('\u{fffd}');
    Ok(Some(Key::Char(ch)))
}

fn parse_escape(stdin_fd: std::os::unix::io::RawFd) -> io::Result<Option<Key>> {
    let mut seq = Vec::<u8>::new();
    let mut scratch = [0u8; 64];
    for _ in 0..4 {
        let n = read_timeout(stdin_fd, &mut scratch, 50)?;
        if n == 0 {
            break;
        }

        seq.extend_from_slice(&scratch[..n]);
        if seq.len() > 48 {
            break;
        }
    }

    if seq.is_empty() {
        return Ok(Some(Key::Esc));
    }

    // SS3: ESC O A (arrow keys on some terminals)
    if seq[0] == b'O' && seq.len() >= 2 {
        return Ok(Some(match seq[1] {
            b'A' => Key::ArrowUp,
            b'B' => Key::ArrowDown,
            b'C' => Key::ArrowRight,
            b'D' => Key::ArrowLeft,
            b'H' => Key::Home,
            b'F' => Key::End,
            _ => Key::Esc,
        }));
    }

    if seq[0] != b'[' {
        return Ok(Some(Key::Esc));
    }

    let body = &seq[1..];
    if body.is_empty() {
        return Ok(Some(Key::Esc));
    }

    match body[0] {
        b'A' => return Ok(Some(Key::ArrowUp)),
        b'B' => return Ok(Some(Key::ArrowDown)),
        b'C' => return Ok(Some(Key::ArrowRight)),
        b'D' => return Ok(Some(Key::ArrowLeft)),
        b'H' => return Ok(Some(Key::Home)),
        b'F' => return Ok(Some(Key::End)),
        _ => {}
    }

    if body.len() >= 2 && *body.last().unwrap_or(&0) == b'~' {
        let lead = body[0];
        match lead {
            b'1' | b'7' => return Ok(Some(Key::Home)),
            b'2' => return Ok(Some(Key::Esc)),
            b'3' => return Ok(Some(Key::Delete)),
            b'4' | b'8' => return Ok(Some(Key::End)),
            b'5' => return Ok(Some(Key::PageUp)),
            b'6' => return Ok(Some(Key::PageDown)),
            _ => {}
        }
    }
    Ok(Some(Key::Esc))
}
