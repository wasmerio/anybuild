//! Starlark evaluator for Shipit files.
//!
//! This module provides the evaluate_shipit_file function that reads and
//! executes a Shipit file, returning the accumulated context and serve
//! configuration.

use crate::starlark::config::ShipitConfig;
use crate::starlark::ctx::{Ctx, Serve};
use crate::starlark::functions::shipit_functions;
use anyhow::{anyhow, Context, Result};
use starlark::environment::{GlobalsBuilder, Module};
use starlark::eval::Evaluator;
use starlark::syntax::{AstModule, Dialect};
use std::cell::RefCell;
use std::fs;
use std::path::Path;
use std::rc::Rc;

// Thread-local storage for Ctx during Starlark evaluation.
//
// Since Starlark functions can't easily pass mutable references around,
// we use thread-local storage to give functions access to the Ctx.
thread_local! {
    static CTX: RefCell<Option<Rc<RefCell<Ctx>>>> = const { RefCell::new(None) };
}

/// Set the context for the current thread.
pub fn set_ctx(ctx: Rc<RefCell<Ctx>>) {
    CTX.with(|c| {
        *c.borrow_mut() = Some(ctx);
    });
}

/// Get the context for the current thread.
pub fn with_ctx<F, R>(f: F) -> Result<R>
where
    F: FnOnce(&mut Ctx) -> Result<R>,
{
    CTX.with(|c| {
        let ctx_opt = c.borrow();
        match &*ctx_opt {
            Some(ctx_rc) => f(&mut ctx_rc.borrow_mut()),
            None => Err(anyhow!("No context available")),
        }
    })
}

/// Clear the context for the current thread.
fn clear_ctx() {
    CTX.with(|c| {
        *c.borrow_mut() = None;
    });
}

/// Evaluate a Shipit file and return the context and serve configuration.
///
/// This function:
/// 1. Reads the Shipit file from disk
/// 2. Creates a Starlark module with Shipit functions
/// 3. Injects `config` and `PORT` globals
/// 4. Evaluates the file
/// 5. Extracts the serve configuration result
/// 6. Returns both the accumulated context and serve config
///
/// The `provider_config` is exposed as the `config` variable inside
/// the Starlark file, allowing Shipit scripts to reference values
/// like `config.php_version`, `config.python_version`, etc.
pub fn evaluate_shipit_file(
    path: &Path,
    provider_config: ShipitConfig,
) -> Result<(Ctx, Serve)> {
    // Read the Shipit file
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read Shipit file: {:?}", path))?;

    // Parse the Starlark code — use a dialect that supports
    // keyword-only arguments and f-strings, matching the Python
    // evaluator behaviour.
    let dialect = Dialect {
        enable_keyword_only_arguments: true,
        enable_f_strings: true,
        ..Dialect::Standard
    };
    let ast = AstModule::parse(&path.display().to_string(), content, &dialect)
        .map_err(|e| anyhow!("Failed to parse Shipit file: {}", e))?;

    // Create the module to store globals
    let module = Module::new();

    // Inject the `config` variable so Shipit files can read
    // provider-detected values (versions, paths, flags, etc.).
    let config_val = module.heap().alloc(provider_config);
    module.set("config", config_val);

    // Inject the `PORT` variable (default 8080, matching the Python
    // evaluator).
    let port_val = module
        .heap()
        .alloc(std::env::var("PORT").unwrap_or_else(|_| "8080".to_string()));
    module.set("PORT", port_val);

    // Create a context to accumulate state
    let ctx = Rc::new(RefCell::new(Ctx::new()));

    // Set the context for this thread
    set_ctx(ctx.clone());

    // Build globals with Shipit functions
    let globals = GlobalsBuilder::standard().with(shipit_functions).build();

    // Create evaluator
    let mut eval = Evaluator::new(&module);

    // Evaluate the module
    let result = eval.eval_module(ast, &globals).map_err(|e| {
        clear_ctx();
        anyhow!("Failed to evaluate Shipit file: {}", e)
    })?;

    // Extract serve reference from result
    let serve_ref = result.unpack_str().ok_or_else(|| {
        clear_ctx();
        anyhow!("Shipit file must return a serve configuration")
    })?;

    // Get the final context
    let final_ctx = ctx.borrow().clone();

    // Extract serve name from reference (ref:serve:name -> name)
    let serve_name = serve_ref
        .strip_prefix("ref:serve:")
        .ok_or_else(|| anyhow!("Invalid serve reference format: {}", serve_ref))?;

    // Resolve the serve_ref to get the actual Serve object
    let serve = final_ctx
        .serves
        .get(serve_name)
        .ok_or_else(|| anyhow!("Serve not found: {}", serve_name))?
        .clone();

    // Clear the thread-local context
    clear_ctx();

    Ok((final_ctx, serve))
}

/// Evaluate Starlark code directly (for testing).
pub fn evaluate_code(code: &str) -> Result<(Ctx, String)> {
    evaluate_code_with_config(code, ShipitConfig::new())
}

/// Evaluate Starlark code with a specific config (for testing).
pub fn evaluate_code_with_config(
    code: &str,
    provider_config: ShipitConfig,
) -> Result<(Ctx, String)> {
    let dialect = Dialect {
        enable_keyword_only_arguments: true,
        enable_f_strings: true,
        ..Dialect::Standard
    };
    let ast = AstModule::parse("test.star", code.to_string(), &dialect)
        .map_err(|e| anyhow!("Failed to parse code: {}", e))?;

    let module = Module::new();

    // Inject config and PORT
    let config_val = module.heap().alloc(provider_config);
    module.set("config", config_val);
    let port_val = module.heap().alloc("8080");
    module.set("PORT", port_val);

    let ctx = Rc::new(RefCell::new(Ctx::new()));

    // Set the context for this thread
    set_ctx(ctx.clone());

    let globals = GlobalsBuilder::standard().with(shipit_functions).build();

    let mut eval = Evaluator::new(&module);
    let result = eval.eval_module(ast, &globals).map_err(|e| {
        clear_ctx();
        anyhow!("Failed to evaluate code: {}", e)
    })?;

    let result_str = result
        .unpack_str()
        .map(|s| s.to_string())
        .unwrap_or_else(|| "None".to_string());

    // Get the final context
    let final_ctx = ctx.borrow().clone();

    // Clear the thread-local context
    clear_ctx();

    Ok((final_ctx, result_str))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_evaluate_simple_code() {
        let code = r#"
x = 1 + 2
"x = 3"
"#;

        let result = evaluate_code(code);
        assert!(result.is_ok());
        let (_, output) = result.unwrap();
        assert_eq!(output, "x = 3");
    }

    #[test]
    fn test_evaluate_with_dep() {
        let code = r#"
dep("python", "3.11")
"#;

        let result = evaluate_code(code);
        assert!(result.is_ok());
    }

    #[test]
    fn test_evaluate_shipit_file() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"
# Simple Shipit file
python = dep("python", "3.11")
serve("app", "python", [], [python], {{"web": "python app.py"}})
"#
        )
        .unwrap();

        let result = evaluate_shipit_file(
            file.path(),
            ShipitConfig::new(),
        );
        if let Err(e) = &result {
            eprintln!("Error: {}", e);
        }
        assert!(result.is_ok());
    }

    #[test]
    fn test_evaluate_invalid_syntax() {
        let code = "this is not valid starlark +++";
        let result = evaluate_code(code);
        assert!(result.is_err());
    }

    #[test]
    fn test_run_with_lists() {
        let code = r#"
run("npm ci", inputs=["package.json", "package-lock.json"], outputs=["node_modules"])
"#;
        let result = evaluate_code(code);
        assert!(result.is_ok());

        let (ctx, _) = result.unwrap();
        assert_eq!(ctx.steps.len(), 1);

        if let crate::types::Step::Run(step) = &ctx.steps[0] {
            assert_eq!(step.command, "npm ci");
            assert_eq!(
                step.inputs,
                Some(vec![
                    "package.json".to_string(),
                    "package-lock.json".to_string()
                ])
            );
            assert_eq!(step.outputs, Some(vec!["node_modules".to_string()]));
        } else {
            panic!("Expected RunStep");
        }
    }

    #[test]
    fn test_copy_with_ignore() {
        let code = r#"
copy("src", "dist", ignore=["*.log", "*.tmp", ".git"])
"#;
        let result = evaluate_code(code);
        assert!(result.is_ok());

        let (ctx, _) = result.unwrap();
        assert_eq!(ctx.steps.len(), 1);

        if let crate::types::Step::Copy(step) = &ctx.steps[0] {
            assert_eq!(step.source, "src");
            assert_eq!(step.target, "dist");
            assert_eq!(
                step.ignore,
                Some(vec![
                    "*.log".to_string(),
                    "*.tmp".to_string(),
                    ".git".to_string()
                ])
            );
        } else {
            panic!("Expected CopyStep");
        }
    }

    #[test]
    fn test_env_with_dict() {
        let code = r#"
env({"FOO": "bar", "BAZ": "qux", "NODE_ENV": "production"})
"#;
        let result = evaluate_code(code);
        if let Err(e) = &result {
            eprintln!("Error: {}", e);
        }
        assert!(result.is_ok());

        let (ctx, _) = result.unwrap();
        assert_eq!(ctx.steps.len(), 1);

        if let crate::types::Step::Env(step) = &ctx.steps[0] {
            assert_eq!(step.variables.get("FOO"), Some(&"bar".to_string()));
            assert_eq!(step.variables.get("BAZ"), Some(&"qux".to_string()));
            assert_eq!(
                step.variables.get("NODE_ENV"),
                Some(&"production".to_string())
            );
        } else {
            panic!("Expected EnvStep");
        }
    }

    #[test]
    fn test_use_with_deps() {
        let code = r#"
python = dep("python", "3.11")
node = dep("node", "20")
use([python, node])
"#;
        let result = evaluate_code(code);
        if let Err(e) = &result {
            eprintln!("Error: {}", e);
        }
        assert!(result.is_ok());

        let (ctx, _) = result.unwrap();
        assert_eq!(ctx.packages.len(), 2);
        assert_eq!(ctx.steps.len(), 1);

        if let crate::types::Step::Use(step) = &ctx.steps[0] {
            assert_eq!(step.dependencies.len(), 2);
            // Check that we have reference strings
            assert!(step.dependencies[0].starts_with("ref:package:"));
            assert!(step.dependencies[1].starts_with("ref:package:"));
        } else {
            panic!("Expected UseStep");
        }
    }

    #[test]
    fn test_serve_with_all_parameters() {
        let code = r#"
python = dep("python", "3.11")
db = service("db", "postgres")
temp = mount("temp")
data = volume("data", "/app/data")

build_step = run("pip install -r requirements.txt")
prepare_step = workdir("/app")

serve(
    "myapp",
    "python",
    [build_step],
    [python],
    {"web": "python app.py", "worker": "python worker.py"},
    prepare=[prepare_step],
    workers=["worker"],
    mounts=[temp],
    volumes=[data],
    env={"DEBUG": "1"},
    services=[db]
)
"#;
        let result = evaluate_shipit_file_from_code(code);
        assert!(result.is_ok());

        let (ctx, serve) = result.unwrap();

        // Verify context accumulated everything
        assert_eq!(ctx.borrow().packages.len(), 1);
        assert_eq!(ctx.borrow().services.len(), 1);
        assert_eq!(ctx.borrow().mounts.len(), 1);
        assert_eq!(ctx.borrow().volumes.len(), 1);
        assert_eq!(ctx.borrow().steps.len(), 2);

        // Verify serve configuration
        assert_eq!(serve.name, "myapp");
        assert_eq!(serve.provider, "python");
        assert_eq!(serve.build.len(), 1);
        assert_eq!(serve.deps.len(), 1);
        assert_eq!(serve.commands.len(), 2);
        assert_eq!(
            serve.commands.get("web"),
            Some(&"python app.py".to_string())
        );
        assert_eq!(serve.prepare.len(), 1);
        assert_eq!(serve.workers.len(), 1);
        assert_eq!(serve.mounts.len(), 1);
        assert_eq!(serve.volumes.len(), 1);
        assert_eq!(serve.env.get("DEBUG"), Some(&"1".to_string()));
        assert_eq!(serve.services.len(), 1);
    }

    /// Helper function to evaluate code and return (Ctx, Serve)
    fn evaluate_shipit_file_from_code(code: &str) -> Result<(Rc<RefCell<Ctx>>, Serve)> {
        let ctx = Rc::new(RefCell::new(Ctx::new()));
        set_ctx(ctx.clone());

        // Create a Starlark module and evaluator
        let globals = GlobalsBuilder::new().with(shipit_functions).build();
        let module = Module::new();

        // Inject config and PORT for tests
        let config_val =
            module.heap().alloc(ShipitConfig::new());
        module.set("config", config_val);
        let port_val = module.heap().alloc("8080");
        module.set("PORT", port_val);

        let mut eval = Evaluator::new(&module);

        // Parse and execute the code
        let dialect = Dialect {
            enable_keyword_only_arguments: true,
            enable_f_strings: true,
            ..Dialect::Standard
        };
        let ast = AstModule::parse("test.star", code.to_owned(), &dialect)
            .map_err(|e| anyhow!("Failed to parse Starlark code: {}", e))?;
        let result_value = eval
            .eval_module(ast, &globals)
            .map_err(|e| anyhow!("Failed to evaluate Starlark code: {}", e))?;

        // Extract serve reference from result
        let serve_ref = result_value
            .unpack_str()
            .ok_or_else(|| anyhow!("Serve configuration not returned"))?;

        // Strip prefix and resolve serve
        let serve_name = serve_ref
            .strip_prefix("ref:serve:")
            .ok_or_else(|| anyhow!("Invalid serve reference: {}", serve_ref))?;

        let serve = ctx
            .borrow()
            .serves
            .get(serve_name)
            .cloned()
            .ok_or_else(|| anyhow!("Serve reference not found: {}", serve_name))?;

        clear_ctx();
        Ok((ctx, serve))
    }

    #[test]
    fn test_mount_field_access() {
        let code = r#"
temp = mount("temp")
# Access mount fields
name = temp.name
path = temp.path
build = temp.build_path
srv = temp.serve_path

# Verify we can use mount in serve
serve("app", "test", [], [], {"start": "echo"}, mounts=[temp])
"#;
        let result = evaluate_shipit_file_from_code(code);
        if let Err(e) = &result {
            eprintln!("Error: {}", e);
        }
        assert!(result.is_ok());

        let (_ctx, serve_config) = result.unwrap();
        assert_eq!(serve_config.mounts.len(), 1);
        assert!(serve_config.mounts[0].starts_with("ref:mount:"));
    }

    #[test]
    fn test_volume_field_access() {
        let code = r#"
data = volume("data", "/app/data")
# Access volume fields
name = data.name
path = data.serve_path

# Verify we can use volume in serve
serve("app", "test", [], [], {"start": "echo"}, volumes=[data])
"#;
        let result = evaluate_shipit_file_from_code(code);
        assert!(result.is_ok());

        let (_ctx, serve) = result.unwrap();
        assert_eq!(serve.volumes.len(), 1);
        assert!(serve.volumes[0].starts_with("ref:volume:"));
    }
}
