use std::path::Path;

use super::DiagnosticItem;
use super::languages::go::GoLintAdapter;
use super::languages::python::PythonLintAdapter;
use super::languages::rust::RustLintAdapter;
use super::languages::ts_js::TsJsLintAdapter;

pub trait LanguageLintAdapter {
    fn supports(&self, path: &Path) -> bool;
    fn run(&self, path: &Path) -> Result<Vec<DiagnosticItem>, String>;
}

pub struct LintEngine {
    adapters: Vec<Box<dyn LanguageLintAdapter>>,
}

impl LintEngine {
    pub fn new() -> Self {
        Self {
            adapters: vec![
                Box::new(RustLintAdapter),
                Box::new(GoLintAdapter),
                Box::new(PythonLintAdapter),
                Box::new(TsJsLintAdapter),
            ],
        }
    }

    pub fn run_for_path(&self, path: &Path) -> Result<Vec<DiagnosticItem>, String> {
        let Some(adapter) = self.adapters.iter().find(|a| a.supports(path)) else {
            return Ok(Vec::new());
        };
        adapter.run(path)
    }
}

#[cfg(test)]
mod tests {
    use super::LintEngine;
    use std::path::Path;

    #[test]
    fn unknown_extension_returns_empty() {
        let engine = LintEngine::new();
        let out = engine.run_for_path(Path::new("/tmp/file.unknown")).expect("engine should not fail");
        assert!(out.is_empty());
    }
}
