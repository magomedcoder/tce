use std::path::{Path, PathBuf};
use std::process::Command;

use crate::plugins::code_quality::lint::LanguageLintAdapter;
use crate::plugins::code_quality::{DiagnosticItem, DiagnosticSeverity};

pub struct RustLintAdapter;

impl LanguageLintAdapter for RustLintAdapter {
    fn supports(&self, path: &Path) -> bool {
        path.extension().and_then(|e| e.to_str()) == Some("rs")
    }

    fn run(&self, path: &Path) -> Result<Vec<DiagnosticItem>, String> {
        let output = Command::new("rustc")
            .arg("--error-format=short")
            .arg("--emit=metadata")
            .arg(path)
            .output()
            .map_err(|e| e.to_string())?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        let mut items = Vec::<DiagnosticItem>::new();

        for line in stderr.lines() {
            let mut parts = line.splitn(4, ':');
            let p0 = parts.next().unwrap_or_default().trim();
            let p1 = parts.next().unwrap_or_default().trim();
            let p2 = parts.next().unwrap_or_default().trim();
            let p3 = parts.next().unwrap_or_default().trim();
            if p0.is_empty() || p1.is_empty() || p2.is_empty() || p3.is_empty() {
                continue;
            }

            let line_num = p1.parse::<usize>().ok().unwrap_or(1).saturating_sub(1);
            let col_num = p2.parse::<usize>().ok().unwrap_or(1).saturating_sub(1);
            let diag_path = PathBuf::from(p0);
            if !diag_path.exists() {
                continue;
            }

            let severity = if p3.to_lowercase().contains("warning") {
                DiagnosticSeverity::Warning
            } else {
                DiagnosticSeverity::Error
            };

            items.push(DiagnosticItem {
                path: diag_path,
                row: line_num,
                col: col_num,
                message: p3.to_string(),
                severity,
            });
        }

        Ok(items)
    }
}
