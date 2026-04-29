use std::fmt;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum SandboxError {
    InvalidPath(String),
    AccessDenied(String),
    ReadLimitExceeded { max_bytes: usize },
    Io(std::io::Error),
}

impl fmt::Display for SandboxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SandboxError::InvalidPath(msg) => write!(f, "{msg}"),
            SandboxError::AccessDenied(msg) => write!(f, "{msg}"),
            SandboxError::ReadLimitExceeded { max_bytes } => {
                write!(f, "Файл превышает лимит чтения ({max_bytes} байт)")
            }
            SandboxError::Io(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for SandboxError {}

impl From<std::io::Error> for SandboxError {
    fn from(value: std::io::Error) -> Self {
        SandboxError::Io(value)
    }
}

#[derive(Debug, Clone)]
pub struct AgentSandbox {
    root: PathBuf,
    max_read_bytes: usize,
}

impl AgentSandbox {
    pub fn new(root: PathBuf, max_read_bytes: usize) -> Result<Self, SandboxError> {
        let canon_root = root.canonicalize().map_err(|e| {
            SandboxError::InvalidPath(format!("Не удалось определить корень workspace: {e}"))
        })?;
        Ok(Self {
            root: canon_root,
            max_read_bytes,
        })
    }

    pub fn resolve_path(&self, user_path: &str) -> Result<PathBuf, SandboxError> {
        if user_path.trim().is_empty() {
            return Err(SandboxError::InvalidPath("Путь не должен быть пустым".to_string()));
        }

        let candidate = if Path::new(user_path).is_absolute() {
            PathBuf::from(user_path)
        } else {
            self.root.join(user_path)
        };

        let canon_candidate = candidate.canonicalize().map_err(|e| {
            SandboxError::InvalidPath(format!("Не удалось разрешить путь `{user_path}`: {e}"))
        })?;

        if !canon_candidate.starts_with(&self.root) {
            return Err(SandboxError::AccessDenied(format!(
                "Доступ к пути вне workspace запрещён: `{user_path}`"
            )));
        }

        Ok(canon_candidate)
    }

    pub fn read_file(&self, user_path: &str) -> Result<String, SandboxError> {
        let resolved = self.resolve_path(user_path)?;
        let mut file = File::open(&resolved)?;
        let mut buf = Vec::with_capacity(self.max_read_bytes.saturating_add(1));
        file.by_ref()
            .take((self.max_read_bytes as u64).saturating_add(1))
            .read_to_end(&mut buf)?;

        if buf.len() > self.max_read_bytes {
            return Err(SandboxError::ReadLimitExceeded {
                max_bytes: self.max_read_bytes,
            });
        }

        Ok(String::from_utf8_lossy(&buf).into_owned())
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn mk_temp_dir() -> PathBuf {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("tce-agent-sandbox-{id}"));
        fs::create_dir_all(&path).expect("должен создаться временный каталог");
        path
    }

    #[test]
    fn read_file_inside_workspace_ok() {
        let root = mk_temp_dir();
        let nested = root.join("src");
        fs::create_dir_all(&nested).expect("должен создаться вложенный каталог");
        let file = nested.join("main.rs");
        fs::write(&file, "fn main() {}\n").expect("должен записаться тестовый файл");

        let sandbox = AgentSandbox::new(root.clone(), 1024).expect("должна создаться песочница");
        let content = sandbox
            .read_file("src/main.rs")
            .expect("должно прочитаться содержимое");
        assert_eq!(content, "fn main() {}\n");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn resolve_path_blocks_parent_escape() {
        let root = mk_temp_dir();
        let outside = root
            .parent()
            .expect("у временного каталога должен быть родитель")
            .join("outside.txt");
        fs::write(&outside, "secret").expect("должен записаться внешний файл");

        let sandbox = AgentSandbox::new(root.clone(), 1024).expect("должна создаться песочница");
        let err = sandbox
            .resolve_path("../outside.txt")
            .expect_err("выход из workspace должен быть запрещён");
        assert!(
            matches!(err, SandboxError::AccessDenied(_)),
            "должна вернуться ошибка запрета доступа"
        );

        let _ = fs::remove_file(outside);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn read_file_respects_limit() {
        let root = mk_temp_dir();
        let file = root.join("big.txt");
        fs::write(&file, "1234567890").expect("должен записаться тестовый файл");

        let sandbox = AgentSandbox::new(root.clone(), 4).expect("должна создаться песочница");
        let err = sandbox
            .read_file("big.txt")
            .expect_err("должна сработать защита лимита чтения");

        assert!(
            matches!(err, SandboxError::ReadLimitExceeded { max_bytes: 4 }),
            "ожидается ошибка превышения лимита чтения"
        );

        let _ = fs::remove_dir_all(root);
    }
}
