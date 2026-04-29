use std::fs;
use std::io;
use std::path::PathBuf;

use crate::localization::Language;

const SETTINGS_FILE: &str = "settings.conf";

#[derive(Clone, Debug)]
pub struct AppSettings {
    pub sidebar_visible: bool,
    pub right_panel_visible: bool,
    pub dark_theme: bool,
    pub autosave_on_edit: bool,
    pub font_zoom: i8,
    pub line_spacing: bool,
    pub ligatures: bool,
    pub language: Language,
    pub llm_enabled: bool,
    pub llm_base_url: String,
    pub llm_timeout_ms: u64,
    pub llm_system_prompt: String,
    pub llm_generate_max_tokens: u32,
    pub llm_generate_temperature: f32,
    pub llm_attach_editor: bool,
    pub llm_snippet_lines: usize,
    pub llm_snippet_max_bytes: usize,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            sidebar_visible: true,
            right_panel_visible: true,
            dark_theme: true,
            autosave_on_edit: true,
            font_zoom: 0,
            line_spacing: false,
            ligatures: false,
            language: Language::En,
            llm_enabled: false,
            llm_base_url: "http://127.0.0.1:8000".to_string(),
            llm_timeout_ms: 30_000,
            llm_system_prompt: "Ты помощник по коду.".to_string(),
            llm_generate_max_tokens: 1024,
            llm_generate_temperature: 0.2,
            llm_attach_editor: true,
            llm_snippet_lines: 120,
            llm_snippet_max_bytes: 12_288,
        }
    }
}

fn config_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config").join(".tce"))
}

fn settings_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join(SETTINGS_FILE))
}

pub fn load_settings() -> AppSettings {
    let Some(path) = settings_path() else {
        return AppSettings::default();
    };

    let Ok(content) = fs::read_to_string(path) else {
        return AppSettings::default();
    };

    let mut s = AppSettings::default();
    for line in content.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }

        let Some((k, v)) = t.split_once('=') else {
            continue;
        };
        let key = k.trim();
        let val = v.trim();
        match key {
            "sidebar_visible" => s.sidebar_visible = val == "true",
            "right_panel_visible" => s.right_panel_visible = val == "true",
            "dark_theme" => s.dark_theme = val == "true",
            "autosave_on_edit" => s.autosave_on_edit = val == "true",
            "font_zoom" => {
                if let Ok(n) = val.parse::<i8>() {
                    s.font_zoom = n.clamp(-2, 4);
                }
            }
            "line_spacing" => s.line_spacing = val == "true",
            "ligatures" => s.ligatures = val == "true",
            "llm_enabled" => s.llm_enabled = val == "true",
            "llm_base_url" => s.llm_base_url = val.to_string(),
            "llm_timeout_ms" => {
                if let Ok(n) = val.parse::<u64>() {
                    s.llm_timeout_ms = n.clamp(1_000, 300_000);
                }
            }
            "llm_system_prompt" => s.llm_system_prompt = val.replace("\\n", "\n"),
            "llm_generate_max_tokens" => {
                if let Ok(n) = val.parse::<u32>() {
                    s.llm_generate_max_tokens = n.clamp(32, 16_384);
                }
            }
            "llm_generate_temperature" => {
                if let Ok(n) = val.parse::<f32>() {
                    s.llm_generate_temperature = n.clamp(0.0, 2.0);
                }
            }
            "llm_attach_editor" => s.llm_attach_editor = val == "true",
            "llm_snippet_lines" => {
                if let Ok(n) = val.parse::<usize>() {
                    s.llm_snippet_lines = n.clamp(10, 400);
                }
            }
            "llm_snippet_max_bytes" => {
                if let Ok(n) = val.parse::<usize>() {
                    s.llm_snippet_max_bytes = n.clamp(512, 128 * 1024);
                }
            }
            "language" => {
                s.language = if val.eq_ignore_ascii_case("ru") {
                    Language::Ru
                } else {
                    Language::En
                };
            }
            _ => {}
        }
    }
    s
}

pub fn save_settings(s: &AppSettings) -> io::Result<()> {
    let Some(path) = settings_path() else {
        return Ok(());
    };

    let Some(dir) = config_dir() else {
        return Ok(());
    };

    fs::create_dir_all(dir)?;
    let language = match s.language {
        Language::Ru => "ru",
        Language::En => "en",
    };
    
    let escaped_prompt = s.llm_system_prompt.replace('\n', "\\n");
    let body = format!(
        "sidebar_visible={}\nright_panel_visible={}\ndark_theme={}\nautosave_on_edit={}\nfont_zoom={}\nline_spacing={}\nligatures={}\nlanguage={}\nllm_enabled={}\nllm_base_url={}\nllm_timeout_ms={}\nllm_system_prompt={}\nllm_generate_max_tokens={}\nllm_generate_temperature={}\nllm_attach_editor={}\nllm_snippet_lines={}\nllm_snippet_max_bytes={}\n",
        s.sidebar_visible,
        s.right_panel_visible,
        s.dark_theme,
        s.autosave_on_edit,
        s.font_zoom,
        s.line_spacing,
        s.ligatures,
        language,
        s.llm_enabled,
        s.llm_base_url,
        s.llm_timeout_ms,
        escaped_prompt,
        s.llm_generate_max_tokens,
        s.llm_generate_temperature,
        s.llm_attach_editor,
        s.llm_snippet_lines,
        s.llm_snippet_max_bytes
    );

    fs::write(path, body)?;
    Ok(())
}
