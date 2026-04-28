use std::time::Duration;
use std::io::{BufRead, BufReader};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::agent_orchestrator::{AgentStepRequest, AgentStepResponse};
use crate::settings::AppSettings;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiErrorBody {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub ok: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct EditorContext {
    pub path: String,
    pub language: String,
    pub snippet: String,
    pub cursor_line: usize,
    pub cursor_column: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct GenerateParams {
    pub max_tokens: u32,
    pub temperature: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatRequest {
    pub stream: bool,
    pub system: String,
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub editor: Option<EditorContext>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generate: Option<GenerateParams>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChatResponseMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChatResponse {
    pub message: ChatResponseMessage,
    pub finish: String,
}

#[derive(Debug, Clone, Deserialize)]
struct SseDeltaData {
    text: String,
}

#[derive(Debug, Clone, Deserialize)]
struct SseEndData {
    finish: String,
}

#[derive(Debug)]
pub enum LlmApiError {
    Http(String),
    Timeout(String),
    Protocol(String),
    Api(ApiErrorBody),
}

impl LlmApiError {
    pub fn user_message(&self) -> String {
        match self {
            LlmApiError::Http(m) => format!("Ошибка HTTP: {m}"),
            LlmApiError::Timeout(m) => format!("Таймаут запроса: {m}"),
            LlmApiError::Protocol(m) if m == "запрос отменён" => "Запрос к LLM отменён".to_string(),
            LlmApiError::Protocol(m) => format!("Ошибка протокола: {m}"),
            LlmApiError::Api(e) => format!("Ошибка API ({}): {}", e.code, e.message),
        }
    }
}

pub struct TceLlmClient {
    base_url: String,
    timeout_ms: u64,
}

impl TceLlmClient {
    pub fn from_settings(s: &AppSettings) -> Self {
        Self {
            base_url: s.llm_base_url.trim_end_matches('/').to_string(),
            timeout_ms: s.llm_timeout_ms,
        }
    }

    pub fn check_health(&self) -> Result<HealthResponse, LlmApiError> {
        // Быстрая проверка доступности адаптера/раннера
        let req = self
            .build_agent()
            .get(&format!("{}/tce/v1/health", self.base_url));

        let mut res = req
            .call()
            .map_err(map_ureq_error)?;
        let status = res.status();
        let body = res
            .body_mut()
            .read_to_string()
            .map_err(map_ureq_error)?;

        if !status.is_success() {
            return Err(parse_api_error_or_protocol(&body));
        }

        serde_json::from_str::<HealthResponse>(&body).map_err(|e| LlmApiError::Protocol(format!("ошибка разбора health-ответа: {e}")))
    }

    pub fn send_chat_streaming<F>(
        &self,
        req_body: &ChatRequest,
        cancel: Arc<AtomicBool>,
        mut on_delta: F,
    ) -> Result<ChatResponse, LlmApiError>
    where
        F: FnMut(&str),
    {
        // Один метод обслуживает и обычный JSON-ответ, и SSE-стрим
        let req = self
            .build_agent()
            .post(&format!("{}/tce/v1/chat", self.base_url))
            .header("Content-Type", "application/json");
        let payload = serde_json::to_string(req_body)
            .map_err(|e| LlmApiError::Protocol(format!("ошибка сериализации chat-запроса: {e}")))?;
        let mut res = req.send(payload).map_err(map_ureq_error)?;
        let status = res.status();
        if !status.is_success() {
            let body = res.body_mut().read_to_string().map_err(map_ureq_error)?;
            return Err(parse_api_error_or_protocol(&body));
        }

        if !req_body.stream {
            let body = res.body_mut().read_to_string().map_err(map_ureq_error)?;
            return serde_json::from_str::<ChatResponse>(&body)
                .map_err(|e| LlmApiError::Protocol(format!("ошибка разбора chat-ответа: {e}")));
        }

        // Для стрима накапливаем полный текст и параллельно отдаём дельты в ui
        let mut current_event = String::new();
        let mut full_text = String::new();
        let mut finish = String::from("stop");

        let reader = BufReader::new(res.body_mut().as_reader());
        for line_res in reader.lines() {
            if cancel.load(Ordering::Relaxed) {
                return Err(LlmApiError::Protocol("запрос отменён".to_string()));
            }

            let raw = line_res.map_err(|e| LlmApiError::Http(e.to_string()))?;
            let line = raw.trim();
            if line.is_empty() {
                continue;
            }

            if let Some(rest) = line.strip_prefix("event:") {
                current_event = rest.trim().to_string();
                continue;
            }

            if let Some(rest) = line.strip_prefix("data:") {
                let data = rest.trim();
                if current_event == "delta" {
                    let chunk = serde_json::from_str::<SseDeltaData>(data).map_err(|e| {
                        LlmApiError::Protocol(format!("ошибка разбора SSE delta data: {e}"))
                    })?;
                    full_text.push_str(&chunk.text);
                    on_delta(&chunk.text);
                } else if current_event == "end" {
                    let end = serde_json::from_str::<SseEndData>(data).map_err(|e| {
                        LlmApiError::Protocol(format!("ошибка разбора SSE end data: {e}"))
                    })?;
                    finish = end.finish;
                }
            }
        }

        Ok(ChatResponse {
            message: ChatResponseMessage {
                role: "assistant".to_string(),
                content: full_text,
            },
            finish,
        })
    }

    pub fn send_agent_step(
        &self,
        req_body: &AgentStepRequest,
    ) -> Result<AgentStepResponse, LlmApiError> {
        // Один шаг автономного агента: запрос -> список tool calls / finish
        let req = self
            .build_agent()
            .post(&format!("{}/tce/v1/agent/step", self.base_url))
            .header("Content-Type", "application/json");

        let payload = serde_json::to_string(req_body).map_err(|e| {
            LlmApiError::Protocol(format!("ошибка сериализации agent-step запроса: {e}"))
        })?;
        let mut res = req.send(payload).map_err(map_ureq_error)?;
        let status = res.status();
        let body = res.body_mut().read_to_string().map_err(map_ureq_error)?;
        if !status.is_success() {
            return Err(parse_api_error_or_protocol(&body));
        }

        parse_agent_step_response(&body)
    }

    fn build_agent(&self) -> ureq::Agent {
        let timeout = Duration::from_millis(self.timeout_ms.clamp(1_000, 300_000));
        let cfg = ureq::Agent::config_builder()
            .timeout_global(Some(timeout))
            .build();
        ureq::Agent::new_with_config(cfg)
    }
}

fn parse_api_error_or_protocol(body: &str) -> LlmApiError {
    #[derive(Deserialize)]
    struct ErrorEnvelope {
        error: ApiErrorBody,
    }

    match serde_json::from_str::<ErrorEnvelope>(body) {
        Ok(e) => LlmApiError::Api(e.error),
        Err(_) => LlmApiError::Protocol(format!("ошибка API без валидного JSON error: {body}")),
    }
}

fn map_ureq_error(e: ureq::Error) -> LlmApiError {
    let msg = e.to_string();
    let lower = msg.to_lowercase();
    if lower.contains("timed out") || lower.contains("timeout") || lower.contains("deadline") {
        return LlmApiError::Timeout(msg);
    }
    LlmApiError::Http(msg)
}

fn parse_sse_chat_response(body: &str) -> Result<ChatResponse, LlmApiError> {
    let mut current_event = String::new();
    let mut full_text = String::new();
    let mut finish = String::from("stop");

    for raw_line in body.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }

        if let Some(rest) = line.strip_prefix("event:") {
            current_event = rest.trim().to_string();
            continue;
        }

        if let Some(rest) = line.strip_prefix("data:") {
            let data = rest.trim();
            if current_event == "delta" {
                let chunk = serde_json::from_str::<SseDeltaData>(data).map_err(|e| {
                    LlmApiError::Protocol(format!("ошибка разбора SSE delta data: {e}"))
                })?;
                full_text.push_str(&chunk.text);
            } else if current_event == "end" {
                let end = serde_json::from_str::<SseEndData>(data).map_err(|e| {
                    LlmApiError::Protocol(format!("ошибка разбора SSE end data: {e}"))
                })?;
                finish = end.finish;
            }
        }
    }

    Ok(ChatResponse {
        message: ChatResponseMessage {
            role: "assistant".to_string(),
            content: full_text,
        },
        finish,
    })
}

fn parse_agent_step_response(body: &str) -> Result<AgentStepResponse, LlmApiError> {
    serde_json::from_str::<AgentStepResponse>(body)
        .map_err(|e| LlmApiError::Protocol(format!("ошибка разбора agent-step ответа: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_health_ok() {
        let data = r#"{"ok":true}"#;
        let parsed: HealthResponse = serde_json::from_str(data).expect("ожидался валидный health JSON");
        assert!(parsed.ok);
    }

    #[test]
    fn parse_api_error_json() {
        let body = r#"{"error":{"code":"bad_request","message":"поле stream обязательно"}}"#;
        let err = parse_api_error_or_protocol(body);
        match err {
            LlmApiError::Api(e) => {
                assert_eq!(e.code, "bad_request");
                assert_eq!(e.message, "поле stream обязательно");
            }
            _ => panic!("ожидался тип ошибки Api"),
        }
    }

    #[test]
    fn parse_sse_chat_ok() {
        let sse = r#"event: delta
data: {"text":"Привет, "}

event: delta
data: {"text":"мир!"}

event: end
data: {"finish":"stop"}
"#;
        let parsed = parse_sse_chat_response(sse).expect("ожидался корректный SSE");
        assert_eq!(parsed.message.role, "assistant");
        assert_eq!(parsed.message.content, "Привет, мир!");
        assert_eq!(parsed.finish, "stop");
    }

    #[test]
    fn parse_agent_step_ok() {
        let body = r#"{
  "finish": false,
  "summary": "Проверил файлы и подготовил следующие действия.",
  "calls": [
    { "tool": "read_file", "id": "call-1", "args": { "path": "src/main.rs" } }
  ]
}"#;
        let parsed = parse_agent_step_response(body).expect("ожидался корректный agent-step JSON");
        assert!(!parsed.finish);
        assert_eq!(parsed.summary, "Проверил файлы и подготовил следующие действия.");
        assert_eq!(parsed.calls.len(), 1);
        assert_eq!(parsed.calls[0].tool, "read_file");
    }

    #[test]
    fn parse_agent_step_invalid_json() {
        let body = r#"{"finish":false,"summary":123,"calls":[]}"#;
        let err = parse_agent_step_response(body).expect_err("должна вернуться ошибка протокола");
        match err {
            LlmApiError::Protocol(msg) => {
                assert!(msg.contains("agent-step"), "сообщение должно указывать на разбор agent-step ответа");
            }
            _ => panic!("ожидалась ошибка типа Protocol"),
        }
    }
}
