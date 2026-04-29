use crate::workspace::Workspace;

use super::commands;

pub fn run(ws: &mut Workspace, cmd: &str) -> bool {
    match cmd {
        commands::ASK => ws.open_llm_prompt(),
        commands::HISTORY => ws.open_llm_history(),
        commands::AGENT_EVENTS => ws.open_agent_events(),
        commands::AGENT_TOGGLE_UNSAFE => ws.toggle_unsafe_agent_tools(),
        commands::HISTORY_CLEAR => ws.clear_llm_history(),
        commands::INSERT_LAST => ws.plugin_insert_last_llm_answer(),
        commands::HEALTH => ws.plugin_run_llm_health_check(),
        commands::EXPLAIN_LINE => ws.plugin_run_llm_explain_current_line(),
        commands::AGENT_RUN_LOOP => ws.plugin_run_agent_loop_mvp(),
        _ => return false,
    }
    true
}
