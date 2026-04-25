use std::io;

use crate::terminal::read_timeout;

#[derive(Clone, Copy, Debug)]
pub enum Key {
    Char(char),
    Enter,
    Tab,
    ShiftTab,
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
    CtrlA,
    CtrlT,
    CtrlY,
    CtrlD,
    CtrlE,
    CtrlC,
    CtrlV,
    CtrlF,
    CtrlG,
    CtrlR,
    CtrlQ,
    CtrlB,
    CtrlL,
    CtrlJ,
    CtrlK,
    CtrlN,
    CtrlO,
    CtrlU,
    CtrlW,
    CtrlP,
    CtrlX,
    CtrlZ,
    Esc,
}

pub fn read_key(stdin_fd: std::os::unix::io::RawFd) -> io::Result<Option<Key>> {
    let byte = match read_one_byte(stdin_fd)? {
        Some(b) => b,
        None => return Ok(None),
    };

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

        if byte == 1 {
            return Ok(Some(Key::CtrlA));
        }

        if byte == 20 {
            return Ok(Some(Key::CtrlT));
        }

        if byte == 25 {
            return Ok(Some(Key::CtrlY));
        }

        if byte == 4 {
            return Ok(Some(Key::CtrlD));
        }

        if byte == 5 {
            return Ok(Some(Key::CtrlE));
        }

        if byte == 3 {
            return Ok(Some(Key::CtrlC));
        }

        if byte == 22 {
            return Ok(Some(Key::CtrlV));
        }

        if byte == 6 {
            return Ok(Some(Key::CtrlF));
        }

        if byte == 7 {
            return Ok(Some(Key::CtrlG));
        }

        if byte == 18 {
            return Ok(Some(Key::CtrlR));
        }

        if byte == 17 {
            return Ok(Some(Key::CtrlQ));
        }

        if byte == 2 {
            return Ok(Some(Key::CtrlB));
        }

        if byte == 12 {
            return Ok(Some(Key::CtrlL));
        }

        if byte == 10 {
            return Ok(Some(Key::CtrlJ));
        }

        if byte == 11 {
            return Ok(Some(Key::CtrlK));
        }

        if byte == 14 {
            return Ok(Some(Key::CtrlN));
        }

        if byte == 15 {
            return Ok(Some(Key::CtrlO));
        }

        if byte == 21 {
            return Ok(Some(Key::CtrlU));
        }

        if byte == 23 {
            return Ok(Some(Key::CtrlW));
        }

        if byte == 24 {
            return Ok(Some(Key::CtrlX));
        }

        if byte == 26 {
            return Ok(Some(Key::CtrlZ));
        }

        if byte == 16 {
            return Ok(Some(Key::CtrlP));
        }

        return Ok(None);
    }

    let needed = utf8_char_len(byte);
    if needed == 1 {
        return Ok(Some(Key::Char(char::from_u32(byte as u32).unwrap_or('\u{fffd}'))));
    }

    let mut buf = [0u8; 4];
    buf[0] = byte;
    for i in 1..needed {
        match read_one_byte(stdin_fd)? {
            Some(b) => buf[i] = b,
            None => return Ok(Some(Key::Char('\u{fffd}'))),
        }
    }

    let ch = std::str::from_utf8(&buf[..needed])
        .ok()
        .and_then(|s| s.chars().next())
        .unwrap_or('\u{fffd}');
    Ok(Some(Key::Char(ch)))
}

fn read_one_byte(fd: std::os::unix::io::RawFd) -> io::Result<Option<u8>> {
    let mut b = [0u8; 1];
    let n = unsafe { libc::read(fd, b.as_mut_ptr().cast(), 1) };
    if n < 0 {
        return Err(io::Error::last_os_error());
    }

    if n == 0 {
        return Ok(None);
    }

    Ok(Some(b[0]))
}

fn utf8_char_len(b: u8) -> usize {
    if b < 0x80 {
        1
    } else if (b & 0xe0) == 0xc0 {
        2
    } else if (b & 0xf0) == 0xe0 {
        3
    } else if (b & 0xf8) == 0xf0 {
        4
    } else {
        1
    }
}

fn parse_escape(stdin_fd: std::os::unix::io::RawFd) -> io::Result<Option<Key>> {
    let mut seq = Vec::<u8>::new();
    let mut scratch = [0u8; 64];
    // First byte after ESC can arrive much later in some terminals/tmux setups
    let mut first_wait = true;
    for _ in 0..6 {
        let timeout_ms = if first_wait { 700 } else { 80 };
        let n = read_timeout(stdin_fd, &mut scratch, timeout_ms)?;
        first_wait = false;
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

    // Some terminals can prepend an extra ESC byte
    while seq.first() == Some(&0x1b) {
        seq.remove(0);
    }

    if seq.is_empty() {
        return Ok(Some(Key::Esc));
    }

    if seq[0] == b'O' && seq.len() >= 2 {
        return Ok(Some(match seq[1] {
            b'A' => Key::ArrowUp,
            b'B' => Key::ArrowDown,
            b'C' => Key::ArrowRight,
            b'D' => Key::ArrowLeft,
            b'H' => Key::Home,
            b'F' => Key::End,
            _ => return Ok(None),
        }));
    } else if seq[0] == b'O' {
        // Incomplete SS3 sequence: ignore this key
        return Ok(None);
    }

    if seq[0] != b'[' {
        return Ok(None);
    }

    let body = &seq[1..];
    if body.is_empty() {
        // Incomplete CSI sequence (often from arrows split across reads): ignore
        return Ok(None);
    }

    if body[0] == b'Z' {
        return Ok(Some(Key::ShiftTab));
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

    // Handle CSI sequences like ESC [ 1 ; 5 A
    if let Some(last) = body.last().copied() {
        match last {
            b'A' => return Ok(Some(Key::ArrowUp)),
            b'B' => return Ok(Some(Key::ArrowDown)),
            b'C' => return Ok(Some(Key::ArrowRight)),
            b'D' => return Ok(Some(Key::ArrowLeft)),
            b'H' => return Ok(Some(Key::Home)),
            b'F' => return Ok(Some(Key::End)),
            _ => {}
        }
    }

    if body.len() >= 2 && *body.last().unwrap_or(&0) == b'~' {
        let lead = body[0];
        match lead {
            b'1' | b'7' => return Ok(Some(Key::Home)),
            b'2' => return Ok(None),
            b'3' => return Ok(Some(Key::Delete)),
            b'4' | b'8' => return Ok(Some(Key::End)),
            b'5' => return Ok(Some(Key::PageUp)),
            b'6' => return Ok(Some(Key::PageDown)),
            _ => {}
        }
    }
    Ok(None)
}
