//! Console rendering (port of `ui.py` + the rich Rule/Panel/Syntax
//! rendering used across the CLI).
//!
//! Color behavior mirrors rich: ANSI is emitted only when stderr is a
//! terminal, `NO_COLOR` disables it, `FORCE_COLOR` forces it. Syntax
//! highlighting itself is additionally behind the `syntax-highlighting`
//! cargo feature (default on) — built without it, panels render plain
//! regardless of TTY, with identical geometry.

use std::io::IsTerminal;

use anybuild::{BuildPlanPackage, BuildPlanStep, DeployScript, PackagePhase, WasmerPackageMapping};
use unicode_width::UnicodeWidthStr;

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
const BRIGHT_MAGENTA: &str = "\x1b[95m";
const BRIGHT_CYAN: &str = "\x1b[96m";
const HIGHLIGHT: &str = "\x1b[38;2;125;86;244m";
const DARK_GRAY: &str = "\x1b[38;5;238m";
const MEDIUM_GRAY: &str = "\x1b[38;5;245m";
const BOLD: &str = "\x1b[1m";
const RESET: &str = "\x1b[0m";

fn style(text: &str, ansi: &str, colors: bool) -> String {
    if colors {
        format!("{ansi}{text}{RESET}")
    } else {
        text.to_owned()
    }
}

fn section_header(output: &mut String, title: &str, colors: bool) {
    if !output.is_empty() {
        output.push('\n');
    }
    output.push_str("  ");
    output.push_str(&style(title, BOLD, colors));
    output.push('\n');
    output.push_str("  ");
    output.push_str(&style(
        &"─".repeat(title.chars().count().max(10)),
        DARK_GRAY,
        colors,
    ));
    output.push('\n');
}

pub fn render_section_header(title: &str, colors: bool) -> String {
    let mut output = String::new();
    section_header(&mut output, title, colors);
    output
}

pub fn render_provider_status(action: &str, provider: &str, suffix: &str, colors: bool) -> String {
    format!("  {action} {} {suffix}\n", style(provider, BOLD, colors))
}

pub fn render_build_progress(description: &str, colors: bool) -> String {
    format!("\n  {}\n", style(description, MEDIUM_GRAY, colors))
}

pub fn render_wasmer_package_mappings(mappings: &[WasmerPackageMapping], colors: bool) -> String {
    let source_width = mappings
        .iter()
        .map(|mapping| mapping.source.chars().count())
        .max()
        .unwrap_or(1);
    let separator = format!("  {}  ", style("│", DARK_GRAY, colors));
    let mut output = String::from("  Mapping dependencies to Wasmer packages:\n\n");
    for mapping in mappings {
        let source = format!(
            "{:<source_width$}",
            mapping.source,
            source_width = source_width
        );
        output.push_str("  ");
        output.push_str(&style(&source, BRIGHT_MAGENTA, colors));
        output.push_str(&separator);
        output.push_str(&style(&mapping.target, BRIGHT_CYAN, colors));
        output.push('\n');
    }
    output
}

pub fn render_success(message: &str, colors: bool) -> String {
    let inner_width = UnicodeWidthStr::width(message) + 2;
    let horizontal = style(&"─".repeat(inner_width), HIGHLIGHT, colors);
    let top_left = style("╭", HIGHLIGHT, colors);
    let top_right = style("╮", HIGHLIGHT, colors);
    let vertical = style("│", HIGHLIGHT, colors);
    let bottom_left = style("╰", HIGHLIGHT, colors);
    let bottom_right = style("╯", HIGHLIGHT, colors);
    format!(
        "\n  {top_left}{horizontal}{top_right}\n  {vertical} {message} {vertical}\n  {bottom_left}{horizontal}{bottom_right}\n"
    )
}

fn command_line(output: &mut String, command: &str, colors: bool) {
    output.push_str("    ");
    output.push_str(&style("$", MEDIUM_GRAY, colors));
    output.push(' ');
    output.push_str(&style(command, BOLD, colors));
    output.push('\n');
}

fn step_header(output: &mut String, name: &str, colors: bool) {
    output.push_str("  ");
    output.push_str(&style(&format!("▸ {name}"), BRIGHT_MAGENTA, colors));
    output.push('\n');
}

fn step_detail(output: &mut String, detail: &str, colors: bool) {
    output.push_str("    ");
    output.push_str(&style(detail, BOLD, colors));
    output.push('\n');
}

/// Railpack-inspired summary of the exact build plan that will execute.
pub fn render_build_plan(
    packages: &[BuildPlanPackage],
    steps: &[BuildPlanStep],
    prepare_steps: &[BuildPlanStep],
    deploy_scripts: &[DeployScript],
    show_detailed_steps: bool,
    colors: bool,
) -> String {
    let mut output = String::new();
    if !packages.is_empty() {
        section_header(&mut output, "Packages", colors);
        let name_width = packages
            .iter()
            .map(|package| package_display_name(package).chars().count())
            .max()
            .unwrap_or(1);
        let version_width = packages
            .iter()
            .map(|package| package.version.as_deref().unwrap_or("-").chars().count())
            .max()
            .unwrap_or(1);
        let separator = format!("  {}  ", style("│", DARK_GRAY, colors));
        for package in packages {
            let name = format!(
                "{:<name_width$}",
                package_display_name(package),
                name_width = name_width
            );
            let version = format!(
                "{:<version_width$}",
                package.version.as_deref().unwrap_or("-"),
                version_width = version_width
            );
            output.push_str("  ");
            output.push_str(&style(&name, BRIGHT_MAGENTA, colors));
            output.push_str(&separator);
            output.push_str(&style(&version, BRIGHT_CYAN, colors));
            match package.phase {
                PackagePhase::Build => {
                    output.push_str(&separator);
                    output.push_str(&style("only build", MEDIUM_GRAY, colors));
                }
                PackagePhase::Deploy => {
                    output.push_str(&separator);
                    output.push_str(&style("only deploy", MEDIUM_GRAY, colors));
                }
                PackagePhase::Both => {}
            }
            output.push('\n');
        }
    }

    render_steps_section(
        &mut output,
        "Build Steps",
        steps,
        show_detailed_steps,
        colors,
    );
    render_steps_section(&mut output, "Prepare", prepare_steps, true, colors);

    if !deploy_scripts.is_empty() {
        section_header(&mut output, "Deploy scripts", colors);
        for (index, script) in deploy_scripts.iter().enumerate() {
            if index > 0 {
                output.push('\n');
            }
            step_header(&mut output, &script.name, colors);
            command_line(&mut output, &script.command, colors);
        }
    }
    output.push('\n');
    output
}

fn render_steps_section(
    output: &mut String,
    title: &str,
    steps: &[BuildPlanStep],
    show_detailed_steps: bool,
    colors: bool,
) {
    let steps = steps
        .iter()
        .filter(|step| {
            show_detailed_steps
                || matches!(
                    step,
                    BuildPlanStep::Run {
                        group: Some(group),
                        ..
                    } if group == "install" || group == "build"
                )
        })
        .collect::<Vec<_>>();
    if steps.is_empty() {
        return;
    }

    section_header(output, title, colors);
    let mut previous_block: Option<String> = None;
    for (index, step) in steps.into_iter().enumerate() {
        let block = match step {
            BuildPlanStep::Run {
                group: Some(group), ..
            } => format!("group:{group}"),
            BuildPlanStep::Run { group: None, .. } => "commands".to_owned(),
            _ => format!("step:{index}"),
        };
        if previous_block.as_deref() != Some(&block) {
            if previous_block.is_some() {
                output.push('\n');
            }
            match step {
                BuildPlanStep::Run {
                    group: Some(group), ..
                } => step_header(output, group, colors),
                BuildPlanStep::Run { group: None, .. } => {}
                BuildPlanStep::Copy { .. } => step_header(output, "copy", colors),
                BuildPlanStep::Environment { .. } => step_header(output, "environment", colors),
                BuildPlanStep::Path { .. } => step_header(output, "path", colors),
                BuildPlanStep::Workdir { .. } => step_header(output, "workdir", colors),
                BuildPlanStep::WriteFile { .. } => step_header(output, "write file", colors),
            }
        }
        match step {
            BuildPlanStep::Run { command, .. } => command_line(output, command, colors),
            BuildPlanStep::Copy {
                source,
                target,
                base,
            } => {
                let detail = if base == "source" {
                    format!("{source} → {target}")
                } else {
                    format!("{source} → {target} ({base})")
                };
                step_detail(output, &detail, colors);
            }
            BuildPlanStep::Environment { variables } => {
                step_detail(output, &variables.join(", "), colors)
            }
            BuildPlanStep::Path { path } => step_detail(output, path, colors),
            BuildPlanStep::Workdir { path } => {
                step_detail(output, &path.display().to_string(), colors)
            }
            BuildPlanStep::WriteFile { path } => step_detail(output, path, colors),
        }
        previous_block = Some(block);
    }
}

fn package_display_name(package: &BuildPlanPackage) -> String {
    package.architecture.as_ref().map_or_else(
        || package.name.clone(),
        |architecture| format!("{} ({architecture})", package.name),
    )
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
    use std::path::PathBuf;

    use anybuild::{
        BuildPlanPackage, BuildPlanStep, DeployScript, PackagePhase, WasmerPackageMapping,
    };

    use super::{
        render_build_plan, render_build_progress, render_panel, render_provider_status,
        render_section_header, render_success, render_wasmer_package_mappings,
    };

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

    #[test]
    fn build_plan_renders_all_steps_and_package_phases() {
        let packages = vec![
            BuildPlanPackage {
                name: "node".to_owned(),
                version: Some("24".to_owned()),
                architecture: None,
                phase: PackagePhase::Both,
            },
            BuildPlanPackage {
                name: "npm".to_owned(),
                version: None,
                architecture: None,
                phase: PackagePhase::Build,
            },
        ];
        let steps = vec![
            BuildPlanStep::Workdir {
                path: PathBuf::from("opt/build"),
            },
            BuildPlanStep::Environment {
                variables: vec!["CI".to_owned()],
            },
            BuildPlanStep::Run {
                command: "npm ci".to_owned(),
                group: Some("install".to_owned()),
            },
            BuildPlanStep::Run {
                command: "npm install extra".to_owned(),
                group: Some("install".to_owned()),
            },
            BuildPlanStep::Copy {
                source: ".".to_owned(),
                target: ".".to_owned(),
                base: "source".to_owned(),
            },
            BuildPlanStep::Run {
                command: "npm run build".to_owned(),
                group: Some("build".to_owned()),
            },
            BuildPlanStep::Run {
                command: "npm prune".to_owned(),
                group: None,
            },
        ];
        let deploy = vec![DeployScript {
            name: "start".to_owned(),
            command: "npm run start".to_owned(),
        }];
        let prepare = vec![BuildPlanStep::Run {
            command: "npm run migrate".to_owned(),
            group: None,
        }];

        let plain = render_build_plan(&packages, &steps, &prepare, &deploy, true, false);
        assert!(plain.contains("node  │  24\n"));
        assert!(plain.contains("npm   │  -   │  only build"));
        assert!(plain.contains("  ▸ workdir\n    opt/build"));
        assert!(plain.contains("  ▸ install\n    $ npm ci\n    $ npm install extra"));
        assert!(plain.contains("  ▸ copy\n    . → ."));
        assert!(plain.contains("    $ npm prune"));
        assert!(plain.contains("  Prepare\n  ──────────\n    $ npm run migrate"));
        assert!(plain.contains("  Deploy scripts\n"));
        assert!(plain.contains("  ▸ start\n    $ npm run start"));

        let concise = render_build_plan(&packages, &steps, &prepare, &deploy, false, false);
        assert!(!concise.contains("▸ workdir"));
        assert!(!concise.contains("▸ copy"));
        assert!(!concise.contains("▸ environment"));
        assert!(!concise.contains("npm prune"));
        assert!(concise.contains("▸ install"));
        assert!(concise.contains("▸ build"));

        let colored = render_build_plan(&packages, &steps, &prepare, &deploy, true, true);
        assert_eq!(strip_ansi(&colored), plain);
        assert!(colored.contains("\x1b[95mnode"));
        assert!(colored.contains("\x1b[96m24"));
        assert!(colored.contains("\x1b[38;5;245m$"));
        assert!(colored.contains("\x1b[1mnpm ci"));
    }

    #[test]
    fn build_execution_uses_section_and_subdued_progress_styles() {
        assert_eq!(
            render_section_header("Starting Build...", false),
            "  Starting Build...\n  ─────────────────\n"
        );
        assert_eq!(
            render_build_progress("Copy to . from .", false),
            "\n  Copy to . from .\n"
        );
        assert!(render_build_progress("Copy to . from .", true)
            .contains("\x1b[38;5;245mCopy to . from ."));
    }

    #[test]
    fn wasmer_package_mappings_are_aligned_like_packages() {
        let mappings = vec![
            WasmerPackageMapping {
                source: "node@24".to_owned(),
                target: "wasmer/edgejs-quickjs@=0.1.0".to_owned(),
            },
            WasmerPackageMapping {
                source: "bash".to_owned(),
                target: "wasmer/bash@=1.0.25".to_owned(),
            },
        ];

        assert_eq!(
            render_wasmer_package_mappings(&mappings, false),
            concat!(
                "  Mapping dependencies to Wasmer packages:\n",
                "\n",
                "  node@24  │  wasmer/edgejs-quickjs@=0.1.0\n",
                "  bash     │  wasmer/bash@=1.0.25\n",
            )
        );
    }

    #[test]
    fn success_result_uses_an_indented_railpack_style_box() {
        let plain = render_success("Build complete in 1.08s", false);
        assert!(plain.starts_with("\n  ╭"));
        assert!(plain.contains("\n  │ Build complete in 1.08s │\n"));
        assert!(plain.ends_with("╯\n"));

        let colored = render_success("Build complete in 1.08s", true);
        assert_eq!(strip_ansi(&colored), plain);
        assert!(colored.contains("\x1b[38;2;125;86;244m"));
    }

    #[test]
    fn provider_status_bolds_only_the_provider_name() {
        assert_eq!(
            render_provider_status("Detected", "Node.js", "provider", false),
            "  Detected Node.js provider\n"
        );
        assert_eq!(
            render_provider_status("Detected", "Node.js", "provider", true),
            "  Detected \x1b[1mNode.js\x1b[0m provider\n"
        );
    }
}
