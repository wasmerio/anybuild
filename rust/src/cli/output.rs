//! CLI output formatting utilities

use console::{style, Term};
use indicatif::{ProgressBar, ProgressStyle};
use std::time::Duration;

/// Output manager for CLI
pub struct Output {
    term: Term,
    colors_enabled: bool,
}

impl Output {
    /// Create a new output manager
    pub fn new(colors_enabled: bool) -> Self {
        Self {
            term: Term::stdout(),
            colors_enabled,
        }
    }

    /// Check if colors are enabled
    pub fn colors_enabled(&self) -> bool {
        self.colors_enabled
    }

    /// Print a success message
    pub fn success(&self, msg: impl AsRef<str>) {
        if self.colors_enabled {
            println!("{} {}", style("✅").green(), msg.as_ref());
        } else {
            println!("[SUCCESS] {}", msg.as_ref());
        }
    }

    /// Print an error message
    pub fn error(&self, msg: impl AsRef<str>) {
        if self.colors_enabled {
            eprintln!("{} {}", style("❌").red(), msg.as_ref());
        } else {
            eprintln!("[ERROR] {}", msg.as_ref());
        }
    }

    /// Print a warning message
    pub fn warning(&self, msg: impl AsRef<str>) {
        if self.colors_enabled {
            println!("{} {}", style("⚠️").yellow(), msg.as_ref());
        } else {
            println!("[WARNING] {}", msg.as_ref());
        }
    }

    /// Print an info message
    pub fn info(&self, msg: impl AsRef<str>) {
        if self.colors_enabled {
            println!("{} {}", style("ℹ️").cyan(), msg.as_ref());
        } else {
            println!("[INFO] {}", msg.as_ref());
        }
    }

    /// Print a step message
    pub fn step(&self, icon: &str, msg: impl AsRef<str>) {
        if self.colors_enabled {
            println!("{} {}", style(icon).bold(), msg.as_ref());
        } else {
            println!("{} {}", icon, msg.as_ref());
        }
    }

    /// Create a progress bar with a message
    pub fn progress(&self, msg: impl Into<String>) -> ProgressBar {
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.cyan} {msg}")
                .unwrap(),
        );
        pb.set_message(msg.into());
        pb.enable_steady_tick(Duration::from_millis(100));
        pb
    }

    /// Create a progress bar with a known length
    pub fn progress_bar(&self, len: u64, msg: impl Into<String>) -> ProgressBar {
        let pb = ProgressBar::new(len);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} {msg}")
                .unwrap()
                .progress_chars("█▓▒░ "),
        );
        pb.set_message(msg.into());
        pb
    }

    /// Print a header section
    pub fn header(&self, title: impl AsRef<str>) {
        if self.colors_enabled {
            println!();
            println!("{}", style("═".repeat(50)).dim());
            println!("{}", style(title.as_ref()).bold().cyan());
            println!("{}", style("═".repeat(50)).dim());
        } else {
            println!();
            println!("========================================");
            println!("{}", title.as_ref());
            println!("========================================");
        }
    }

    /// Print a key-value pair
    pub fn kv(&self, key: impl AsRef<str>, value: impl AsRef<str>) {
        if self.colors_enabled {
            println!(
                "  {} {}",
                style(format!("{}:", key.as_ref())).bold(),
                value.as_ref()
            );
        } else {
            println!("  {}: {}", key.as_ref(), value.as_ref());
        }
    }

    /// Clear the current line
    pub fn clear_line(&self) {
        let _ = self.term.clear_line();
    }

    /// Print a blank line
    pub fn blank(&self) {
        println!();
    }

    /// Print the Shipit banner
    pub fn banner(&self) {
        if self.colors_enabled {
            println!();
            println!(
                "{}",
                style("╭─────────────────────────────────────╮").cyan()
            );
            println!("{}", style("│  Shipit CLI v0.17.2                │").cyan());
            println!("{}", style("│  Build and serve your projects     │").cyan());
            println!(
                "{}",
                style("╰─────────────────────────────────────╯").cyan()
            );
            println!();
        } else {
            println!();
            println!("Shipit CLI v0.17.2");
            println!("Build and serve your projects");
            println!();
        }
    }
}

impl Default for Output {
    fn default() -> Self {
        Self::new(true)
    }
}

/// Format a file path for display
pub fn format_path(path: &std::path::Path) -> String {
    path.display().to_string()
}

/// Format a duration for display
pub fn format_duration(duration: Duration) -> String {
    let secs = duration.as_secs();
    if secs < 60 {
        format!("{:.1}s", duration.as_secs_f64())
    } else {
        let mins = secs / 60;
        let secs = secs % 60;
        format!("{}m {}s", mins, secs)
    }
}

/// Format file size for display
pub fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} bytes", bytes)
    }
}
