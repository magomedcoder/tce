//! Unix TTY: raw mode, window size, restore on drop.
#![cfg(unix)]

use std::io;
use std::mem::MaybeUninit;
use std::os::unix::io::AsRawFd;

/// Rows and columns of the terminal window (character cells).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TermSize {
    pub rows: u16,
    pub cols: u16,
}

pub fn winsize_tty() -> io::Result<TermSize> {
    winsize_fd(std::io::stdin().as_raw_fd())
}

fn winsize_fd(fd: std::os::unix::io::RawFd) -> io::Result<TermSize> {
    unsafe {
        let mut ws = MaybeUninit::<libc::winsize>::uninit();
        if libc::ioctl(fd, libc::TIOCGWINSZ, ws.as_mut_ptr()) != 0 {
            return Err(io::Error::last_os_error());
        }
        let ws = ws.assume_init();
        Ok(TermSize {
            rows: ws.ws_row,
            cols: ws.ws_col,
        })
    }
}

/// Restores cooked mode when dropped (including on panic unwind).
pub struct RawMode {
    fd: std::os::unix::io::RawFd,
    saved: libc::termios,
}

impl RawMode {
    pub fn enable_stdin() -> io::Result<Self> {
        let fd = std::io::stdin().as_raw_fd();
        unsafe {
            let mut saved = MaybeUninit::<libc::termios>::uninit();
            if libc::tcgetattr(fd, saved.as_mut_ptr()) != 0 {
                return Err(io::Error::last_os_error());
            }

            let saved = saved.assume_init();
            let mut raw = saved;
            raw.c_iflag &= !(libc::BRKINT
                | libc::ICRNL
                | libc::INPCK
                | libc::ISTRIP
                | libc::IXON);
            raw.c_oflag &= !libc::OPOST;
            raw.c_cflag |= libc::CS8;
            raw.c_lflag &= !(libc::ECHO | libc::ICANON | libc::IEXTEN | libc::ISIG);
            raw.c_cc[libc::VMIN] = 1;
            raw.c_cc[libc::VTIME] = 0;
            if libc::tcsetattr(fd, libc::TCSAFLUSH, &raw) != 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(Self { fd, saved })
        }
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        unsafe {
            let _ = libc::tcsetattr(self.fd, libc::TCSAFLUSH, &self.saved);
        }
    }
}

/// Read up to `buf.len()` bytes within `timeout_ms`, non-blocking after poll.
pub fn read_timeout(fd: std::os::unix::io::RawFd, buf: &mut [u8], timeout_ms: i32) -> io::Result<usize> {
    unsafe {
        let mut pfd = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        
        let pr = libc::poll(&mut pfd, 1, timeout_ms);
        if pr < 0 {
            return Err(io::Error::last_os_error());
        }

        if pr == 0 {
            return Ok(0);
        }

        let n = libc::read(fd, buf.as_mut_ptr().cast(), buf.len());
        if n < 0 {
            return Err(io::Error::last_os_error());
        }

        Ok(n as usize)
    }
}
