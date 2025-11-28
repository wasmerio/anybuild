//! Minimal Starlark AST + renderer for Shipit generation.
//!
//! This is intentionally lightweight: just enough expressions and statements
//! to render the Shipit files we generate, while keeping control over
//! formatting and identifiers (no string interpolation).

use std::fmt;

/// Writer used during rendering.
struct Writer {
    output: String,
    indent: usize,
    at_line_start: bool,
}

impl Writer {
    fn new() -> Self {
        Self {
            output: String::new(),
            indent: 0,
            at_line_start: true,
        }
    }

    fn newline(&mut self) {
        self.output.push('\n');
        self.at_line_start = true;
    }

    fn write_str(&mut self, s: &str) {
        if self.at_line_start {
            for _ in 0..self.indent {
                self.output.push(' ');
            }
            self.at_line_start = false;
        }
        self.output.push_str(s);
    }

    fn indented<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        self.indent += 4;
        let r = f(self);
        self.indent -= 4;
        r
    }
}

/// A Starlark expression.
#[derive(Clone, Debug)]
pub enum Expr {
    Ident(String),
    StringLit(String),
    Call { name: String, args: Vec<Arg> },
    List(Vec<Expr>),
    Dict(Vec<(Expr, Expr)>),
    Or(Box<Expr>, Box<Expr>),
    Index(Box<Expr>, Box<Expr>),
    Raw(String),
}

/// Function call argument.
#[derive(Clone, Debug)]
pub enum Arg {
    Pos(Expr),
    Named(String, Expr),
}

/// Top-level statement.
#[derive(Clone, Debug)]
pub enum Stmt {
    Assignment { target: String, value: Expr },
    Expr(Expr),
    Raw(String),
}

impl Expr {
    pub fn call(name: impl Into<String>, args: Vec<Arg>) -> Self {
        Expr::Call {
            name: name.into(),
            args,
        }
    }
}

impl Arg {
    pub fn pos(expr: Expr) -> Self {
        Arg::Pos(expr)
    }

    pub fn named(name: impl Into<String>, expr: Expr) -> Self {
        Arg::Named(name.into(), expr)
    }
}

impl Stmt {
    pub fn assignment(target: impl Into<String>, value: Expr) -> Self {
        Stmt::Assignment {
            target: target.into(),
            value,
        }
    }
}

/// Render a sequence of statements into a Starlark string.
pub fn render_module(stmts: &[Stmt]) -> String {
    let mut w = Writer::new();
    for (i, stmt) in stmts.iter().enumerate() {
        render_stmt(stmt, &mut w);
        if i + 1 != stmts.len() {
            w.newline();
        }
        w.newline();
    }
    w.output
}

fn render_stmt(stmt: &Stmt, w: &mut Writer) {
    match stmt {
        Stmt::Assignment { target, value } => {
            w.write_str(target);
            w.write_str(" = ");
            render_expr(value, w);
        }
        Stmt::Expr(expr) => render_expr(expr, w),
        Stmt::Raw(raw) => w.write_str(raw),
    }
}

fn render_expr(expr: &Expr, w: &mut Writer) {
    match expr {
        Expr::Ident(s) => w.write_str(s),
        Expr::StringLit(s) => render_string(s, w),
        Expr::Call { name, args } => render_call(name, args, w),
        Expr::List(items) => render_list(items, w),
        Expr::Dict(items) => render_dict(items, w),
        Expr::Or(lhs, rhs) => {
            render_expr(lhs, w);
            w.write_str(" or ");
            render_expr(rhs, w);
        }
        Expr::Index(base, key) => {
            render_expr(base, w);
            w.write_str("[");
            render_expr(key, w);
            w.write_str("]");
        }
        Expr::Raw(raw) => w.write_str(raw),
    }
}

fn render_call(name: &str, args: &[Arg], w: &mut Writer) {
    w.write_str(name);
    w.write_str("(");
    let multiline = args.len() > 1;
    if multiline {
        w.newline();
    }
    w.indented(|w| {
        for (i, arg) in args.iter().enumerate() {
            if multiline {
                w.write_str("");
            } else if i > 0 {
                w.write_str(", ");
            }
            match arg {
                Arg::Pos(expr) => render_expr(expr, w),
                Arg::Named(name, expr) => {
                    w.write_str(name);
                    w.write_str(" = ");
                    render_expr(expr, w);
                }
            }
            if multiline {
                w.write_str(",");
                w.newline();
            }
        }
    });
    w.write_str(")");
}

fn render_list(items: &[Expr], w: &mut Writer) {
    let multiline = items.len() > 1;
    w.write_str("[");
    if multiline {
        w.newline();
    }
    w.indented(|w| {
        for (i, item) in items.iter().enumerate() {
            if multiline {
                w.write_str("");
            } else if i > 0 {
                w.write_str(", ");
            }
            render_expr(item, w);
            if multiline {
                w.write_str(",");
                w.newline();
            }
        }
    });
    w.write_str("]");
}

fn render_dict(items: &[(Expr, Expr)], w: &mut Writer) {
    let multiline = !items.is_empty();
    w.write_str("{");
    if multiline {
        w.newline();
    }
    w.indented(|w| {
        for (i, (k, v)) in items.iter().enumerate() {
            if multiline {
                w.write_str("");
            } else if i > 0 {
                w.write_str(", ");
            }
            render_expr(k, w);
            w.write_str(": ");
            render_expr(v, w);
            if multiline {
                w.write_str(",");
                w.newline();
            }
        }
    });
    w.write_str("}");
}

fn render_string(value: &str, w: &mut Writer) {
    // Copied/adapted from serde-starlark's string serialization to ensure
    // Starlark-compatible escaping.
    w.write_str("\"");
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        if let Some(escape) = match ch {
            '\x07' => Some('a'), // alert or bell
            '\x08' => Some('b'), // backspace
            '\x0C' => Some('f'), // form feed
            '\n' => Some('n'),   // line feed
            '\r' => Some('r'),   // carriage return
            '\t' => Some('t'),   // horizontal tab
            '\x0B' => Some('v'), // vertical tab
            '"' => Some('"'),
            '\\' => Some('\\'),
            _ => None,
        } {
            w.write_str("\\");
            w.write_str(&escape.to_string());
        } else if ch.is_ascii_control()
            && (ch as u8 >= 0o100 || chars.peek().map_or(true, |next| !next.is_digit(8)))
        {
            // Variable-width octal escapes: \0 through \177. Avoid swallowing the next char.
            w.write_str(&format!("\\{:o}", ch as u8));
        } else if ch.is_control() {
            if ch <= '\x7F' {
                w.write_str(&format!("\\x{:02X}", ch as u8));
            } else if ch <= '\u{FFFF}' {
                w.write_str(&format!("\\u{:04X}", ch as u16));
            } else {
                w.write_str(&format!("\\U{:08X}", ch as u32));
            }
        } else {
            w.write_str(&ch.to_string());
        }
    }
    w.write_str("\"");
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut w = Writer::new();
        render_expr(self, &mut w);
        write!(f, "{}", w.output)
    }
}

impl fmt::Display for Stmt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut w = Writer::new();
        render_stmt(self, &mut w);
        write!(f, "{}", w.output)
    }
}
