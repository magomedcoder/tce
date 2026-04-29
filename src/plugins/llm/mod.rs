mod commands;
mod handlers;
mod localization;
pub(crate) mod agent_orchestrator;
pub(crate) mod agent_sandbox;
pub(crate) mod agent_tools;
pub(crate) mod llm_api;

use crate::core::plugin::{PaletteCommand, WorkspacePlugin};
use crate::core::keys::Key;
use crate::workspace::Workspace;

pub struct LlmPlugin;

impl WorkspacePlugin for LlmPlugin {
    fn id(&self) -> &'static str {
        "llm"
    }

    fn palette_commands(&self, ws: &Workspace) -> Vec<PaletteCommand> {
        if !ws.is_llm_enabled_in_settings() {
            return Vec::new();
        }

        let tx = localization::texts(ws.current_language());
        vec![
            PaletteCommand::new(tx.ask.to_string(), commands::ASK),
            PaletteCommand::new(tx.show_history.to_string(), commands::HISTORY),
            PaletteCommand::new(tx.show_agent_events.to_string(), commands::AGENT_EVENTS),
            PaletteCommand::new(tx.toggle_unsafe_tools.to_string(), commands::AGENT_TOGGLE_UNSAFE),
            PaletteCommand::new(tx.clear_history.to_string(), commands::HISTORY_CLEAR),
            PaletteCommand::new(tx.insert_last_answer.to_string(), commands::INSERT_LAST),
            PaletteCommand::new(tx.health_check.to_string(), commands::HEALTH),
            PaletteCommand::new(tx.explain_current_line.to_string(), commands::EXPLAIN_LINE),
            PaletteCommand::new(tx.run_agent_loop.to_string(), commands::AGENT_RUN_LOOP),
        ]
    }

    fn run_command(&self, ws: &mut Workspace, cmd: &str) -> bool {
        handlers::run(ws, cmd)
    }

    fn handle_key(&self, ws: &mut Workspace, key: Key) -> bool {
        if ws.plugin_is_agent_unsafe_confirm_active() {
            ws.plugin_handle_agent_unsafe_confirm_key(key);
            return true;
        }
        if ws.plugin_is_llm_prompt_active() {
            ws.plugin_handle_llm_prompt_key(key);
            return true;
        }
        if ws.plugin_is_multi_edit_active() {
            ws.plugin_handle_multi_edit_key(key);
            return true;
        }
        if ws.plugin_is_sync_edit_active() {
            ws.plugin_handle_sync_edit_key(key);
            return true;
        }
        if ws.plugin_is_llm_history_view_active() {
            ws.plugin_handle_llm_history_view_key(key);
            return true;
        }
        if ws.plugin_is_agent_events_view_active() {
            ws.plugin_handle_agent_events_view_key(key);
            return true;
        }

        if !ws.is_llm_enabled_in_settings() {
            return false;
        }
        match key {
            Key::CtrlG => {
                ws.open_llm_prompt();
                ws.plugin_focus_editor();
            }
            _ => return false,
        }
        true
    }

    fn render_overlay(&self, ws: &Workspace, out: &mut String, cols: usize, rows: usize) -> bool {
        ws.plugin_render_assistant_overlays(out, cols, rows)
    }
}
