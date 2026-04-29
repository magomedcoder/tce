use super::LlmTexts;

pub const RU: LlmTexts = LlmTexts {
    ask: "LLM: задать вопрос",
    show_history: "LLM: история",
    show_agent_events: "Agent: события",
    toggle_unsafe_tools: "Agent: unsafe tools (toggle)",
    clear_history: "LLM: очистить историю",
    insert_last_answer: "LLM: вставить последний ответ",
    health_check: "LLM: health check",
    explain_current_line: "LLM: объяснить текущую строку",
    run_agent_loop: "Agent: запустить loop",
};
