use std::path::{Path, PathBuf};

pub const FIRST_WAVE_EXTENSIONS: &[&str] = &[
    "rs", "py", "pyi", "go", "ts", "tsx", "js", "jsx", "mjs", "cjs",
];

pub fn is_first_wave_path(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
        FIRST_WAVE_EXTENSIONS.iter().any(|&x| x.eq_ignore_ascii_case(e))
    })
}

pub fn syntax_highlight_line(path: Option<&PathBuf>, line: &str) -> String {
    let ext = path
        .and_then(|p| p.extension())
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "rs" => highlight_rust(line),
        "py" | "pyi" => highlight_python(line),
        "go" => highlight_go(line),
        "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" => highlight_js_ts(line),
        _ => line.to_string(),
    }
}

const RUST_KW: &[&str] = &[
    "fn", "let", "mut", "pub", "impl", "struct", "enum", "trait", "use", "mod", "if", "else",
    "match", "for", "while", "loop", "return", "crate", "self", "Self", "async", "await",
    "where", "type", "const", "static", "unsafe", "move", "ref", "dyn", "as", "break",
    "continue", "in", "super", "union", "yield",
];

const PY_KW: &[&str] = &[
    "def", "class", "if", "elif", "else", "for", "while", "try", "except", "finally", "with",
    "as", "import", "from", "return", "pass", "break", "continue", "raise", "lambda", "yield",
    "async", "await", "global", "nonlocal", "assert", "del", "and", "or", "not", "in", "is",
    "True", "False", "None",
];

const GO_KW: &[&str] = &[
    "package", "import", "func", "var", "const", "type", "struct", "interface", "map", "chan",
    "if", "else", "for", "range", "return", "go", "defer", "select", "case", "default",
    "switch", "break", "continue", "fallthrough", "goto",
];

const JS_TS_KW: &[&str] = &[
    "function", "const", "let", "var", "class", "extends", "implements", "interface", "type",
    "enum", "namespace", "module", "import", "export", "from", "as", "default", "return",
    "async", "await", "new", "this", "super", "static", "public", "private", "protected",
    "readonly", "if", "else", "for", "while", "do", "switch", "case", "break", "continue",
    "try", "catch", "finally", "throw", "typeof", "instanceof", "void", "null", "undefined",
    "true", "false",
];

fn highlight_rust(line: &str) -> String {
    highlight_slash_comment(line, RUST_KW)
}

fn highlight_python(line: &str) -> String {
    highlight_hash_comment(line, PY_KW)
}

fn highlight_go(line: &str) -> String {
    highlight_slash_comment(line, GO_KW)
}

fn highlight_js_ts(line: &str) -> String {
    highlight_slash_comment(line, JS_TS_KW)
}

fn highlight_hash_comment(line: &str, keywords: &[&str]) -> String {
    let mut out = String::new();
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '#' {
            out.push_str("\x1b[32m#");
            for c in chars {
                out.push(c);
            }

            out.push_str("\x1b[0m");
            break;
        }

        if ch == '"' || ch == '\'' {
            out.push_str("\x1b[33m");
            out.push(ch);
            let quote = ch;
            while let Some(c) = chars.next() {
                out.push(c);
                if c == '\\' {
                    if let Some(n) = chars.next() {
                        out.push(n);
                    }
                    continue;
                }

                if c == quote {
                    break;
                }
            }

            out.push_str("\x1b[0m");
            continue;
        }
        if ch.is_alphabetic() || ch == '_' {
            let mut ident = String::from(ch);
            while let Some(next) = chars.peek().copied() {
                if next.is_alphanumeric() || next == '_' {
                    ident.push(next);
                    let _ = chars.next();
                } else {
                    break;
                }
            }
            
            push_keyword_or_ident(&mut out, &ident, keywords);
            continue;
        }
        out.push(ch);
    }
    out
}

fn highlight_slash_comment(line: &str, keywords: &[&str]) -> String {
    let mut out = String::new();
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '/' && chars.peek() == Some(&'/') {
            out.push_str("\x1b[32m//");
            let _ = chars.next();
            for c in chars {
                out.push(c);
            }

            out.push_str("\x1b[0m");
            break;
        }

        if ch == '"' || ch == '`' {
            out.push_str("\x1b[33m");
            out.push(ch);
            let quote = ch;
            while let Some(c) = chars.next() {
                out.push(c);
                if c == quote {
                    break;
                }

                if c == '\\' && quote == '"' {
                    if let Some(n) = chars.next() {
                        out.push(n);
                    }
                }
            }

            out.push_str("\x1b[0m");

            continue;
        }

        if ch == '\'' && chars.peek().is_some_and(|c| *c != '\\' && *c != '\n') {
            out.push_str("\x1b[33m'");
            while let Some(c) = chars.next() {
                out.push(c);
                if c == '\'' {
                    break;
                }

                if c == '\\' {
                    if let Some(n) = chars.next() {
                        out.push(n);
                    }
                }
            }

            out.push_str("\x1b[0m");

            continue;
        }

        if ch.is_alphabetic() || ch == '_' {
            let mut ident = String::from(ch);
            while let Some(next) = chars.peek().copied() {
                if next.is_alphanumeric() || next == '_' {
                    ident.push(next);
                    let _ = chars.next();
                } else {
                    break;
                }
            }

            push_keyword_or_ident(&mut out, &ident, keywords);
            
            continue;
        }
        out.push(ch);
    }
    out
}

fn push_keyword_or_ident(out: &mut String, ident: &str, keywords: &[&str]) {
    if keywords.iter().any(|k| *k == ident) {
        out.push_str("\x1b[36m");
        out.push_str(ident);
        out.push_str("\x1b[0m");
    } else {
        out.push_str(ident);
    }
}
