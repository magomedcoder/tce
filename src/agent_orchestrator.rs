use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::llm_api::TceLlmClient;
use crate::agent_tools::{AgentToolExecutor, ToolCall};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStepObservation {
    pub call_id: String,
    pub tool: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStepRequest {
    pub session_id: String,
    pub goal: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub observations: Vec<AgentStepObservation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCall {
    pub tool: String,
    pub id: String,
    pub args: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStepResponse {
    pub finish: bool,
    pub summary: String,
    #[serde(default)]
    pub calls: Vec<AgentCall>,
}

#[derive(Debug, Clone)]
pub struct AgentRunResult {
    pub finished: bool,
    pub steps: usize,
    pub final_summary: String,
    pub last_observations: Vec<AgentStepObservation>,
    pub events: Vec<String>,
}

pub trait AgentStepClient {
    fn step(&self, req: &AgentStepRequest) -> Result<AgentStepResponse, String>;
}

impl AgentStepClient for TceLlmClient {
    fn step(&self, req: &AgentStepRequest) -> Result<AgentStepResponse, String> {
        self.send_agent_step(req).map_err(|e| e.user_message())
    }
}

pub struct AgentOrchestrator<'a> {
    step_client: &'a dyn AgentStepClient,
    tools: &'a AgentToolExecutor,
    max_steps: usize,
}

impl<'a> AgentOrchestrator<'a> {
    pub fn new(
        step_client: &'a dyn AgentStepClient,
        tools: &'a AgentToolExecutor,
        max_steps: usize,
    ) -> Self {
        Self {
            step_client,
            tools,
            max_steps: max_steps.clamp(1, 128),
        }
    }

    pub fn run(&self, session_id: &str, goal: &str) -> Result<AgentRunResult, String> {
        if session_id.trim().is_empty() {
            return Err("session_id не должен быть пустым".to_string());
        }

        if goal.trim().is_empty() {
            return Err("goal не должен быть пустым".to_string());
        }

        let mut req = AgentStepRequest {
            session_id: session_id.to_string(),
            goal: goal.to_string(),
            observations: Vec::new(),
        };
        let mut last_summary = String::new();
        let mut last_observations = Vec::new();
        let mut events = Vec::<String>::new();

        for step_idx in 0..self.max_steps {
            // Модель возвращает план следующего шага (calls или finish=true)
            let step = self.step_client.step(&req)?;
            last_summary = step.summary.clone();
            events.push(format!("step {} summary: {}", step_idx + 1, step.summary));

            if step.finish {
                return Ok(AgentRunResult {
                    finished: true,
                    steps: step_idx + 1,
                    final_summary: last_summary,
                    last_observations,
                    events,
                });
            }

            let mut observations = Vec::new();
            for call in step.calls {
                // Лёгкий preview для потенциально рискованных изменений
                if call.tool == "apply_patch" {
                    events.push(format!(
                        "step {} diff-preview: {}",
                        step_idx + 1,
                        summarize_patch_preview(&call.args)
                    ));
                }

                let tool_result = self.tools.execute_call(&ToolCall {
                    tool: call.tool.clone(),
                    id: call.id.clone(),
                    args: call.args.clone(),
                });

                // Результат выполнения инструмента становится observation для следующего шага
                observations.push(AgentStepObservation {
                    call_id: call.id,
                    tool: call.tool,
                    ok: tool_result.ok,
                    result: tool_result.result,
                    error: tool_result.error,
                });

                let last = observations.last().expect("observation just pushed");
                events.push(format!(
                    "step {} call {} ({}) ok={}",
                    step_idx + 1,
                    last.call_id,
                    last.tool,
                    last.ok
                ));
            }
            last_observations = observations.clone();
            req.observations = observations;
        }

        Ok(AgentRunResult {
            finished: false,
            steps: self.max_steps,
            final_summary: format!(
                "Достигнут лимит шагов ({}) без finish=true. Последний summary: {}",
                self.max_steps, last_summary
            ),
            last_observations,
            events,
        })
    }
}

fn summarize_patch_preview(args: &Value) -> String {
    let Some(patch) = args.get("patch").and_then(Value::as_str) else {
        return "patch отсутствует".to_string();
    };

    let first_meaningful = patch
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("");

    let preview = if first_meaningful.chars().count() > 90 {
        format!("{}...", first_meaningful.chars().take(90).collect::<String>())
    } else {
        first_meaningful.to_string()
    };

    if preview.is_empty() {
        "пустой patch".to_string()
    } else {
        preview
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::fs;
    use std::path::PathBuf;
    use std::rc::Rc;
    use std::time::{SystemTime, UNIX_EPOCH};

    use serde_json::json;

    use crate::agent_sandbox::AgentSandbox;

    use super::*;

    struct FakeStepClient {
        steps: RefCell<Vec<AgentStepResponse>>,
        seen_requests: Rc<RefCell<Vec<AgentStepRequest>>>,
    }

    impl FakeStepClient {
        fn new(steps: Vec<AgentStepResponse>, seen_requests: Rc<RefCell<Vec<AgentStepRequest>>>) -> Self {
            Self {
                steps: RefCell::new(steps),
                seen_requests,
            }
        }
    }

    impl AgentStepClient for FakeStepClient {
        fn step(&self, req: &AgentStepRequest) -> Result<AgentStepResponse, String> {
            self.seen_requests.borrow_mut().push(req.clone());
            let mut steps = self.steps.borrow_mut();
            if steps.is_empty() {
                return Err("нет подготовленного ответа шага".to_string());
            }

            Ok(steps.remove(0))
        }
    }

    fn mk_temp_dir() -> PathBuf {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();

        let path = std::env::temp_dir().join(format!("tce-agent-orchestrator-{id}"));
        fs::create_dir_all(&path).expect("должен создаться временный каталог");
        path
    }

    #[test]
    fn run_finishes_after_second_step() {
        let root = mk_temp_dir();
        fs::write(root.join("sample.txt"), "hello").expect("должен записаться sample.txt");
        let sandbox = AgentSandbox::new(root.clone(), 1024).expect("должна создаться песочница");
        let tools = AgentToolExecutor::new(sandbox, false);
        let seen = Rc::new(RefCell::new(Vec::<AgentStepRequest>::new()));

        let client = FakeStepClient::new(vec![
            AgentStepResponse {
                finish: false,
                summary: "читаю файл".to_string(),
                calls: vec![AgentCall {
                    tool: "read_file".to_string(),
                    id: "call-1".to_string(),
                    args: json!({ "path": "sample.txt" }),
                }],
            },
            AgentStepResponse {
                finish: true,
                summary: "готово".to_string(),
                calls: vec![],
            },
        ], Rc::clone(&seen));

        let orchestrator = AgentOrchestrator::new(&client, &tools, 5);
        let result = orchestrator
            .run("session-1", "прочитать sample.txt")
            .expect("цикл должен завершиться успешно");

        assert!(result.finished, "должен вернуться finished=true");
        assert_eq!(result.steps, 2, "должно быть два шага");
        assert_eq!(result.final_summary, "готово");
        assert!(
            !result.last_observations.is_empty(),
            "должны быть observations после tool-вызовов"
        );

        let seen_requests = seen.borrow();
        assert_eq!(seen_requests.len(), 2, "должно быть два запроса к step-клиенту");
        assert!(
            seen_requests[1].observations.iter().any(|o| {
                o.call_id == "call-1" && o.tool == "read_file" && o.ok
            }),
            "на второй шаг должны передаваться observations из первого шага"
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn run_stops_on_max_steps() {
        let root = mk_temp_dir();
        let sandbox = AgentSandbox::new(root.clone(), 1024).expect("должна создаться песочница");
        let tools = AgentToolExecutor::new(sandbox, false);
        let seen = Rc::new(RefCell::new(Vec::<AgentStepRequest>::new()));

        let client = FakeStepClient::new(vec![AgentStepResponse {
            finish: false,
            summary: "ещё работаю".to_string(),
            calls: vec![],
        }], Rc::clone(&seen));

        let orchestrator = AgentOrchestrator::new(&client, &tools, 1);
        let result = orchestrator
            .run("session-1", "тест лимита")
            .expect("результат должен вернуться даже без finish=true");

        assert!(!result.finished, "должен сработать лимит шагов");
        assert_eq!(result.steps, 1);
        assert!(result.final_summary.contains("Достигнут лимит шагов"), "в summary должен быть текст про лимит шагов");
        assert_eq!(seen.borrow().len(),1,"при max_steps=1 должен быть ровно один запрос к step-клиенту");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn run_records_diff_preview_event_for_apply_patch() {
        let root = mk_temp_dir();
        let sandbox = AgentSandbox::new(root.clone(), 1024).expect("должна создаться песочница");
        let tools = AgentToolExecutor::new(sandbox, false);
        let seen = Rc::new(RefCell::new(Vec::<AgentStepRequest>::new()));

        let client = FakeStepClient::new(
            vec![
                AgentStepResponse {
                    finish: false,
                    summary: "готовлю patch".to_string(),
                    calls: vec![AgentCall {
                        tool: "apply_patch".to_string(),
                        id: "call-42".to_string(),
                        args: json!({"patch":"*** Begin Patch\n*** Update File: src/lib.rs\n+new"}),
                    }],
                },
                AgentStepResponse {
                    finish: true,
                    summary: "готово".to_string(),
                    calls: vec![],
                },
            ],
            Rc::clone(&seen),
        );
        
        let orchestrator = AgentOrchestrator::new(&client, &tools, 4);
        let result = orchestrator
            .run("session-1", "сделай patch")
            .expect("цикл должен завершиться успешно");
        assert!(result.events.iter().any(|e| e.contains("diff-preview")), "должно появиться событие diff-preview для apply_patch");
        let _ = fs::remove_dir_all(root);
    }
}
