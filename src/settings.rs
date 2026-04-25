use std::fs;
use std::io;
use std::path::PathBuf;

use crate::localization::Language;

const SETTINGS_FILE: &str = "settings.conf";

#[derive(Clone, Copy, Debug)]
pub struct AppSettings {
    pub dark_theme: bool,
    pub autosave_on_edit: bool,
    pub font_zoom: i8,
    pub line_spacing: bool,
    pub ligatures: bool,
    pub language: Language,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            dark_theme: true,
            autosave_on_edit: true,
            font_zoom: 0,
            line_spacing: false,
            ligatures: false,
            language: Language::En,
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
            "dark_theme" => s.dark_theme = val == "true",
            "autosave_on_edit" => s.autosave_on_edit = val == "true",
            "font_zoom" => {
                if let Ok(n) = val.parse::<i8>() {
                    s.font_zoom = n.clamp(-2, 4);
                }
            }
            "line_spacing" => s.line_spacing = val == "true",
            "ligatures" => s.ligatures = val == "true",
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
    
    let body = format!(
        "dark_theme={}\nautosave_on_edit={}\nfont_zoom={}\nline_spacing={}\nligatures={}\nlanguage={}\n",
        s.dark_theme, s.autosave_on_edit, s.font_zoom, s.line_spacing, s.ligatures, language
    );

    fs::write(path, body)?;
    Ok(())
}
