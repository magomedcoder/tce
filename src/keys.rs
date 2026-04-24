use std::io::{self, Read};

use crate::terminal::read_timeout;

#[derive(Debug)]
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
    CtrlQ,
    CtrlB,
    CtrlN,
    Esc,
}

pub fn read_key(stdin_fd: std::os::unix::io::RawFd) -> io::Result<Option<Key>> {
    let mut b0 = [0u8; 1];
    let n = std::io::stdin().read(&mut b0)?;
    if n == 0 {
        return Ok(None);
    }
    let byte = b0[0];

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

        if byte == 2 {
            return Ok(Some(Key::CtrlB));
        }

        if byte == 14 {
            return Ok(Some(Key::CtrlN));
        }

        return Ok(None);
    }

    let needed = utf8_char_len(byte);
    if needed == 1 {
        return Ok(Some(Key::Char(char::from_u32(byte as u32).unwrap_or('\u{fffd}'))));
    }

    let mut buf = [0u8; 4];
    buf[0] = byte;
    if let Err(e) = std::io::stdin().read_exact(&mut buf[1..needed]) {
        if e.kind() == io::ErrorKind::UnexpectedEof {
            return Ok(Some(Key::Char('\u{fffd}')));
        }
        return Err(e);
    }

    let ch = std::str::from_utf8(&buf[..needed])
        .ok()
        .and_then(|s| s.chars().next())
        .unwrap_or('\u{fffd}');
    Ok(Some(Key::Char(ch)))
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
