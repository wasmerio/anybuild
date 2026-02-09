//! Emission of Starlark code for Shipit files.
//!
//! This module contains functions for converting provider plans into
//! properly formatted Starlark code.

use crate::providers::specs::{DependencySpec, ProviderPlan};
use crate::types::Service;
use anyhow::Result;
use std::collections::{HashMap, HashSet};

/// Sanitize a name to be a valid Starlark variable.
fn sanitize_var_name(name: &str) -> String {
    name.replace(['-', '.', '/', '@'], "_")
}

/// Generate dependency declarations.
///
/// Returns:
/// - Starlark code for dependency declarations
/// - List of variable names used in serve
/// - List of variable names used in build
fn emit_dependencies(deps: &[DependencySpec]) -> (String, Vec<String>, Vec<String>) {
    let mut code = String::new();
    let mut serve_vars = Vec::new();
    let mut build_vars = Vec::new();
    let mut seen = HashSet::new();

    for dep in deps {
        // Generate variable name
        let var_name = dep
            .alias
            .as_ref()
            .map(|a| sanitize_var_name(a))
            .unwrap_or_else(|| sanitize_var_name(&dep.name));

        // Skip duplicates
        if !seen.insert(var_name.clone()) {
            continue;
        }

        // Track usage
        if dep.use_in_serve {
            serve_vars.push(var_name.clone());
        }
        if dep.use_in_build {
            build_vars.push(var_name.clone());
        }

        // Generate dep() call
        let mut args = vec![format!(r#""{}""#, dep.name)];

        if let Some(version) = &dep.default_version {
            args.push(format!(r#""{}""#, version));
        }

        if let Some(arch) = &dep.architecture_var_name {
            args.push(format!("architecture={}", arch));
        }

        code.push_str(&format!("{} = dep({})\n", var_name, args.join(", ")));
    }

    (code, serve_vars, build_vars)
}

/// Generate build steps code.
fn emit_build_steps(steps: &[String]) -> String {
    if steps.is_empty() {
        return String::new();
    }

    let mut code = String::from("build = [\n");

    for step in steps {
        code.push_str(&format!("    {},\n", step));
    }

    code.push_str("]\n");
    code
}

/// Generate mount declarations.
fn emit_mounts(mount_names: &[String]) -> String {
    let mut code = String::new();

    for name in mount_names {
        let var_name = sanitize_var_name(name);
        code.push_str(&format!("{} = mount(\"{}\")\n", var_name, name));
    }

    code
}

/// Generate volume declarations.
fn emit_volumes(volumes: &[(String, String)]) -> String {
    let mut code = String::new();

    for (name, serve_path) in volumes {
        let var_name = sanitize_var_name(name);
        code.push_str(&format!(
            r#"{} = volume("{}", "{}")"#,
            var_name, name, serve_path
        ));
        code.push('\n');
    }

    code
}

/// Generate service declarations.
fn emit_services(services: &[Service]) -> String {
    let mut code = String::new();

    for svc in services {
        let var_name = sanitize_var_name(&svc.name);
        let provider = match svc.provider {
            crate::types::ServiceProvider::Postgres => "postgres",
            crate::types::ServiceProvider::Mysql => "mysql",
            crate::types::ServiceProvider::Redis => "redis",
        };
        code.push_str(&format!(
            r#"{} = service("{}", "{}")"#,
            var_name, svc.name, provider
        ));
        code.push('\n');
    }

    code
}

/// Parameters for generating a serve() function call
struct ServeCallParams<'a> {
    serve_name: &'a str,
    provider: &'a str,
    commands: &'a HashMap<String, String>,
    serve_vars: &'a [String],
    mounts: &'a [String],
    volumes: &'a [String],
    services: &'a [String],
    env_vars: &'a HashMap<String, String>,
    prepare: &'a [String],
}

/// Generate serve() function call.
fn emit_serve_call(params: ServeCallParams) -> String {
    let mut code = String::from("serve(\n");

    // Name and provider
    code.push_str(&format!(r#"    "{}","#, params.serve_name));
    code.push('\n');
    code.push_str(&format!(r#"    "{}","#, params.provider));
    code.push('\n');

    // Build steps
    code.push_str("    build,\n");

    // Dependencies
    if params.serve_vars.is_empty() {
        code.push_str("    [],\n");
    } else {
        code.push_str(&format!("    [{}],\n", params.serve_vars.join(", ")));
    }

    // Commands
    code.push_str("    {\n");
    for (name, cmd) in params.commands {
        code.push_str(&format!(r#"        "{}": "{}","#, name, cmd));
        code.push('\n');
    }
    code.push_str("    },\n");

    // Optional parameters
    if !params.prepare.is_empty() {
        code.push_str(&format!("    prepare=[{}],\n", params.prepare.join(", ")));
    }

    if !params.mounts.is_empty() {
        code.push_str(&format!("    mounts=[{}],\n", params.mounts.join(", ")));
    }

    if !params.volumes.is_empty() {
        code.push_str(&format!("    volumes=[{}],\n", params.volumes.join(", ")));
    }

    if !params.services.is_empty() {
        code.push_str(&format!("    services=[{}],\n", params.services.join(", ")));
    }

    if !params.env_vars.is_empty() {
        code.push_str("    env={\n");
        for (k, v) in params.env_vars {
            code.push_str(&format!(r#"        "{}": "{}","#, k, v));
            code.push('\n');
        }
        code.push_str("    },\n");
    }

    code.push_str(")\n");
    code
}

/// Generate a complete Shipit file from a provider plan.
pub fn generate_shipit_file(_path: &std::path::Path, plan: &ProviderPlan) -> Result<String> {
    let mut output = String::new();

    // Header comment
    output.push_str(&format!(
        "# Generated Shipit file\n# Provider: {}\n\n",
        plan.provider
    ));

    // Declarations (if any)
    if let Some(decl) = &plan.declarations {
        output.push_str(decl);
        output.push_str("\n\n");
    }

    // Dependencies
    let (deps_code, serve_vars, _build_vars) = emit_dependencies(&plan.dependencies);
    if !deps_code.is_empty() {
        output.push_str(&deps_code);
        output.push('\n');
    }

    // Services
    if !plan.services.is_empty() {
        let services_code = emit_services(&plan.services);
        output.push_str(&services_code);
        output.push('\n');
    }

    // Mounts
    let mount_names: Vec<String> = plan.mounts.iter().map(|m| m.name.clone()).collect();
    if !mount_names.is_empty() {
        let mounts_code = emit_mounts(&mount_names);
        output.push_str(&mounts_code);
        output.push('\n');
    }

    // Volumes
    let volume_specs: Vec<(String, String)> = plan
        .volumes
        .iter()
        .map(|v| (v.name.clone(), v.serve_path.display().to_string()))
        .collect();
    if !volume_specs.is_empty() {
        let volumes_code = emit_volumes(&volume_specs);
        output.push_str(&volumes_code);
        output.push('\n');
    }

    // Build steps
    if !plan.build_steps.is_empty() {
        let build_code = emit_build_steps(&plan.build_steps);
        output.push_str(&build_code);
        output.push('\n');
    }

    // Serve call
    let mount_vars: Vec<String> = mount_names.iter().map(|n| sanitize_var_name(n)).collect();

    let volume_vars: Vec<String> = volume_specs
        .iter()
        .map(|(n, _)| sanitize_var_name(n))
        .collect();

    let service_vars: Vec<String> = plan
        .services
        .iter()
        .map(|s| sanitize_var_name(&s.name))
        .collect();

    let prepare = plan.prepare.clone().unwrap_or_default();

    let env_vars = plan.env.as_ref().cloned().unwrap_or_default();

    let serve_code = emit_serve_call(ServeCallParams {
        serve_name: &plan.serve_name,
        provider: &plan.provider,
        commands: &plan.commands,
        serve_vars: &serve_vars,
        mounts: &mount_vars,
        volumes: &volume_vars,
        services: &service_vars,
        env_vars: &env_vars,
        prepare: &prepare,
    });

    output.push_str(&serve_code);

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::specs::DependencySpec;

    #[test]
    fn test_sanitize_var_name() {
        assert_eq!(sanitize_var_name("my-package"), "my_package");
        assert_eq!(sanitize_var_name("node.js"), "node_js");
        assert_eq!(sanitize_var_name("python@3.11"), "python_3_11");
    }

    #[test]
    fn test_emit_dependencies() {
        let mut dep = DependencySpec::new("python");
        dep.default_version = Some("3.11".to_string());
        dep.use_in_serve = true;

        let (code, serve_vars, _) = emit_dependencies(&[dep]);

        assert!(code.contains("python = dep("));
        assert!(code.contains(r#""python""#));
        assert!(code.contains(r#""3.11""#));
        assert_eq!(serve_vars, vec!["python"]);
    }

    #[test]
    fn test_emit_build_steps() {
        let steps = vec![
            r#"run("npm install")"#.to_string(),
            r#"run("npm run build")"#.to_string(),
        ];

        let code = emit_build_steps(&steps);

        assert!(code.contains("build = ["));
        assert!(code.contains(r#"run("npm install")"#));
        assert!(code.contains(r#"run("npm run build")"#));
    }

    #[test]
    fn test_generate_simple_shipit() {
        let mut plan = ProviderPlan::new("app", "test");

        let mut dep = DependencySpec::new("python");
        dep.default_version = Some("3.11".to_string());
        dep.use_in_serve = true;
        plan.dependencies.push(dep);

        plan.build_steps.push(r#"run("echo hello")"#.to_string());
        plan.commands
            .insert("web".to_string(), "python app.py".to_string());

        let result = generate_shipit_file(std::path::Path::new("/tmp"), &plan);

        assert!(result.is_ok());
        let code = result.unwrap();

        assert!(code.contains("# Provider: test"));
        assert!(code.contains("python = dep"));
        assert!(code.contains("build = ["));
        assert!(code.contains("serve("));
    }
}
