//! Console rendering (port of `ui.py` + the rich Rule/Panel/Syntax
//! rendering used across the CLI).
//!
//! Color behavior mirrors rich: ANSI is emitted only when stderr is a
//! terminal, `NO_COLOR` disables it, `FORCE_COLOR` forces it. Syntax
//! highlighting itself is additionally behind the `syntax-highlighting`
//! cargo feature (default on) — built without it, panels render plain
//! regardless of TTY, with identical geometry.

use std::io::IsTerminal;

/// `console.print(...)` — rich writes to stderr (`Console(stderr=True)`).
pub fn console_print(text: &str) {
    eprintln!("{text}");
}

/// `Rule(characters="-")` at rich's piped-output width (80).
pub fn rule() {
    eprintln!("{}", "-".repeat(80));
}

/// rich's color decision for a stream: NO_COLOR wins, FORCE_COLOR forces,
/// otherwise color only when stderr is a terminal.
pub fn colors_enabled() -> bool {
    if std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty()) {
        return false;
    }
    if std::env::var_os("FORCE_COLOR").is_some_and(|v| !v.is_empty()) {
        return true;
    }
    std::io::stderr().is_terminal()
}

const BRIGHT_BLACK: &str = "\x1b[90m";
const RESET: &str = "\x1b[0m";

/// Rich `Panel(Syntax(..., line_numbers=True), box=box.SQUARE,
/// expand=False, border_style="bright_black")`: a square box around
/// line-numbered content, capped at width 80 (long lines cropped).
pub fn print_panel(content: &str) {
    eprint!("{}", render_panel(content, None, colors_enabled()));
}

/// `print_panel` with monokai syntax highlighting for `lang` (one of the
/// lexer tokens the Python CLI used: json/bash/toml/yaml/dockerfile).
pub fn print_syntax_panel(content: &str, lang: &str) {
    eprint!("{}", render_panel(content, Some(lang), colors_enabled()));
}

/// Pure renderer behind the panel printers (separable for tests).
/// Geometry is always computed from the plain text, so colored and plain
/// output have identical layout.
pub fn render_panel(content: &str, lang: Option<&str>, colors: bool) -> String {
    let lines: Vec<&str> = content.split('\n').collect();
    // rich Syntax: len(str(start_line + newline_count)) + 2
    let num_width = lines.len().to_string().len() + 2;
    let max_code = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
    let inner = (num_width + 1 + max_code + 2).min(78);
    let code_width = inner - num_width - 3;

    let highlighted: Option<Vec<String>> = if colors {
        lang.and_then(|lang| highlight::highlight_lines(&lines, lang, code_width))
    } else {
        None
    };

    let (border_on, border_off) = if colors {
        (BRIGHT_BLACK, RESET)
    } else {
        ("", "")
    };

    let mut out = String::new();
    out.push_str(&format!("{border_on}┌{}┐{border_off}\n", "─".repeat(inner)));
    for (i, line) in lines.iter().enumerate() {
        let plain_cropped: String = line.chars().take(code_width).collect();
        let visible = plain_cropped.chars().count();
        let body = match &highlighted {
            Some(lines) => lines[i].clone(),
            None => plain_cropped,
        };
        let padding = " ".repeat(code_width - visible);
        let number = format!("{:>num_width$}", i + 1);
        let number = if colors {
            format!("{BRIGHT_BLACK}{number}{RESET}")
        } else {
            number
        };
        out.push_str(&format!(
            "{border_on}│{border_off} {number} {body}{padding} {border_on}│{border_off}\n"
        ));
    }
    out.push_str(&format!("{border_on}└{}┘{border_off}\n", "─".repeat(inner)));
    out
}

#[cfg(feature = "syntax-highlighting")]
mod highlight {
    use std::sync::LazyLock;

    use syntect::easy::HighlightLines;
    use syntect::highlighting::{Style, Theme};
    use syntect::parsing::SyntaxSet;
    use syntect::util::as_24_bit_terminal_escaped;

    /// bat's extended syntax set (adds TOML and Dockerfile over syntect's
    /// defaults; the newlines variant matches line-based highlighting).
    static SYNTAXES: LazyLock<SyntaxSet> = LazyLock::new(two_face::syntax::extra_newlines);
    /// rich's `theme="monokai"` equivalent.
    static THEME: LazyLock<Theme> = LazyLock::new(|| {
        two_face::theme::extra()
            .get(two_face::theme::EmbeddedThemeName::MonokaiExtended)
            .clone()
    });

    /// Highlight every line (stateful across the whole text, cropped to
    /// `width` visible chars). None when the lexer token is unknown.
    pub fn highlight_lines(lines: &[&str], lang: &str, width: usize) -> Option<Vec<String>> {
        let syntax = SYNTAXES.find_syntax_by_token(lang).or_else(|| {
            SYNTAXES
                .syntaxes()
                .iter()
                .find(|s| s.name.eq_ignore_ascii_case(lang))
        })?;
        let mut highlighter = HighlightLines::new(syntax, &THEME);
        let mut out = Vec::with_capacity(lines.len());
        for line in lines {
            let regions = highlighter.highlight_line(line, &SYNTAXES).ok()?;
            let cropped = crop_regions(&regions, width);
            let mut rendered = as_24_bit_terminal_escaped(&cropped, false);
            rendered.push_str(super::RESET);
            out.push(rendered);
        }
        Some(out)
    }

    /// Cut styled regions at `width` visible characters (crop before any
    /// escape codes exist, so widths never count ANSI bytes).
    fn crop_regions<'a>(regions: &[(Style, &'a str)], width: usize) -> Vec<(Style, &'a str)> {
        let mut out = Vec::new();
        let mut used = 0usize;
        for (style, text) in regions {
            if used >= width {
                break;
            }
            let remaining = width - used;
            let chars = text.chars().count();
            if chars <= remaining {
                out.push((*style, *text));
                used += chars;
            } else {
                let byte_end = text
                    .char_indices()
                    .nth(remaining)
                    .map(|(i, _)| i)
                    .unwrap_or(text.len());
                out.push((*style, &text[..byte_end]));
                used = width;
            }
        }
        out
    }
}

#[cfg(not(feature = "syntax-highlighting"))]
mod highlight {
    /// Built without the `syntax-highlighting` feature: no highlighter, the
    /// panel falls back to plain content (geometry unchanged).
    pub fn highlight_lines(_lines: &[&str], _lang: &str, _width: usize) -> Option<Vec<String>> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::render_panel;

    fn strip_ansi(text: &str) -> String {
        let mut out = String::new();
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\x1b' {
                for c in chars.by_ref() {
                    if c == 'm' {
                        break;
                    }
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    const TOML: &str = "[package]\nname = \"demo\"\nversion = \"1.0\"";

    #[test]
    fn plain_panel_geometry_is_stable() {
        let plain = render_panel(TOML, None, false);
        assert!(plain.contains("│   1 [package]"));
        assert!(!plain.contains('\x1b'));
    }

    #[test]
    fn colored_panel_strips_back_to_plain() {
        // The colored rendering must be the plain rendering plus ANSI only:
        // identical geometry, identical text.
        let plain = render_panel(TOML, Some("toml"), false);
        let colored = render_panel(TOML, Some("toml"), true);
        assert_eq!(strip_ansi(&colored), plain);
    }

    #[test]
    fn long_lines_crop_identically_when_colored() {
        let long = format!("key = \"{}\"", "x".repeat(200));
        let plain = render_panel(&long, Some("toml"), false);
        let colored = render_panel(&long, Some("toml"), true);
        assert_eq!(strip_ansi(&colored), plain);
    }

    #[test]
    fn unknown_language_falls_back_to_plain_text() {
        let colored = render_panel(TOML, Some("not-a-lang"), true);
        // Borders/numbers may carry color, but the code text is uncolored.
        assert!(strip_ansi(&colored).contains("name = \"demo\""));
    }
}
