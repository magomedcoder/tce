use std::path::Path;

use crate::plugins::code_quality::lint::LanguageLintAdapter;
use crate::plugins::code_quality::DiagnosticItem;

pub struct TsJsLintAdapter;

impl LanguageLintAdapter for TsJsLintAdapter {
    fn supports(&self, path: &Path) -> bool {
        matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("ts") | Some("tsx") | Some("js") | Some("jsx") | Some("mjs") | Some("cjs")
        )
    }

    fn run(&self, _path: &Path) -> Result<Vec<DiagnosticItem>, String> {
        Ok(Vec::new())
    }
}
