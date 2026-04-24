//! Flat file tree under a project root (depth-first, dirs first)

use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const MAX_ENTRIES: usize = 4000;

static SKIP_DIR_NAMES: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    ".cargo",
    "__pycache__",
    ".idea",
    ".vscode",
];

#[derive(Clone, Debug)]
pub struct TreeEntry {
    pub path: PathBuf,
    pub depth: usize,
    pub is_dir: bool,
    pub label: String,
}

pub fn build_tree(root: &Path) -> io::Result<Vec<TreeEntry>> {
    let mut out = Vec::new();
    if !root.is_dir() {
        return Ok(out);
    }

    walk(root, 0, &mut out)?;
    Ok(out)
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<TreeEntry>) -> io::Result<()> {
    if out.len() >= MAX_ENTRIES {
        return Ok(());
    }
    
    let mut entries: Vec<_> = fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .collect();
    entries.sort_by(|a, b| {
        let ad = a.path().is_dir();
        let bd = b.path().is_dir();
        ad.cmp(&bd).reverse().then_with(|| a.file_name().cmp(&b.file_name()))
    });

    for e in entries {
        if out.len() >= MAX_ENTRIES {
            break;
        }

        let name = e.file_name();
        if should_skip_dir(&name) {
            continue;
        }
        
        let p = e.path();
        let is_dir = p.is_dir();
        let label = name.to_string_lossy().into_owned();
        out.push(TreeEntry {
            path: p.clone(),
            depth,
            is_dir,
            label,
        });
        if is_dir {
            walk(&p, depth + 1, out)?;
        }
    }
    Ok(())
}

fn should_skip_dir(name: &OsStr) -> bool {
    SKIP_DIR_NAMES.iter().any(|s| Some(OsStr::new(s)) == Some(name))
}
