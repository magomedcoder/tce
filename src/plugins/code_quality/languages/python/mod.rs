use std::path::Path;

use crate::plugins::code_quality::lint::LanguageLintAdapter;
use crate::plugins::code_quality::DiagnosticItem;

pub struct PythonLintAdapter;

impl LanguageLintAdapter for PythonLintAdapter {
    fn supports(&self, path: &Path) -> bool {
        matches!(path.extension().and_then(|e| e.to_str()), Some("py") | Some("pyi"))
    }

    fn run(&self, _path: &Path) -> Result<Vec<DiagnosticItem>, String> {
        Ok(Vec::new())
    }
}
