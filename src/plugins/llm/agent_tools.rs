use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use super::agent_sandbox::{AgentSandbox, SandboxError};

#[derive(Debug, Clone)]
pub struct ToolCall {
    pub tool: String,
    pub id: String,
    pub args: Value,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ToolResult {
    pub id: String,
    pub tool: String,
    pub ok: bool,
    pub result: Option<Value>,
    pub error: Option<String>,
}

pub struct AgentToolExecutor {
    sandbox: AgentSandbox,
    allow_unsafe: bool,
}

impl AgentToolExecutor {
    pub fn new(sandbox: AgentSandbox, allow_unsafe: bool) -> Self {
        Self {
            sandbox,
            allow_unsafe,
        }
    }

    pub fn execute_call(&self, call: &ToolCall) -> ToolResult {
        if is_dangerous_tool(&call.tool) && !self.allow_unsafe {
            return ToolResult {
                id: call.id.clone(),
                tool: call.tool.clone(),
                ok: false,
                result: None,
                error: Some(format!(
                    "Инструмент `{}` требует подтверждения. Включите `Agent: Toggle Unsafe Tools` и повторите.",
                    call.tool
                )),
            };
        }
        let outcome = match call.tool.as_str() {
            "read_file" => self.exec_read_file(&call.args),
            "list_dir" => self.exec_list_dir(&call.args),
            "glob_search" => self.exec_glob_search(&call.args),
            "search_content" => self.exec_search_content(&call.args),
            _ => Err(format!("Неподдерживаемый инструмент: {}", call.tool)),
        };

        match outcome {
            Ok(result) => ToolResult {
                id: call.id.clone(),
                tool: call.tool.clone(),
                ok: true,
                result: Some(result),
                error: None,
            },
            Err(error) => ToolResult {
                id: call.id.clone(),
                tool: call.tool.clone(),
                ok: false,
                result: None,
                error: Some(error),
            },
        }
    }

    fn exec_read_file(&self, args: &Value) -> Result<Value, String> {
        let path = arg_str(args, "path")?;
        let content = self
            .sandbox
            .read_file(path)
            .map_err(sandbox_error_message)?;
        Ok(json!({
            "path": path,
            "content": content,
        }))
    }

    fn exec_list_dir(&self, args: &Value) -> Result<Value, String> {
        let path = arg_str_opt(args, "path").unwrap_or(".");
        let resolved = self
            .sandbox
            .resolve_path(path)
            .map_err(sandbox_error_message)?;
        let mut entries = Vec::<Value>::new();
        let read_dir =
            fs::read_dir(&resolved).map_err(|e| format!("Не удалось прочитать каталог `{path}`: {e}"))?;

        for item in read_dir {
            let item = item.map_err(|e| format!("Ошибка чтения элемента каталога: {e}"))?;
            let meta = item
                .metadata()
                .map_err(|e| format!("Не удалось прочитать metadata: {e}"))?;
            entries.push(json!({
                "name": item.file_name().to_string_lossy().to_string(),
                "path": rel_from_root(self.sandbox.root(), &item.path()),
                "is_dir": meta.is_dir(),
                "size": if meta.is_file() { meta.len() } else { 0 },
            }));
        }

        entries.sort_by(|a, b| {
            a.get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .cmp(b.get("name").and_then(Value::as_str).unwrap_or(""))
        });

        Ok(json!({
            "path": path,
            "entries": entries,
        }))
    }

    fn exec_glob_search(&self, args: &Value) -> Result<Value, String> {
        let pattern = arg_str(args, "pattern")?;
        let base = arg_str_opt(args, "base").unwrap_or(".");
        let max_results = arg_usize_opt(args, "max_results").unwrap_or(200).clamp(1, 5000);
        let base_resolved = self
            .sandbox
            .resolve_path(base)
            .map_err(sandbox_error_message)?;

        let mut out = Vec::<Value>::new();
        visit_files(&base_resolved, &mut |file_path| {
            if out.len() >= max_results {
                return;
            }
            let rel = rel_from_root(self.sandbox.root(), file_path);
            if wildcard_match(pattern, &rel) {
                out.push(json!(rel));
            }
        })
        .map_err(|e| format!("Ошибка обхода файлов: {e}"))?;

        Ok(json!({
            "pattern": pattern,
            "base": base,
            "matches": out,
            "truncated": out.len() >= max_results,
        }))
    }

    fn exec_search_content(&self, args: &Value) -> Result<Value, String> {
        let query = arg_str(args, "query")?;
        let base = arg_str_opt(args, "base").unwrap_or(".");
        let case_insensitive = arg_bool_opt(args, "case_insensitive").unwrap_or(false);
        let max_results = arg_usize_opt(args, "max_results").unwrap_or(200).clamp(1, 5000);
        let base_resolved = self
            .sandbox
            .resolve_path(base)
            .map_err(sandbox_error_message)?;

        let needle = if case_insensitive {
            query.to_lowercase()
        } else {
            query.to_string()
        };

        let mut matches = Vec::<Value>::new();
        visit_files(&base_resolved, &mut |file_path| {
            if matches.len() >= max_results {
                return;
            }

            let rel = rel_from_root(self.sandbox.root(), file_path);
            let content = match self.sandbox.read_file(&rel) {
                Ok(content) => content,
                Err(SandboxError::ReadLimitExceeded { .. }) => return,
                Err(_) => return,
            };

            for (line_idx, line) in content.lines().enumerate() {
                if matches.len() >= max_results {
                    break;
                }

                let hay = if case_insensitive {
                    line.to_lowercase()
                } else {
                    line.to_string()
                };

                if hay.contains(&needle) {
                    matches.push(json!({
                        "path": rel,
                        "line": line_idx + 1,
                        "text": line,
                    }));
                }
            }
        })
        .map_err(|e| format!("Ошибка обхода файлов: {e}"))?;

        Ok(json!({
            "query": query,
            "base": base,
            "matches": matches,
            "truncated": matches.len() >= max_results,
        }))
    }
}

fn arg_str<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("Аргумент `{key}` обязателен и должен быть строкой"))
}

fn arg_str_opt<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str)
}

fn arg_bool_opt(args: &Value, key: &str) -> Option<bool> {
    args.get(key).and_then(Value::as_bool)
}

fn arg_usize_opt(args: &Value, key: &str) -> Option<usize> {
    args.get(key).and_then(Value::as_u64).map(|v| v as usize)
}

fn sandbox_error_message(error: SandboxError) -> String {
    error.to_string()
}

fn rel_from_root(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .ok()
        .unwrap_or(path)
        .to_string_lossy()
        .to_string()
}

fn visit_files<F>(base: &Path, on_file: &mut F) -> std::io::Result<()>
where
    F: FnMut(&Path),
{
    let mut stack = vec![PathBuf::from(base)];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            let meta = entry.metadata()?;
            if meta.is_dir() {
                stack.push(path);
            } else if meta.is_file() {
                on_file(&path);
            }
        }
    }
    Ok(())
}

fn wildcard_match(pattern: &str, text: &str) -> bool {
    let mut memo = BTreeMap::<(usize, usize), bool>::new();
    wildcard_match_impl(pattern.as_bytes(), text.as_bytes(), 0, 0, &mut memo)
}

fn wildcard_match_impl(
    pattern: &[u8],
    text: &[u8],
    p: usize,
    t: usize,
    memo: &mut BTreeMap<(usize, usize), bool>,
) -> bool {
    if let Some(v) = memo.get(&(p, t)) {
        return *v;
    }

    let answer = if p == pattern.len() {
        t == text.len()
    } else if pattern[p] == b'*' {
        wildcard_match_impl(pattern, text, p + 1, t, memo)
            || (t < text.len() && wildcard_match_impl(pattern, text, p, t + 1, memo))
    } else if pattern[p] == b'?' {
        t < text.len() && wildcard_match_impl(pattern, text, p + 1, t + 1, memo)
    } else {
        t < text.len()
            && pattern[p] == text[t]
            && wildcard_match_impl(pattern, text, p + 1, t + 1, memo)
    };

    memo.insert((p, t), answer);
    answer
}

fn is_dangerous_tool(tool: &str) -> bool {
    matches!(tool, "apply_patch" | "run_command")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn mk_temp_dir() -> PathBuf {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("tce-agent-tools-{id}"));
        fs::create_dir_all(&path).expect("должен создаться временный каталог");
        path
    }

    fn executor_with_files() -> (PathBuf, AgentToolExecutor) {
        let root = mk_temp_dir();
        fs::create_dir_all(root.join("src")).expect("должен создаться каталог src");
        fs::write(root.join("src/main.rs"), "fn main() {}\n").expect("должен записаться main.rs");
        fs::write(root.join("src/lib.rs"), "pub fn sum(a: i32, b: i32) -> i32 { a + b }\n")
            .expect("должен записаться lib.rs");
        fs::write(root.join("README.md"), "Project tce\n").expect("должен записаться README");

        let sandbox = AgentSandbox::new(root.clone(), 1024).expect("должна создаться песочница");
        (root, AgentToolExecutor::new(sandbox, false))
    }

    #[test]
    fn read_file_tool_works() {
        let (root, executor) = executor_with_files();
        let call = ToolCall {
            tool: "read_file".to_string(),
            id: "call-1".to_string(),
            args: json!({ "path": "src/main.rs" }),
        };

        let result = executor.execute_call(&call);
        assert!(result.ok, "read_file должен выполниться успешно");
        let content = result
            .result
            .as_ref()
            .and_then(|v| v.get("content"))
            .and_then(Value::as_str)
            .unwrap_or("");

        assert_eq!(content, "fn main() {}\n");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn list_dir_tool_returns_entries() {
        let (root, executor) = executor_with_files();
        let call = ToolCall {
            tool: "list_dir".to_string(),
            id: "call-2".to_string(),
            args: json!({ "path": "." }),
        };

        let result = executor.execute_call(&call);

        assert!(result.ok, "list_dir должен выполниться успешно");

        let entries = result
            .result
            .as_ref()
            .and_then(|v| v.get("entries"))
            .and_then(Value::as_array)
            .expect("entries должен быть массивом");

        assert!(
            entries
                .iter()
                .any(|e| e.get("name").and_then(Value::as_str) == Some("src")),
            "в списке должен быть каталог src"
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn glob_search_finds_rust_files() {
        let (root, executor) = executor_with_files();
        let call = ToolCall {
            tool: "glob_search".to_string(),
            id: "call-3".to_string(),
            args: json!({ "base": ".", "pattern": "*.rs" }),
        };
        let result = executor.execute_call(&call);

        assert!(result.ok, "glob_search должен выполниться успешно");

        let matches = result
            .result
            .as_ref()
            .and_then(|v| v.get("matches"))
            .and_then(Value::as_array)
            .expect("matches должен быть массивом");

        assert!(
            matches.iter().any(|v| v.as_str() == Some("src/main.rs")),
            "должен найтись файл src/main.rs"
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn search_content_finds_lines() {
        let (root, executor) = executor_with_files();
        let call = ToolCall {
            tool: "search_content".to_string(),
            id: "call-4".to_string(),
            args: json!({ "base": ".", "query": "sum" }),
        };
        let result = executor.execute_call(&call);

        assert!(result.ok, "search_content должен выполниться успешно");

        let matches = result
            .result
            .as_ref()
            .and_then(|v| v.get("matches"))
            .and_then(Value::as_array)
            .expect("matches должен быть массивом");

        assert!(
            matches.iter().any(|v| {
                v.get("path").and_then(Value::as_str) == Some("src/lib.rs")
                    && v.get("line").and_then(Value::as_u64) == Some(1)
            }),
            "должна найтись строка с sum в src/lib.rs"
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn dangerous_tool_is_blocked_without_confirmation() {
        let (root, executor) = executor_with_files();
        let call = ToolCall {
            tool: "run_command".to_string(),
            id: "call-unsafe".to_string(),
            args: json!({ "command": "echo test" }),
        };

        let result = executor.execute_call(&call);

        assert!(!result.ok, "опасный инструмент должен блокироваться");

        let err = result.error.unwrap_or_default();
        assert!(
            err.contains("требует подтверждения"),
            "должно быть сообщение про подтверждение опасного инструмента"
        );

        let _ = fs::remove_dir_all(root);
    }
}
