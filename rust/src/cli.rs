//! CLI entrypoints mirroring the Python tool (`auto`, `generate`, `plan`,
//! `build`, `serve`).

use camino::Utf8PathBuf;
use clap::{Parser, Subcommand};

use crate::builder::Builder;
use crate::detect::detect_registered_provider;
use crate::env::load_env;
use crate::generator::{GeneratorOptions, ShipitGenerator};
use crate::model::{CustomCommands, Step};
use crate::provider::registry;
use crate::starlark_runtime::evaluate_shipit;
use crate::Result;

#[derive(Parser, Debug)]
#[command(name = "shipit", version, about = "Shipit Rust CLI (WIP)")]
pub struct Cli {
    /// Project path (defaults to current directory).
    #[arg(value_name = "PATH", default_value = ".")]
    pub path: Utf8PathBuf,
    /// Flags passed without a subcommand map to `auto`.
    #[command(flatten)]
    pub auto_args: AutoArgs,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Auto mode: regenerate Shipit, build, optionally serve.
    Auto(AutoArgs),
    /// Generate Shipit file.
    Generate(GenerateArgs),
    /// Evaluate Shipit and output plan as JSON.
    Plan(PlanArgs),
    /// Build project defined by Shipit file.
    Build(BuildArgs),
    /// Run serve command (after build).
    Serve(ServeArgs),
}

#[derive(Parser, Debug, Default, Clone)]
pub struct AutoArgs {
    /// Use Wasmer backend.
    #[arg(long)]
    pub wasmer: bool,
    /// Use Docker backend.
    #[arg(long)]
    pub docker: bool,
    /// Skip prepare steps.
    #[arg(long)]
    pub skip_prepare: bool,
    /// Run start command after build.
    #[arg(long)]
    pub start: bool,
    /// Force regenerate Shipit file.
    #[arg(long)]
    pub regenerate: bool,
    /// Optional Shipit path override.
    #[arg(long)]
    pub shipit_path: Option<Utf8PathBuf>,
    /// Environment name to load from .env.<name>.
    #[arg(long)]
    pub env_name: Option<String>,
}

#[derive(Parser, Debug, Default)]
pub struct GenerateArgs {
    /// Path to write Shipit file.
    #[arg(long)]
    pub shipit_path: Option<Utf8PathBuf>,
    /// Force provider name (skips detection).
    #[arg(long)]
    pub use_provider: Option<String>,
    /// Override install command.
    #[arg(long)]
    pub install_command: Option<String>,
    /// Override build command.
    #[arg(long)]
    pub build_command: Option<String>,
    /// Override start command.
    #[arg(long)]
    pub start_command: Option<String>,
    /// Override after_deploy command.
    #[arg(long)]
    pub after_deploy_command: Option<String>,
}

#[derive(Parser, Debug, Default)]
pub struct PlanArgs {
    /// Shipit path override.
    #[arg(long)]
    pub shipit_path: Option<Utf8PathBuf>,
    /// Environment name to load from .env.<name>.
    #[arg(long)]
    pub env_name: Option<String>,
}

#[derive(Parser, Debug, Default)]
pub struct BuildArgs {
    /// Shipit path override.
    #[arg(long)]
    pub shipit_path: Option<Utf8PathBuf>,
    /// Environment name to load from .env.<name>.
    #[arg(long)]
    pub env_name: Option<String>,
    /// Use Wasmer backend.
    #[arg(long)]
    pub wasmer: bool,
    /// Use Docker backend.
    #[arg(long)]
    pub docker: bool,
    /// Skip prepare steps.
    #[arg(long)]
    pub skip_prepare: bool,
}

#[derive(Parser, Debug, Default)]
pub struct ServeArgs {
    /// Shipit path override.
    #[arg(long)]
    pub shipit_path: Option<Utf8PathBuf>,
    /// Environment name to load from .env.<name>.
    #[arg(long)]
    pub env_name: Option<String>,
    /// Use Wasmer backend.
    #[arg(long)]
    pub wasmer: bool,
    /// Use Docker backend.
    #[arg(long)]
    pub docker: bool,
}

/// CLI dispatcher. Defaults to `auto` when no subcommand is provided.
pub fn run() -> Result<()> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();
    match cli
        .command
        .unwrap_or(Command::Auto(cli.auto_args.clone()))
    {
        Command::Auto(args) => run_auto(cli.path, args),
        Command::Generate(args) => run_generate(cli.path, args),
        Command::Plan(args) => run_plan(cli.path, args),
        Command::Build(args) => run_build(cli.path, args),
        Command::Serve(args) => run_serve(cli.path, args),
    }
}

// Placeholder handlers to be implemented as control flows are ported.
fn run_auto(_path: Utf8PathBuf, _args: AutoArgs) -> Result<()> {
    let shipit_path = _args
        .shipit_path
        .clone()
        .unwrap_or_else(|| _path.join("Shipit"));
    if _args.regenerate || !shipit_path.exists() {
        run_generate(
            _path.clone(),
            GenerateArgs {
                shipit_path: Some(shipit_path.clone()),
                ..GenerateArgs::default()
            },
        )?;
    }

    let env = load_env(&_path, _args.env_name.as_deref())?;
    build_with_provider(
        _path,
        shipit_path,
        env,
        _args.wasmer,
        _args.docker,
        _args.skip_prepare,
        _args.start,
    )
}

fn run_generate(_path: Utf8PathBuf, _args: GenerateArgs) -> Result<()> {
    let shipit_path = _args
        .shipit_path
        .clone()
        .unwrap_or_else(|| _path.join("Shipit"));
    let custom = CustomCommands {
        install: _args.install_command.clone(),
        build: _args.build_command.clone(),
        start: _args.start_command.clone(),
        after_deploy: _args.after_deploy_command.clone(),
    };

    // Honor explicit provider name if present.
    let provider = if let Some(name) = &_args.use_provider {
        let providers = registry::providers();
        let provider = providers
            .into_iter()
            .find(|p| p.name().eq_ignore_ascii_case(name))
            .ok_or_else(|| anyhow::anyhow!("Provider {} not found", name))?;
        provider.create(_path.as_std_path(), &custom)?
    } else {
        detect_registered_provider(_path.as_std_path(), &custom)?
            .ok_or_else(|| anyhow::anyhow!("No provider detected"))?
            .0
    };

    let plan = provider.plan()?;
    let generator = ShipitGenerator::new(GeneratorOptions {
        project_name: _path
            .file_name()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "app".to_string()),
    });
    let content = generator.generate(&plan)?;
    std::fs::write(&shipit_path, content)?;
    println!("Generated Shipit at {}", shipit_path);
    Ok(())
}

fn run_plan(_path: Utf8PathBuf, _args: PlanArgs) -> Result<()> {
    // Placeholder: just generate Shipit for now.
    run_generate(_path, GenerateArgs::default())
}

fn run_build(_path: Utf8PathBuf, _args: BuildArgs) -> Result<()> {
    let env = load_env(&_path, _args.env_name.as_deref())?;
    let shipit_path = _args
        .shipit_path
        .clone()
        .unwrap_or_else(|| _path.join("Shipit"));

    // Generate Shipit if it doesn't exist yet.
    if !shipit_path.exists() {
        run_generate(
            _path.clone(),
            GenerateArgs {
                shipit_path: Some(shipit_path.clone()),
                ..Default::default()
            },
        )?;
    }

    build_with_provider(
        _path,
        shipit_path,
        env,
        _args.wasmer,
        _args.docker,
        _args.skip_prepare,
        false,
    )
}

fn run_serve(_path: Utf8PathBuf, _args: ServeArgs) -> Result<()> {
    let env = load_env(&_path, _args.env_name.as_deref())?;
    let shipit_path = _args
        .shipit_path
        .clone()
        .unwrap_or_else(|| _path.join("Shipit"));
    build_with_provider(
        _path,
        shipit_path,
        env,
        _args.wasmer,
        _args.docker,
        false,
        true,
    )
}

fn build_with_provider(
    path: Utf8PathBuf,
    shipit_path: Utf8PathBuf,
    env: std::collections::BTreeMap<String, String>,
    use_wasmer: bool,
    use_docker: bool,
    skip_prepare: bool,
    run_start: bool,
) -> Result<()> {
    use crate::builder::local::LocalBuilder;
    use crate::builder::wasmer::WasmerBuilder;
    use crate::model::Mount;

    if use_docker {
        anyhow::bail!("Docker backend not implemented yet");
    }

    let mut base_builder: Box<dyn Builder> = Box::new(LocalBuilder::new(path.clone())?);
    if use_wasmer {
        base_builder = Box::new(WasmerBuilder::new(base_builder, path.clone())?);
    }

    let (ctx, serve) = evaluate_shipit(&shipit_path, base_builder.as_mut())?;

    let mounts: Vec<Mount> = ctx.mounts.iter().cloned().collect();

    base_builder.build(&env, &mounts, &serve.build)?;
    if let Some(prepare) = &serve.prepare {
        if !skip_prepare {
            base_builder.prepare(&env, prepare)?;
        }
    }
    base_builder.build_serve(&serve)?;
    base_builder.finalize_build(&serve)?;
    if run_start {
        let command = serve
            .commands
            .keys()
            .next()
            .cloned()
            .unwrap_or_else(|| "start".to_string());
        base_builder.run_serve_command(&command)?;
    }
    println!("Build complete ({})", serve.provider);
    Ok(())
}

#[allow(dead_code)]
fn build_steps_for_staticfile(
    plan: &crate::model::ProviderPlan,
    path: &Utf8PathBuf,
) -> Result<Vec<Step>> {
    use crate::model::{CopyBase, CopyStep, Step, WorkdirStep};

    let mut steps = Vec::new();
    let app_build = path.join(".shipit/local/build/app");
    steps.push(Step::Workdir(WorkdirStep { path: app_build }));

    let mut root = ".".to_string();
    for s in &plan.build_steps {
        if let Some(idx) = s.find("copy(") {
            if let Some(rest) = s[idx + 5..].split(',').next() {
                let parsed: String =
                    serde_json::from_str(rest.trim()).unwrap_or_else(|_| ".".to_string());
                root = parsed;
                break;
            }
        }
    }
    steps.push(Step::Copy(CopyStep {
        source: root,
        target: ".".to_string(),
        ignore: vec![".git".to_string()],
        base: CopyBase::Source,
    }));
    Ok(steps)
}
