use std::fs;
use std::io::{self, Write};
use std::os::unix::io::AsRawFd;
use std::path::PathBuf;

use crate::core::keys::read_key;
use crate::core::terminal::RawMode;
use crate::plugins::core_ui::welcome::{Welcome, WelcomeAction};
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
                match fs::metadata(&p) {
                    Ok(meta) => {
                        if meta.is_dir() {
                            Phase::Workspace(Workspace::open_dir(p)?)
                        } else {
                            Phase::Workspace(Workspace::open_file_in_project(p)?)
                        }
                    }
                    Err(err) if err.kind() == io::ErrorKind::NotFound => {
                        // Разрешаем запуск с новым файлом:
                        // `tce path/to/new_file.rs` открывает пустой буфер с привязкой к пути.
                        Phase::Workspace(Workspace::open_file_in_project(p)?)
                    }
                    Err(err) => return Err(err),
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
                            WelcomeAction::OpenProject(root, language) => {
                                let mut ws = Workspace::open_project(root)?;
                                ws.set_language(language);
                                self.phase = Phase::Workspace(ws);
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
