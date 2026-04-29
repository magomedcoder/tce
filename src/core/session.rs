use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io;
use std::path::{Path, PathBuf};

const SESSION_DIR: &str = "sessions";

#[derive(Debug, Default)]
pub struct ProjectSession {
    pub tabs: Vec<PathBuf>,
    pub active: Option<PathBuf>,
    pub pinned: Vec<PathBuf>,
}

fn config_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config").join(".tce"))
}

fn sessions_dir() -> Option<PathBuf> {
    config_dir().map(|d| d.join(SESSION_DIR))
}

fn project_session_path(root: &Path) -> Option<PathBuf> {
    let root_key = root.to_string_lossy();
    let mut hasher = DefaultHasher::new();
    root_key.hash(&mut hasher);
    let key = format!("{:016x}", hasher.finish());
    sessions_dir().map(|d| d.join(format!("{key}.session")))
}

pub fn load_project_session(root: &Path) -> ProjectSession {
    let Some(path) = project_session_path(root) else {
        return ProjectSession::default();
    };
    
    let Ok(body) = fs::read_to_string(path) else {
        return ProjectSession::default();
    };

    let mut out = ProjectSession::default();
    for line in body.lines() {
        let item = line.trim();
        if item.is_empty() {
            continue;
        }

        if let Some(active_path) = item.strip_prefix("active\t") {
            let p = PathBuf::from(active_path);
            if p.is_file() {
                out.active = Some(p);
            }
            continue;
        }
        if let Some(pinned_path) = item.strip_prefix("pinned\t") {
            let p = PathBuf::from(pinned_path);
            if p.is_file() {
                out.pinned.push(p);
            }
            continue;
        }
        let p = PathBuf::from(item);
        if p.is_file() {
            out.tabs.push(p);
        }
    }

    dedupe_keep_order(&mut out.tabs);
    dedupe_keep_order(&mut out.pinned);
    if let Some(active) = &out.active {
        if !out.tabs.iter().any(|p| p == active) {
            out.active = None;
        }
    }
    out.pinned.retain(|p| out.tabs.iter().any(|tab| tab == p));
    out
}

pub fn save_project_session(
    root: &Path,
    tabs: &[PathBuf],
    active: Option<&PathBuf>,
    pinned: &[PathBuf],
) -> io::Result<()> {
    let Some(path) = project_session_path(root) else {
        return Ok(());
    };
    let Some(dir) = sessions_dir() else {
        return Ok(());
    };
    
    fs::create_dir_all(dir)?;

    let mut unique_tabs: Vec<PathBuf> = tabs
        .iter()
        .filter(|p| p.is_file())
        .cloned()
        .collect();
    dedupe_keep_order(&mut unique_tabs);
    let mut unique_pinned: Vec<PathBuf> = pinned
        .iter()
        .filter(|p| p.is_file())
        .filter(|p| unique_tabs.iter().any(|tab| tab == *p))
        .cloned()
        .collect();
    dedupe_keep_order(&mut unique_pinned);

    let mut lines = Vec::<String>::new();
    if let Some(active_path) = active {
        if unique_tabs.iter().any(|p| p == active_path) {
            lines.push(format!("active\t{}", active_path.to_string_lossy()));
        }
    }

    for p in unique_pinned {
        lines.push(format!("pinned\t{}", p.to_string_lossy()));
    }

    for tab in unique_tabs {
        lines.push(tab.to_string_lossy().to_string());
    }

    if lines.is_empty() {
        fs::write(path, "")?;
    } else {
        fs::write(path, format!("{}\n", lines.join("\n")))?;
    }
    Ok(())
}

fn dedupe_keep_order(paths: &mut Vec<PathBuf>) {
    let mut seen = std::collections::HashSet::<String>::new();
    paths.retain(|p| seen.insert(p.to_string_lossy().to_string()));
}
