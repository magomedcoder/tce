use std::fs;
use std::io::{self, Write};
use std::os::unix::io::AsRawFd;
use std::path::PathBuf;

use crate::keys::read_key;
use crate::terminal::RawMode;
use crate::welcome::{Welcome, WelcomeAction};
use crate::workspace::Workspace;

enum Phase {
    Welcome(Welcome),
    Workspace(Workspace),
}

pub struct App {
    phase: Phase,
}

impl App {
    pub fn from_args(arg: Option<PathBuf>) -> io::Result<Self> {
        let phase = match arg {
            None => Phase::Welcome(Welcome::new()),
            Some(p) => {
                let meta = fs::metadata(&p)?;
                if meta.is_dir() {
                    Phase::Workspace(Workspace::open_dir(p)?)
                } else {
                    Phase::Workspace(Workspace::open_file_in_project(p)?)
                }
            }
        };
        Ok(Self { phase })
    }

    pub fn run(&mut self) -> io::Result<()> {
        let _raw = RawMode::enable_stdin()?;
        let stdin_fd = std::io::stdin().as_raw_fd();

        write!(io::stdout(), "\x1b[?25l\x1b[?7l")?;
        io::stdout().flush()?;

        let result = (|| -> io::Result<()> {
            loop {
                match &mut self.phase {
                    Phase::Welcome(w) => {
                        w.render()?;
                        let key = match read_key(stdin_fd)? {
                            Some(k) => k,
                            None => continue,
                        };

                        match w.handle_key(key)? {
                            WelcomeAction::Quit => break,
                            WelcomeAction::OpenProject(root) => {
                                self.phase = Phase::Workspace(Workspace::open_project(root)?);
                            }
                            WelcomeAction::None => {}
                        }
                    }
                    Phase::Workspace(ws) => {
                        ws.render()?;
                        let key = match read_key(stdin_fd)? {
                            Some(k) => k,
                            None => continue,
                        };

                        if ws.handle_key(key)? {
                            break;
                        }
                    }
                }
            }
            Ok(())
        })();

        write!(io::stdout(), "\x1b[?25h\x1b[?7h\x1b[m\r\n")?;
        io::stdout().flush()?;

        result
    }
}
