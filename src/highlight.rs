use syntect::easy::HighlightLines;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::html::{styled_line_to_highlighted_html, IncludeBackground};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

pub struct Highlighter {
    syntaxes: SyntaxSet,
    theme: Theme,
}

impl Highlighter {
    pub fn new() -> Self {
        let syntaxes = SyntaxSet::load_defaults_newlines();
        let themes = ThemeSet::load_defaults();
        let theme = themes
            .themes
            .get("base16-ocean.dark")
            .or_else(|| themes.themes.get("Solarized (dark)"))
            .cloned()
            .unwrap_or_else(|| themes.themes.values().next().cloned().unwrap());
        Highlighter { syntaxes, theme }
    }

    pub fn highlight(&self, code: &str, file_name: &str) -> Vec<String> {
        let ext = std::path::Path::new(file_name)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        let syntax = self
            .syntaxes
            .find_syntax_by_extension(ext)
            .or_else(|| {
                self.syntaxes
                    .find_syntax_by_first_line(code.lines().next().unwrap_or(""))
            })
            .unwrap_or_else(|| self.syntaxes.find_syntax_plain_text());

        let mut hl = HighlightLines::new(syntax, &self.theme);
        let mut out = Vec::new();
        for line in LinesWithEndings::from(code) {
            match hl.highlight_line(line, &self.syntaxes) {
                Ok(ranges) => {
                    let html = styled_line_to_highlighted_html(&ranges[..], IncludeBackground::No)
                        .unwrap_or_else(|_| escape_html(line));
                    out.push(html.trim_end_matches('\n').to_string());
                }
                Err(_) => out.push(escape_html(line.trim_end_matches('\n'))),
            }
        }
        if out.is_empty() {
            out.push(String::new());
        }
        out
    }
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
