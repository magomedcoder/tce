use std::path::PathBuf;

use crate::core::keys::Key;
use crate::core::plugin::{PaletteCommand, WorkspacePlugin};
use crate::workspace::Workspace;

pub mod highlights;
pub mod lint;
pub mod languages;
pub mod syntax;

const RUN_LINT_CURRENT: &str = "code_quality_run_lint_current";
const SHOW_DIAGNOSTICS: &str = "code_quality_show_diagnostics";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiagnosticItem {
    pub path: PathBuf,
    pub row: usize,
    pub col: usize,
    pub message: String,
    pub severity: DiagnosticSeverity,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticsFilter {
    All,
    Errors,
    Warnings,
}

#[derive(Clone, Debug)]
pub struct DiagnosticsState {
    pub items: Vec<DiagnosticItem>,
    pub sel: usize,
    pub open: bool,
    pub filter: DiagnosticsFilter,
}

impl Default for DiagnosticsState {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            sel: 0,
            open: false,
            filter: DiagnosticsFilter::All,
        }
    }
}

pub fn infer_inlay_hint(line: &str) -> Option<String> {
    let t = line.trim_start();
    let rest = t.strip_prefix("let ")?;
    let eq_pos = rest.find('=')?;
    let lhs = rest[..eq_pos].trim_end();
    if lhs.contains(':') {
        return None;
    }

    let name = lhs
        .trim_start_matches("mut ")
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect::<String>();

    if name.is_empty() {
        return None;
    }

    Some(" : ?".to_string())
}

pub fn apply_quick_fix_to_line(line: &str, message: &str) -> Option<String> {
    if message.contains("unused_mut") || message.contains("unused mut") {
        if let Some(idx) = line.find("mut ") {
            let mut s = line.to_string();
            s.replace_range(idx..idx + 4, "");
            return Some(s);
        }
    }

    if message.contains("unused variable") {
        let start = message.find('`')?;
        let tail = &message[start + 1..];
        let end_rel = tail.find('`')?;
        let var = &tail[..end_rel];
        if var.is_empty() || var.starts_with('_') {
            return None;
        }

        let needle = format!("let {}", var);
        if let Some(pos) = line.find(&needle) {
            let mut s = line.to_string();
            s.replace_range(pos + 4..pos + 4 + var.len(), &format!("_{}", var));
            return Some(s);
        }

        let needle_mut = format!("let mut {}", var);
        if let Some(pos) = line.find(&needle_mut) {
            let mut s = line.to_string();
            let start_var = pos + "let mut ".len();
            s.replace_range(start_var..start_var + var.len(), &format!("_{}", var));
            return Some(s);
        }
    }
    None
}

pub struct CodeQualityPlugin;

impl WorkspacePlugin for CodeQualityPlugin {
    fn id(&self) -> &'static str {
        "code_quality"
    }

    fn palette_commands(&self, _ws: &Workspace) -> Vec<PaletteCommand> {
        vec![
            PaletteCommand::new("Code quality: run lint (current file)".to_string(), RUN_LINT_CURRENT),
            PaletteCommand::new("Code quality: show diagnostics".to_string(), SHOW_DIAGNOSTICS),
        ]
    }

    fn run_command(&self, ws: &mut Workspace, cmd: &str) -> bool {
        let mut ctx = ws.plugin_context();
        match cmd {
            RUN_LINT_CURRENT => ctx.run_lint_current_file(),
            SHOW_DIAGNOSTICS => ctx.show_diagnostics(),
            _ => return false,
        }
        true
    }

    fn handle_key(&self, ws: &mut Workspace, key: Key) -> bool {
        let mut ctx = ws.plugin_context();
        if !ctx.is_diagnostics_open() {
            return false;
        }
        ctx.handle_diagnostics_key(key);
        true
    }

    fn render_overlay(&self, ws: &Workspace, out: &mut String, cols: usize, rows: usize) -> bool {
        ws.plugin_render_diagnostics_overlay(out, cols, rows)
    }
}

#[cfg(test)]
mod tests {
    use super::{apply_quick_fix_to_line, infer_inlay_hint};

    #[test]
    fn quick_fix_removes_mut() {
        let fixed = apply_quick_fix_to_line("let mut x = 1;", "warning: unused_mut").expect("quick fix should apply");
        assert_eq!(fixed, "let x = 1;");
    }

    #[test]
    fn inlay_hint_for_untyped_let() {
        assert_eq!(infer_inlay_hint("let value = call();"), Some(" : ?".to_string()));
        assert_eq!(infer_inlay_hint("let value: i32 = 1;"), None);
    }
}
