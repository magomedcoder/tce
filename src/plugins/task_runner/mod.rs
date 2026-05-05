use crate::core::plugin::{PaletteCommand, WorkspacePlugin};
use crate::workspace::Workspace;

const TASK_BUILD: &str = "task_build";
const TASK_TEST: &str = "task_test";
const TASK_RUN: &str = "task_run";

pub struct TaskRunnerPlugin;

impl WorkspacePlugin for TaskRunnerPlugin {
    fn id(&self) -> &'static str {
        "task_runner"
    }

    fn palette_commands(&self, _ws: &Workspace) -> Vec<PaletteCommand> {
        vec![
            PaletteCommand::new("Task: Build".to_string(), TASK_BUILD),
            PaletteCommand::new("Task: Test".to_string(), TASK_TEST),
            PaletteCommand::new("Task: Run".to_string(), TASK_RUN),
        ]
    }

    fn run_command(&self, ws: &mut Workspace, cmd: &str) -> bool {
        match cmd {
            TASK_BUILD => ws.plugin_run_task_build(),
            TASK_TEST => ws.plugin_run_task_test(),
            TASK_RUN => ws.plugin_run_task_run(),
            _ => return false,
        }
        true
    }
}
