pub(crate) mod core_ui;
pub(crate) mod code_quality;
pub(crate) mod filesystem;
pub(crate) mod manifest;
mod editing;
mod git;
pub(crate) mod llm;
mod lsp;
mod navigation;
mod task_runner;

use crate::core::plugin::{PluginRegistry, WorkspacePlugin};

pub fn builtin_registry() -> PluginRegistry {
    let manifests = manifest::discover_manifests();
    let is_enabled = |id: &str| {
        manifests.iter().find(|m| m.builtin && m.id == id).is_none_or(|m| m.enabled && m.compatible)
    };

    let mut plugins: Vec<Box<dyn WorkspacePlugin>> = Vec::new();
    if is_enabled("core_ui") {
        plugins.push(Box::new(core_ui::CoreUiPlugin));
    }

    if is_enabled("filesystem") {
        plugins.push(Box::new(filesystem::FilesystemPlugin));
    }

    if is_enabled("editing") {
        plugins.push(Box::new(editing::EditingPlugin));
    }

    if is_enabled("code_quality") {
        plugins.push(Box::new(code_quality::CodeQualityPlugin));
    }

    if is_enabled("navigation") {
        plugins.push(Box::new(navigation::NavigationPlugin));
    }

    if is_enabled("task_runner") {
        plugins.push(Box::new(task_runner::TaskRunnerPlugin));
    }

    if is_enabled("git") {
        plugins.push(Box::new(git::GitPlugin));
    }

    if is_enabled("lsp") {
        plugins.push(Box::new(lsp::LspPlugin));
    }

    if is_enabled("llm") {
        plugins.push(Box::new(llm::LlmPlugin));
    }

    PluginRegistry::new(plugins)
}
