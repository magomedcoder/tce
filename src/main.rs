#[cfg(unix)]
mod app;
#[cfg(unix)]
mod core;
#[cfg(unix)]
mod localization;
#[cfg(unix)]
mod plugins;
#[cfg(unix)]
mod workspace;

#[cfg(not(unix))]
fn main() {
    eprintln!("Tce - Terminal code editor");
    std::process::exit(1);
}

#[cfg(unix)]
fn main() {
    let path = std::env::args_os().nth(1).map(std::path::PathBuf::from);
    match app::App::from_args(path) {
        Ok(mut app) => {
            if let Err(e) = app.run() {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}
