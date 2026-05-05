use std::path::Path;

use crate::plugins::code_quality::lint::LanguageLintAdapter;
use crate::plugins::code_quality::DiagnosticItem;

pub struct GoLintAdapter;

impl LanguageLintAdapter for GoLintAdapter {
    fn supports(&self, path: &Path) -> bool {
        path.extension().and_then(|e| e.to_str()) == Some("go")
    }

    fn run(&self, _path: &Path) -> Result<Vec<DiagnosticItem>, String> {
        Ok(Vec::new())
    }
}
