mod en;
mod ru;

use crate::localization::Language;

pub struct LlmTexts {
    pub ask: &'static str,
    pub show_history: &'static str,
    pub show_agent_events: &'static str,
    pub toggle_unsafe_tools: &'static str,
    pub clear_history: &'static str,
    pub insert_last_answer: &'static str,
    pub health_check: &'static str,
    pub explain_current_line: &'static str,
    pub run_agent_loop: &'static str,
}

pub fn texts(lang: Language) -> &'static LlmTexts {
    match lang {
        Language::Ru => &ru::RU,
        Language::En => &en::EN,
    }
}
