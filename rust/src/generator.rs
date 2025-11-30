//! Shipit file generation using a minimal Starlark AST.

use std::collections::{BTreeMap, HashSet};

use crate::Result;
use crate::model::{ProviderPlan, ServiceProvider};
use crate::starlark_ast::{Arg, Expr, Stmt, render_module};

/// Options for Shipit generation.
pub struct GeneratorOptions {
    /// Path to the source project, used for deriving defaults.
    pub project_name: String,
}

impl Default for GeneratorOptions {
    fn default() -> Self {
        Self {
            project_name: "app".to_string(),
        }
    }
}

/// Generator entrypoint.
pub struct ShipitGenerator {
    pub options: GeneratorOptions,
}

impl ShipitGenerator {
    pub fn new(options: GeneratorOptions) -> Self {
        Self { options }
    }

    /// Render a Shipit file for the given provider plan.
    pub fn generate(&self, plan: &ProviderPlan) -> Result<String> {
        let mut stmts: Vec<Stmt> = Vec::new();

        // Dependencies (version/arch + dep call).
        for dep in &plan.dependencies {
            let alias = dep
                .alias
                .clone()
                .unwrap_or_else(|| sanitize_alias(&dep.name));
            if let Some(env_var) = &dep.env_var {
                let mut value =
                    Expr::call("getenv", vec![Arg::Pos(Expr::StringLit(env_var.clone()))]);
                if let Some(default) = &dep.default_version {
                    value = Expr::Or(Box::new(value), Box::new(Expr::StringLit(default.clone())));
                }
                stmts.push(Stmt::assignment(format!("{alias}_version"), value));
            }
            if let Some(arch_var) = &dep.architecture_var {
                let expr = Expr::call("getenv", vec![Arg::Pos(Expr::StringLit(arch_var.clone()))]);
                stmts.push(Stmt::assignment(format!("{alias}_architecture"), expr));
            }
            let dep_args = DepCall {
                name: dep.name.clone(),
                version: dep
                    .env_var
                    .as_ref()
                    .map(|_| Expr::Ident(format!("{alias}_version"))),
                architecture: dep
                    .architecture_var
                    .as_ref()
                    .map(|_| Expr::Ident(format!("{alias}_architecture"))),
            };
            stmts.push(Stmt::assignment(alias, dep_args.into_expr()));
        }

        // Mounts
        for mount in &plan.mounts {
            let mount_call =
                Expr::call("mount", vec![Arg::Pos(Expr::StringLit(mount.name.clone()))]);
            stmts.push(Stmt::assignment(&mount.name, mount_call));
        }

        // Volumes
        for volume in &plan.volumes {
            let name = volume
                .var_name
                .clone()
                .unwrap_or_else(|| volume.name.clone());
            let call = Expr::call(
                "volume",
                vec![
                    Arg::Pos(Expr::StringLit(volume.name.clone())),
                    Arg::Pos(Expr::Raw(volume.serve_path.clone())),
                ],
            );
            stmts.push(Stmt::assignment(name, call));
        }

        // Services
        for svc in &plan.services {
            let call = Expr::call(
                "service",
                vec![
                    Arg::Named("name".to_string(), Expr::StringLit(svc.name.clone())),
                    Arg::Named(
                        "provider".to_string(),
                        Expr::StringLit(
                            match svc.provider {
                                ServiceProvider::Postgres => "postgres",
                                ServiceProvider::Mysql => "mysql",
                                ServiceProvider::Redis => "redis",
                            }
                            .to_string(),
                        ),
                    ),
                ],
            );
            stmts.push(Stmt::assignment(&svc.name, call));
        }

        // PORT helper (simple getenv without default; builder start script still sets default).
        stmts.push(Stmt::assignment(
            "PORT",
            Expr::Or(
                Box::new(Expr::call(
                    "getenv",
                    vec![Arg::Pos(Expr::StringLit("PORT".to_string()))],
                )),
                Box::new(Expr::StringLit("8080".to_string())),
            ),
        ));

        if let Some(extra) = &plan.declarations {
            if !extra.is_empty() {
                stmts.push(Stmt::Raw(extra.clone()));
            }
        }

        // Build steps (attempt to convert provider plan strings into typed AST; fall back to raw).
        let mut build_steps = build_steps_from_plan(plan);
        if build_steps.is_empty() {
            build_steps.push(Expr::call(
                "workdir",
                vec![Arg::Pos(Expr::StringLit("app".to_string()))],
            ));
            build_steps.push(Expr::call(
                "copy",
                vec![
                    Arg::Pos(Expr::StringLit(detect_root(plan))),
                    Arg::Named("target".to_string(), Expr::StringLit(".".to_string())),
                    Arg::Named(
                        "ignore".to_string(),
                        Expr::List(vec![Expr::StringLit(".git".to_string())]),
                    ),
                ],
            ));
        }

        let mut build_dep_vars: Vec<String> = Vec::new();
        let mut build_seen: HashSet<String> = HashSet::new();
        for dep in &plan.dependencies {
            if !dep.use_in_build {
                continue;
            }
            let alias = dep
                .alias
                .clone()
                .unwrap_or_else(|| sanitize_alias(&dep.name));
            if build_seen.insert(alias.clone()) {
                build_dep_vars.push(alias);
            }
        }
        let has_use_step = plan.build_steps.iter().any(|s| s.contains("use("));
        if !build_dep_vars.is_empty() && !has_use_step {
            let args = build_dep_vars
                .iter()
                .map(|d| Arg::Pos(Expr::Ident(d.clone())))
                .collect();
            build_steps.insert(0, Expr::call("use", args));
        }

        let mut serve_deps: Vec<String> = Vec::new();
        let mut serve_seen: HashSet<String> = HashSet::new();
        for dep in &plan.dependencies {
            if !dep.use_in_serve {
                continue;
            }
            let alias = dep
                .alias
                .clone()
                .unwrap_or_else(|| sanitize_alias(&dep.name));
            if serve_seen.insert(alias.clone()) {
                serve_deps.push(alias);
            }
        }

        let prepare_steps = plan.prepare.as_ref().map(|steps| {
            steps
                .iter()
                .map(|s| Expr::Raw(s.clone()))
                .collect::<Vec<_>>()
        });

        let env_map = plan.env.as_ref().map(|env| {
            env.iter()
                .map(|(k, v)| (Expr::StringLit(k.clone()), Expr::Raw(v.clone())))
                .collect::<Vec<_>>()
        });

        let commands = serve_commands(plan);
        let command_map = commands
            .into_iter()
            .map(|(k, v)| (Expr::StringLit(k), v))
            .collect::<Vec<_>>();

        let mount_names: Vec<String> = plan
            .mounts
            .iter()
            .filter(|m| m.attach_to_serve)
            .map(|m| m.name.clone())
            .collect();
        let cwd_expr = mount_names
            .iter()
            .find(|m| m.as_str() == "app")
            .map(|_| Expr::Raw("app[\"serve\"]".to_string()));

        let volume_names: Vec<String> = plan
            .volumes
            .iter()
            .map(|v| v.var_name.clone().unwrap_or_else(|| v.name.clone()))
            .collect();

        let service_refs: Vec<String> = plan.services.iter().map(|s| s.name.clone()).collect();

        // serve(...) expression
        let mut serve_args: Vec<Arg> = Vec::new();
        serve_args.push(Arg::Named(
            "name".to_string(),
            Expr::StringLit(plan.serve_name.clone()),
        ));
        serve_args.push(Arg::Named(
            "provider".to_string(),
            Expr::StringLit(plan.provider.clone()),
        ));
        if let Some(cwd_expr) = cwd_expr {
            serve_args.push(Arg::Named("cwd".to_string(), cwd_expr));
        } else if let Some(cwd) = &plan.cwd {
            serve_args.push(Arg::Named("cwd".to_string(), Expr::Raw(cwd.clone())));
        }
        serve_args.push(Arg::Named("build".to_string(), Expr::List(build_steps)));
        serve_args.push(Arg::Named(
            "deps".to_string(),
            Expr::List(serve_deps.iter().map(|d| Expr::Ident(d.clone())).collect()),
        ));
        if let Some(prepare) = prepare_steps {
            serve_args.push(Arg::Named("prepare".to_string(), Expr::List(prepare)));
        }
        if let Some(env) = env_map {
            serve_args.push(Arg::Named("env".to_string(), Expr::Dict(env)));
        }
        serve_args.push(Arg::Named("commands".to_string(), Expr::Dict(command_map)));
        if !mount_names.is_empty() {
            serve_args.push(Arg::Named(
                "mounts".to_string(),
                Expr::List(mount_names.iter().map(|m| Expr::Ident(m.clone())).collect()),
            ));
        }
        if !volume_names.is_empty() {
            serve_args.push(Arg::Named(
                "volumes".to_string(),
                Expr::List(
                    volume_names
                        .iter()
                        .map(|v| Expr::Ident(v.clone()))
                        .collect(),
                ),
            ));
        }
        if !service_refs.is_empty() {
            serve_args.push(Arg::Named(
                "services".to_string(),
                Expr::List(
                    service_refs
                        .iter()
                        .map(|s| Expr::Ident(s.clone()))
                        .collect(),
                ),
            ));
        }

        stmts.push(Stmt::Expr(Expr::call("serve", serve_args)));

        Ok(render_module(&stmts))
    }
}

fn detect_root(plan: &ProviderPlan) -> String {
    for step in &plan.build_steps {
        if let Some(rest) = step.strip_prefix("copy(") {
            if let Some(first) = rest.split(',').next() {
                if let Ok(src) = serde_json::from_str::<String>(first.trim()) {
                    return src;
                }
            }
        }
    }
    ".".to_string()
}

fn serve_commands(plan: &ProviderPlan) -> BTreeMap<String, Expr> {
    plan.commands
        .iter()
        .map(|(k, v)| {
            (
                k.clone(),
                Expr::Raw(format!(r#"{}.replace("$PORT", PORT)"#, v)),
            )
        })
        .collect()
}

fn sanitize_alias(name: &str) -> String {
    let mut alias = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            alias.push(c);
        }
    }
    alias.replace('-', "")
}

#[derive(Clone)]
struct DepCall {
    name: String,
    version: Option<Expr>,
    architecture: Option<Expr>,
}

impl DepCall {
    fn into_expr(self) -> Expr {
        let mut args = vec![Arg::Pos(Expr::StringLit(self.name))];
        if let Some(v) = self.version {
            args.push(Arg::Pos(v));
        }
        if let Some(arch) = self.architecture {
            args.push(Arg::Named("architecture".to_string(), arch));
        }
        Expr::call("dep", args)
    }
}

fn build_steps_from_plan(plan: &ProviderPlan) -> Vec<Expr> {
    let mut steps = Vec::new();
    for raw in &plan.build_steps {
        if raw.starts_with("workdir(") && raw.ends_with(')') {
            let inner = raw.trim_start_matches("workdir(").trim_end_matches(')');
            let arg = if let Ok(path) = serde_json::from_str::<String>(inner.trim()) {
                Expr::StringLit(path)
            } else {
                Expr::Raw(inner.trim().to_string())
            };
            steps.push(Expr::call("workdir", vec![Arg::Pos(arg)]));
            continue;
        }
        if raw.starts_with("copy(") && raw.ends_with(')') {
            let inner = raw.trim_start_matches("copy(").trim_end_matches(')');
            let mut args = Vec::new();
            let mut parts = inner.split(',').map(str::trim);
            if let Some(src_part) = parts.next() {
                let src = serde_json::from_str::<String>(src_part)
                    .unwrap_or_else(|_| src_part.to_string());
                args.push(Arg::Pos(Expr::StringLit(src)));
            }
            if let Some(target_part) = parts.next() {
                if !target_part.is_empty() && !target_part.starts_with("ignore=") {
                    let tgt = serde_json::from_str::<String>(target_part)
                        .unwrap_or_else(|_| target_part.to_string());
                    args.push(Arg::Named("target".to_string(), Expr::StringLit(tgt)));
                }
            }
            // naive parse for ignore list
            if let Some(ignore_pos) = inner.find("ignore=") {
                let ignore_str = inner[ignore_pos + "ignore=".len()..].trim();
                if ignore_str.starts_with('[') && ignore_str.ends_with(']') {
                    let trimmed = &ignore_str[1..ignore_str.len() - 1];
                    let mut ignores = Vec::new();
                    for item in trimmed.split(',').map(str::trim) {
                        if item.is_empty() {
                            continue;
                        }
                        let val = serde_json::from_str::<String>(item)
                            .unwrap_or_else(|_| item.to_string());
                        ignores.push(Expr::StringLit(val));
                    }
                    if !ignores.is_empty() {
                        args.push(Arg::Named("ignore".to_string(), Expr::List(ignores)));
                    }
                }
            }
            steps.push(Expr::call("copy", args));
            continue;
        }
        steps.push(Expr::Raw(raw.clone()));
    }
    steps
}
