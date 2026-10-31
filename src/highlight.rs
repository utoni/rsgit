use std::collections::HashMap;
use tree_sitter_highlight::{HighlightConfiguration, HighlightEvent, Highlighter as TsCursor};

const MAX_HL_BYTES: usize = 512 * 1024;

const HL: &[(&str, &str)] = &[
    ("attribute", "tsh-attr"),
    ("comment", "tsh-comment"),
    ("constant", "tsh-const"),
    ("constant.builtin", "tsh-const"),
    ("constructor", "tsh-type"),
    ("function", "tsh-fn"),
    ("function.builtin", "tsh-fn"),
    ("function.method", "tsh-fn"),
    ("keyword", "tsh-kw"),
    ("label", "tsh-label"),
    ("number", "tsh-num"),
    ("operator", "tsh-op"),
    ("property", "tsh-prop"),
    ("punctuation", "tsh-punct"),
    ("punctuation.bracket", "tsh-punct"),
    ("punctuation.delimiter", "tsh-punct"),
    ("string", "tsh-str"),
    ("string.special", "tsh-str"),
    ("tag", "tsh-tag"),
    ("type", "tsh-type"),
    ("type.builtin", "tsh-type"),
    ("variable", "tsh-var"),
    ("variable.builtin", "tsh-var"),
    ("variable.parameter", "tsh-var"),
];

pub struct Highlighter {
    configs: HashMap<&'static str, HighlightConfiguration>,
    classes: Vec<&'static str>,
}

impl Highlighter {
    pub fn new() -> Self {
        let names: Vec<String> = HL.iter().map(|(n, _)| n.to_string()).collect();
        let classes: Vec<&'static str> = HL.iter().map(|(_, c)| *c).collect();

        let mut configs = HashMap::new();
        for (key, cfg) in build_configs(&names) {
            configs.insert(key, cfg);
        }
        eprintln!("Loaded {} tree-sitter-grammars", configs.len());

        Highlighter { configs, classes }
    }

    pub fn highlight(&self, code: &str, file_name: &str) -> Vec<String> {
        let Some(key) = lang_for(file_name) else {
            return escape_lines(code);
        };
        let Some(cfg) = self.configs.get(key) else {
            return escape_lines(code);
        };
        if code.len() > MAX_HL_BYTES {
            return escape_lines(code);
        }

        let mut cursor = TsCursor::new();
        let events = match cursor.highlight(cfg, code.as_bytes(), None, |_| None) {
            Ok(ev) => ev,
            Err(_) => return escape_lines(code),
        };

        render_lines(events, code, &self.classes)
    }
}

fn render_lines<I>(events: I, code: &str, classes: &[&'static str]) -> Vec<String>
where
    I: Iterator<Item = Result<HighlightEvent, tree_sitter_highlight::Error>>,
{
    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut stack: Vec<usize> = Vec::new();

    let open_span = |buf: &mut String, idx: usize| {
        if let Some(cls) = classes.get(idx) {
            buf.push_str("<span class=\"");
            buf.push_str(cls);
            buf.push_str("\">");
        } else {
            buf.push_str("<span>");
        }
    };

    for event in events {
        let Ok(event) = event else {
            return escape_lines(code);
        };
        match event {
            HighlightEvent::HighlightStart(h) => {
                stack.push(h.0);
                open_span(&mut cur, h.0);
            }
            HighlightEvent::HighlightEnd => {
                if stack.pop().is_some() {
                    cur.push_str("</span>");
                }
            }
            HighlightEvent::Source { start, end } => {
                let piece = slice_lossy(code, start, end);
                let mut segs = piece.split('\n').peekable();
                while let Some(seg) = segs.next() {
                    push_escaped(&mut cur, seg);
                    if segs.peek().is_some() {
                        for _ in 0..stack.len() {
                            cur.push_str("</span>");
                        }
                        lines.push(std::mem::take(&mut cur));
                        for &idx in &stack {
                            open_span(&mut cur, idx);
                        }
                    }
                }
            }
        }
    }
    lines.push(cur);
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn lang_for(file_name: &str) -> Option<&'static str> {
    let path = std::path::Path::new(file_name);
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let key = match ext.as_str() {
        "rs" => "rust",
        "py" | "pyi" => "python",
        "js" | "mjs" | "cjs" | "jsx" => "javascript",
        "ts" => "typescript",
        "cpp" | "cxx" | "c" | "h" | "hpp" => "c",
        "go" => "go",
        "json" => "json",
        "sh" | "bash" => "bash",
        "html" | "htm" => "html",
        "md" | "markdown" => "md",
        "patch" => "diff",
        "yml" | "yaml" => "yaml",
        "" | _ => {
            let basename = path
                .file_prefix()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            let key = match basename.as_str() {
                "makefile" | "gnumakefile" => Some("make"),
                _ => None,
            };
            return key;
        }
    };
    Some(key)
}

fn build_configs(names: &[String]) -> Vec<(&'static str, HighlightConfiguration)> {
    let mut v: Vec<(&'static str, HighlightConfiguration)> = Vec::new();

    let mut add = |key: &'static str, cfg: Result<HighlightConfiguration, _>| match cfg {
        Ok(mut c) => {
            c.configure(names);
            v.push((key, c));
        }
        Err(e) => eprintln!("[highlight] Config '{key}' übersprungen: {e:?}"),
    };

    add(
        "rust",
        HighlightConfiguration::new(
            tree_sitter_rust::LANGUAGE.into(),
            "rust",
            tree_sitter_rust::HIGHLIGHTS_QUERY,
            tree_sitter_rust::INJECTIONS_QUERY,
            "",
        ),
    );

    add(
        "python",
        HighlightConfiguration::new(
            tree_sitter_python::LANGUAGE.into(),
            "python",
            tree_sitter_python::HIGHLIGHTS_QUERY,
            "",
            "",
        ),
    );

    add(
        "javascript",
        HighlightConfiguration::new(
            tree_sitter_javascript::LANGUAGE.into(),
            "javascript",
            tree_sitter_javascript::HIGHLIGHT_QUERY,
            tree_sitter_javascript::INJECTIONS_QUERY,
            tree_sitter_javascript::LOCALS_QUERY,
        ),
    );

    add(
        "c",
        HighlightConfiguration::new(
            tree_sitter_c::LANGUAGE.into(),
            "c",
            tree_sitter_c::HIGHLIGHT_QUERY,
            "",
            "",
        ),
    );

    add(
        "go",
        HighlightConfiguration::new(
            tree_sitter_go::LANGUAGE.into(),
            "go",
            tree_sitter_go::HIGHLIGHTS_QUERY,
            "",
            "",
        ),
    );

    add(
        "json",
        HighlightConfiguration::new(
            tree_sitter_json::LANGUAGE.into(),
            "json",
            tree_sitter_json::HIGHLIGHTS_QUERY,
            "",
            "",
        ),
    );

    add(
        "bash",
        HighlightConfiguration::new(
            tree_sitter_bash::LANGUAGE.into(),
            "bash",
            tree_sitter_bash::HIGHLIGHT_QUERY,
            "",
            "",
        ),
    );

    add(
        "html",
        HighlightConfiguration::new(
            tree_sitter_html::LANGUAGE.into(),
            "html",
            tree_sitter_html::HIGHLIGHTS_QUERY,
            "",
            "",
        ),
    );

    add(
        "diff",
        HighlightConfiguration::new(
            tree_sitter_diff::LANGUAGE.into(),
            "diff",
            tree_sitter_diff::HIGHLIGHTS_QUERY,
            "",
            "",
        ),
    );

    add(
        "make",
        HighlightConfiguration::new(
            tree_sitter_make::LANGUAGE.into(),
            "make",
            tree_sitter_make::HIGHLIGHTS_QUERY,
            "",
            "",
        ),
    );

    add(
        "yaml",
        HighlightConfiguration::new(
            tree_sitter_yaml::LANGUAGE.into(),
            "yaml",
            tree_sitter_yaml::HIGHLIGHTS_QUERY,
            "",
            "",
        ),
    );

    add(
        "md",
        HighlightConfiguration::new(
            tree_sitter_md::LANGUAGE.into(),
            "md",
            tree_sitter_md::HIGHLIGHT_QUERY_BLOCK,
            "",
            "",
        ),
    );

    v
}

fn slice_lossy(code: &str, start: usize, end: usize) -> std::borrow::Cow<'_, str> {
    if code.is_char_boundary(start) && code.is_char_boundary(end) {
        std::borrow::Cow::Borrowed(&code[start..end])
    } else {
        String::from_utf8_lossy(&code.as_bytes()[start..end])
    }
}

fn push_escaped(buf: &mut String, s: &str) {
    for ch in s.chars() {
        match ch {
            '&' => buf.push_str("&amp;"),
            '<' => buf.push_str("&lt;"),
            '>' => buf.push_str("&gt;"),
            _ => buf.push(ch),
        }
    }
}

fn escape_lines(code: &str) -> Vec<String> {
    let mut out: Vec<String> = code
        .split('\n')
        .map(|l| {
            let mut s = String::new();
            push_escaped(&mut s, l.trim_end_matches('\r'));
            s
        })
        .collect();
    if out.is_empty() {
        out.push(String::new());
    }
    out
}
