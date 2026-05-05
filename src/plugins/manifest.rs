use std::fs;
use std::path::{Path, PathBuf};

use crate::core::settings;

pub const PLUGIN_API_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PluginPermission {
    FsRead,
    FsWrite,
    Network,
    Process,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub api_version: u32,
    pub builtin: bool,
    pub enabled: bool,
    pub compatible: bool,
    pub compatibility_error: Option<String>,
    pub permissions: Vec<PluginPermission>,
    pub entry: Option<PathBuf>,
}

pub fn discover_manifests() -> Vec<PluginManifest> {
    let settings = settings::load_settings();
    let mut out = builtin_manifests(&settings.disabled_plugins);
    out.extend(external_manifests(
        &settings.disabled_plugins,
        &settings.trusted_plugins,
    ));
    out
}

fn builtin_manifests(disabled: &[String]) -> Vec<PluginManifest> {
    let is_enabled = |id: &str| !disabled.iter().any(|d| d == id);
    vec![
        PluginManifest {
            id: "core_ui".to_string(),
            name: "Core UI".to_string(),
            version: "1.0.0".to_string(),
            api_version: PLUGIN_API_VERSION,
            builtin: true,
            enabled: is_enabled("core_ui"),
            compatible: true,
            compatibility_error: None,
            permissions: vec![PluginPermission::FsRead],
            entry: None,
        },
        PluginManifest {
            id: "filesystem".to_string(),
            name: "Filesystem".to_string(),
            version: "1.0.0".to_string(),
            api_version: PLUGIN_API_VERSION,
            builtin: true,
            enabled: is_enabled("filesystem"),
            compatible: true,
            compatibility_error: None,
            permissions: vec![PluginPermission::FsRead, PluginPermission::FsWrite],
            entry: None,
        },
        PluginManifest {
            id: "editing".to_string(),
            name: "Editing".to_string(),
            version: "1.0.0".to_string(),
            api_version: PLUGIN_API_VERSION,
            builtin: true,
            enabled: is_enabled("editing"),
            compatible: true,
            compatibility_error: None,
            permissions: vec![PluginPermission::FsRead],
            entry: None,
        },
        PluginManifest {
            id: "code_quality".to_string(),
            name: "Code Quality".to_string(),
            version: "1.0.0".to_string(),
            api_version: PLUGIN_API_VERSION,
            builtin: true,
            enabled: is_enabled("code_quality"),
            compatible: true,
            compatibility_error: None,
            permissions: vec![PluginPermission::FsRead, PluginPermission::Process],
            entry: None,
        },
        PluginManifest {
            id: "navigation".to_string(),
            name: "Navigation".to_string(),
            version: "1.0.0".to_string(),
            api_version: PLUGIN_API_VERSION,
            builtin: true,
            enabled: is_enabled("navigation"),
            compatible: true,
            compatibility_error: None,
            permissions: vec![PluginPermission::FsRead],
            entry: None,
        },
        PluginManifest {
            id: "task_runner".to_string(),
            name: "Task Runner".to_string(),
            version: "1.0.0".to_string(),
            api_version: PLUGIN_API_VERSION,
            builtin: true,
            enabled: is_enabled("task_runner"),
            compatible: true,
            compatibility_error: None,
            permissions: vec![PluginPermission::Process],
            entry: None,
        },
        PluginManifest {
            id: "git".to_string(),
            name: "Git".to_string(),
            version: "1.0.0".to_string(),
            api_version: PLUGIN_API_VERSION,
            builtin: true,
            enabled: is_enabled("git"),
            compatible: true,
            compatibility_error: None,
            permissions: vec![PluginPermission::Process],
            entry: None,
        },
        PluginManifest {
            id: "lsp".to_string(),
            name: "LSP".to_string(),
            version: "1.0.0".to_string(),
            api_version: PLUGIN_API_VERSION,
            builtin: true,
            enabled: is_enabled("lsp"),
            compatible: true,
            compatibility_error: None,
            permissions: vec![PluginPermission::FsRead],
            entry: None,
        },
        PluginManifest {
            id: "llm".to_string(),
            name: "LLM".to_string(),
            version: "1.0.0".to_string(),
            api_version: PLUGIN_API_VERSION,
            builtin: true,
            enabled: is_enabled("llm"),
            compatible: true,
            compatibility_error: None,
            permissions: vec![PluginPermission::FsRead, PluginPermission::Network],
            entry: None,
        },
    ]
}

fn external_manifests(disabled: &[String], trusted: &[String]) -> Vec<PluginManifest> {
    let Some(home) = std::env::var_os("HOME") else {
        return Vec::new();
    };
    let root = PathBuf::from(home).join(".config").join(".tce").join("plugins");
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("manifest") {
            continue;
        }
        
        if let Some(m) = parse_manifest_file(&path, disabled, trusted) {
            out.push(m);
        }
    }
    out
}

fn parse_manifest_file(path: &Path, disabled: &[String], trusted: &[String]) -> Option<PluginManifest> {
    let content = fs::read_to_string(path).ok()?;
    let mut id = String::new();
    let mut name = String::new();
    let mut version = "1.0.0".to_string();
    let mut api_version = PLUGIN_API_VERSION;
    let mut permissions: Vec<PluginPermission> = Vec::new();
    let mut entry: Option<PathBuf> = None;

    for raw in content.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let key = k.trim();
        let val = v.trim();

        match key {
            "id" => id = val.to_string(),
            "name" => name = val.to_string(),
            "version" => version = val.to_string(),
            "api_version" => {
                if let Ok(v) = val.parse::<u32>() {
                    api_version = v;
                }
            }
            "permissions" => {
                permissions = parse_permissions(val);
            }
            "entry" => entry = Some(PathBuf::from(val)),
            _ => {}
        }
    }

    if id.is_empty() {
        return None;
    }

    if name.is_empty() {
        name = id.clone();
    }

    let mut compatible = api_version == PLUGIN_API_VERSION;
    let mut compatibility_error = if compatible {
        None
    } else {
        Some(format!(
            "plugin api_version={} is incompatible with runtime api_version={}",
            api_version, PLUGIN_API_VERSION
        ))
    };

    if compatible && requires_trust(&permissions) && !trusted.iter().any(|t| t == &id) {
        compatible = false;
        compatibility_error = Some("plugin requests privileged permissions; add id to trusted_plugins".to_string());
    }

    let enabled = !disabled.iter().any(|d| d == &id);
    Some(PluginManifest {
        id,
        name,
        version,
        api_version,
        builtin: false,
        enabled,
        compatible,
        compatibility_error,
        permissions,
        entry,
    })
}

fn parse_permissions(raw: &str) -> Vec<PluginPermission> {
    raw.split(',')
        .map(str::trim)
        .filter_map(|p| match p {
            "fs_read" => Some(PluginPermission::FsRead),
            "fs_write" => Some(PluginPermission::FsWrite),
            "network" => Some(PluginPermission::Network),
            "process" => Some(PluginPermission::Process),
            _ => None,
        })
        .collect()
}

fn requires_trust(perms: &[PluginPermission]) -> bool {
    perms.iter().any(|p| matches!(p, PluginPermission::FsWrite | PluginPermission::Network | PluginPermission::Process))
}
