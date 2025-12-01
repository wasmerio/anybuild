//! CLI entrypoints mirroring the Python tool (`auto`, `generate`, `plan`,
//! `build`, `serve`).

use camino::Utf8PathBuf;
use clap::{Parser, Subcommand};
use tempfile::Builder as TempFileBuilder;

use crate::Result;
use crate::builder::Builder;
use crate::detect::detect_registered_provider;
use crate::env::load_env;
use crate::generator::{GeneratorOptions, ShipitGenerator};
use crate::model::{CustomCommands, Step};
use crate::procfile::Procfile;
use crate::provider::registry;
use crate::starlark_runtime::evaluate_shipit;

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
    /// Deploy project (placeholder; Wasmer deploy parity pending).
    Deploy(DeployArgs),
}

#[derive(Parser, Debug, Default, Clone)]
pub struct ProviderArgs {
    /// Use Procfile to infer start command.
    #[arg(long, default_value_t = true)]
    pub use_procfile: bool,
    /// Choose a specific provider.
    #[arg(long)]
    pub use_provider: Option<String>,
}

#[derive(Parser, Debug, Default, Clone)]
pub struct CommandOverrides {
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

#[derive(Parser, Debug, Default, Clone)]
pub struct WasmerArgs {
    /// Use Wasmer backend.
    #[arg(long)]
    pub wasmer: bool,
    /// Path to Wasmer binary.
    #[arg(long)]
    pub wasmer_bin: Option<String>,
    /// Optional Wasmer registry.
    #[arg(long)]
    pub wasmer_registry: Option<String>,
    /// Optional Wasmer token.
    #[arg(long)]
    pub wasmer_token: Option<String>,
}

#[derive(Parser, Debug, Default, Clone)]
pub struct WasmerDeployArgs {
    /// Deploy the project to Wasmer (instead of running start).
    #[arg(long)]
    pub wasmer_deploy: bool,
    /// Write Wasmer deploy config output to a file.
    #[arg(long)]
    pub wasmer_deploy_config: Option<Utf8PathBuf>,
    /// Owner of the Wasmer app.
    #[arg(long)]
    pub wasmer_app_owner: Option<String>,
    /// Name of the Wasmer app.
    #[arg(long)]
    pub wasmer_app_name: Option<String>,
}

#[derive(Parser, Debug, Default, Clone)]
pub struct DockerArgs {
    /// Use Docker backend.
    #[arg(long)]
    pub docker: bool,
    /// Docker client (docker, podman, depot, etc.).
    #[arg(long)]
    pub docker_client: Option<String>,
    /// Skip Docker when build steps have no run commands.
    #[arg(long, default_value_t = true)]
    pub skip_docker_if_safe_build: bool,
}

#[derive(Parser, Debug, Default, Clone)]
pub struct AutoArgs {
    #[command(flatten)]
    pub wasmer: WasmerArgs,
    #[command(flatten)]
    pub wasmer_deploy: WasmerDeployArgs,
    #[command(flatten)]
    pub docker: DockerArgs,
    #[command(flatten)]
    pub provider: ProviderArgs,
    #[command(flatten)]
    pub overrides: CommandOverrides,
    /// Skip prepare steps.
    #[arg(long)]
    pub skip_prepare: bool,
    /// Run start command after build.
    #[arg(long)]
    pub start: bool,
    /// Force regenerate Shipit file.
    #[arg(long)]
    pub regenerate: bool,
    /// Use a temporary Shipit path in the system temp directory.
    #[arg(long)]
    pub temp_shipit: bool,
    /// Optional Shipit path override.
    #[arg(long)]
    pub shipit_path: Option<Utf8PathBuf>,
    /// Environment name to load from .env.<name>.
    #[arg(long)]
    pub env_name: Option<String>,
}

#[derive(Parser, Debug)]
pub struct GenerateArgs {
    /// Project path (defaults to current directory).
    #[arg(value_name = "PATH", default_value = ".")]
    pub path: Utf8PathBuf,
    #[command(flatten)]
    pub provider: ProviderArgs,
    #[command(flatten)]
    pub overrides: CommandOverrides,
    /// Path to write Shipit file.
    #[arg(long)]
    pub shipit_path: Option<Utf8PathBuf>,
}

impl Default for GenerateArgs {
    fn default() -> Self {
        Self {
            path: Utf8PathBuf::from("."),
            provider: Default::default(),
            overrides: Default::default(),
            shipit_path: None,
        }
    }
}

#[derive(Parser, Debug, Default)]
pub struct PlanArgs {
    #[command(flatten)]
    pub provider: ProviderArgs,
    #[command(flatten)]
    pub overrides: CommandOverrides,
    /// Output path of the plan (defaults to stdout).
    #[arg(short = 'o', long, aliases = ["output", "out"])]
    pub out: Option<Utf8PathBuf>,
    /// Use a temporary Shipit file in the system temp directory.
    #[arg(long)]
    pub temp_shipit: bool,
    /// Shipit path override.
    #[arg(long)]
    pub shipit_path: Option<Utf8PathBuf>,
    /// Environment name to load from .env.<name>.
    #[arg(long)]
    pub env_name: Option<String>,
    /// Regenerate Shipit when missing.
    #[arg(long)]
    pub regenerate: bool,
}

#[derive(Parser, Debug, Default)]
pub struct BuildArgs {
    #[command(flatten)]
    pub wasmer: WasmerArgs,
    #[command(flatten)]
    pub docker: DockerArgs,
    #[command(flatten)]
    pub provider: ProviderArgs,
    #[command(flatten)]
    pub overrides: CommandOverrides,
    /// Shipit path override.
    #[arg(long)]
    pub shipit_path: Option<Utf8PathBuf>,
    /// Environment name to load from .env.<name>.
    #[arg(long)]
    pub env_name: Option<String>,
    /// Skip prepare steps.
    #[arg(long)]
    pub skip_prepare: bool,
    /// Regenerate Shipit before building (or when missing).
    #[arg(long)]
    pub regenerate: bool,
}

#[derive(Parser, Debug, Default)]
pub struct ServeArgs {
    #[command(flatten)]
    pub wasmer: WasmerArgs,
    #[command(flatten)]
    pub wasmer_deploy: WasmerDeployArgs,
    #[command(flatten)]
    pub docker: DockerArgs,
    #[command(flatten)]
    pub provider: ProviderArgs,
    #[command(flatten)]
    pub overrides: CommandOverrides,
    /// Shipit path override.
    #[arg(long)]
    pub shipit_path: Option<Utf8PathBuf>,
    /// Environment name to load from .env.<name>.
    #[arg(long)]
    pub env_name: Option<String>,
    /// Run start command (disable to only build).
    #[arg(long, default_value_t = true)]
    pub start: bool,
    /// Regenerate Shipit before serving (or when missing).
    #[arg(long)]
    pub regenerate: bool,
}

#[derive(Parser, Debug, Default)]
pub struct DeployArgs {
    #[command(flatten)]
    pub wasmer: WasmerArgs,
    #[command(flatten)]
    pub wasmer_deploy: WasmerDeployArgs,
    #[command(flatten)]
    pub provider: ProviderArgs,
    #[command(flatten)]
    pub overrides: CommandOverrides,
    /// Shipit path override.
    #[arg(long)]
    pub shipit_path: Option<Utf8PathBuf>,
    /// Environment name to load from .env.<name>.
    #[arg(long)]
    pub env_name: Option<String>,
    /// Regenerate Shipit before deploying (or when missing).
    #[arg(long)]
    pub regenerate: bool,
}

#[derive(Clone)]
enum FinalAction {
    None,
    RunStart,
    WasmerDeploy {
        deploy: bool,
        config: Option<Utf8PathBuf>,
        app_owner: Option<String>,
        app_name: Option<String>,
    },
}

/// CLI dispatcher. Defaults to `auto` when no subcommand is provided.
pub fn run() -> Result<()> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();
    match cli.command.unwrap_or(Command::Auto(cli.auto_args.clone())) {
        Command::Auto(args) => run_auto(cli.path, args),
        Command::Generate(args) => run_generate(args),
        Command::Plan(args) => run_plan(cli.path, args),
        Command::Build(args) => run_build(cli.path, args),
        Command::Serve(args) => run_serve(cli.path, args),
        Command::Deploy(args) => run_deploy(cli.path, args),
    }
}

fn temporary_shipit_path() -> Result<Utf8PathBuf> {
    let temp = TempFileBuilder::new().prefix("Shipit").tempfile()?;
    let path = temp.into_temp_path().keep()?;
    Utf8PathBuf::from_path_buf(path)
        .map_err(|_| anyhow::anyhow!("Temporary Shipit path is not valid UTF-8"))
}

// Placeholder handlers to be implemented as control flows are ported.
fn run_auto(_path: Utf8PathBuf, args: AutoArgs) -> Result<()> {
    let shipit_path = if args.temp_shipit {
        if args.shipit_path.is_some() {
            anyhow::bail!("Cannot use both --temp-shipit and --shipit-path");
        }
        temporary_shipit_path()?
    } else {
        args.shipit_path.clone().unwrap_or_else(|| {
            if _path.is_file() {
                _path.clone()
            } else {
                _path.join("Shipit")
            }
        })
    };

    let mut regenerate = args.regenerate;
    if !regenerate && !shipit_path.exists() {
        regenerate = true;
    }
    if regenerate {
        run_generate(GenerateArgs {
            path: _path.clone(),
            shipit_path: Some(shipit_path.clone()),
            provider: args.provider.clone(),
            overrides: args.overrides.clone(),
            ..Default::default()
        })?;
    }

    let env = load_env(&_path, args.env_name.as_deref())?;
    let mut wasmer = args.wasmer.clone();
    if args.wasmer_deploy.wasmer_deploy || args.wasmer_deploy.wasmer_deploy_config.is_some() {
        wasmer.wasmer = true;
    }
    let action =
        if args.wasmer_deploy.wasmer_deploy || args.wasmer_deploy.wasmer_deploy_config.is_some() {
            FinalAction::WasmerDeploy {
                deploy: args.wasmer_deploy.wasmer_deploy,
                config: args.wasmer_deploy.wasmer_deploy_config.clone(),
                app_owner: args.wasmer_deploy.wasmer_app_owner.clone(),
                app_name: args.wasmer_deploy.wasmer_app_name.clone(),
            }
        } else if args.start {
            FinalAction::RunStart
        } else {
            FinalAction::None
        };
    build_with_provider(
        _path,
        shipit_path,
        env,
        &wasmer,
        &args.docker,
        args.skip_prepare,
        action,
    )
}

fn run_generate(args: GenerateArgs) -> Result<()> {
    let shipit_path = args
        .shipit_path
        .clone()
        .unwrap_or_else(|| args.path.join("Shipit"));
    let custom = resolve_custom_commands(&args.path, &args.provider, &args.overrides);

    // Honor explicit provider name if present.
    let provider = if let Some(name) = &args.provider.use_provider {
        let providers = registry::providers();
        let provider = providers
            .into_iter()
            .find(|p| p.name().eq_ignore_ascii_case(name))
            .ok_or_else(|| anyhow::anyhow!("Provider {} not found", name))?;
        provider.create(args.path.as_std_path(), &custom)?
    } else {
        detect_registered_provider(args.path.as_std_path(), &custom)?
            .ok_or_else(|| anyhow::anyhow!("No provider detected"))?
            .0
    };

    let plan = provider.plan()?;
    let generator = ShipitGenerator::new(GeneratorOptions {
        project_name: args
            .path
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
    let env = load_env(&_path, _args.env_name.as_deref())?;
    for (k, v) in &env {
        // set_var is unsafe in this toolchain; propagate env into the process for evaluation.
        unsafe {
            std::env::set_var(k, v);
        }
    }
    let shipit_path = if _args.temp_shipit {
        if _args.shipit_path.is_some() {
            anyhow::bail!("Cannot use both --temp-shipit and --shipit-path");
        }
        temporary_shipit_path()?
    } else {
        _args.shipit_path.clone().unwrap_or_else(|| {
            if _path.is_file() {
                _path.clone()
            } else {
                _path.join("Shipit")
            }
        })
    };
    let mut regenerate = _args.regenerate;
    if !regenerate && !shipit_path.exists() {
        regenerate = true;
    }
    if regenerate {
        let overrides = _args.overrides.clone();
        let provider = _args.provider.clone();
        run_generate(GenerateArgs {
            path: _path.clone(),
            shipit_path: Some(shipit_path.clone()),
            provider,
            overrides,
            ..Default::default()
        })?;
    }

    let mut builder: Box<dyn Builder> =
        Box::new(crate::builder::local::LocalBuilder::new(_path.clone())?);
    let (ctx, serve) = evaluate_shipit(&shipit_path, builder.as_mut())?;

    // Build metadata commands (start/after_deploy plus grouped build/install).
    let collect_group = |group: &str| -> Option<String> {
        let cmds: Vec<String> = serve
            .build
            .iter()
            .filter_map(|s| {
                if let Step::Run(r) = s {
                    if r.group.as_deref() == Some(group) {
                        return Some(r.command.clone());
                    }
                }
                None
            })
            .collect();
        if cmds.is_empty() {
            None
        } else {
            Some(cmds.join(" && "))
        }
    };
    let mut metadata_commands: std::collections::BTreeMap<String, Option<String>> =
        std::collections::BTreeMap::new();
    metadata_commands.insert("start".to_string(), serve.commands.get("start").cloned());
    metadata_commands.insert(
        "after_deploy".to_string(),
        serve.commands.get("after_deploy").cloned(),
    );
    metadata_commands.insert("install".to_string(), collect_group("install"));
    metadata_commands.insert("build".to_string(), collect_group("build"));

    let custom = resolve_custom_commands(&_path, &_args.provider, &_args.overrides);
    let platform = detect_registered_provider(_path.as_std_path(), &custom)
        .ok()
        .and_then(|opt| opt)
        .and_then(|(provider, _)| provider.plan().ok().and_then(|p| p.platform));

    #[derive(serde::Serialize)]
    struct PlanOut {
        provider: String,
        metadata: Metadata,
        config: Vec<String>,
        services: Vec<ServiceOut>,
    }
    #[derive(serde::Serialize)]
    struct Metadata {
        platform: Option<String>,
        commands: std::collections::BTreeMap<String, Option<String>>,
    }
    #[derive(serde::Serialize)]
    struct ServiceOut {
        name: String,
        provider: crate::model::ServiceProvider,
    }
    let out = PlanOut {
        provider: serve.provider.clone(),
        metadata: Metadata {
            platform,
            commands: metadata_commands,
        },
        config: ctx.getenv_variables.iter().cloned().collect(),
        services: serve
            .services
            .clone()
            .unwrap_or_default()
            .into_iter()
            .map(|s| ServiceOut {
                name: s.name,
                provider: s.provider,
            })
            .collect(),
    };
    let json = serde_json::to_string_pretty(&out)?;
    if let Some(out_path) = &_args.out {
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(out_path, &json)?;
        println!("Plan saved to {}", out_path);
    } else {
        println!("{}", json);
    }
    Ok(())
}

fn run_build(_path: Utf8PathBuf, _args: BuildArgs) -> Result<()> {
    let env = load_env(&_path, _args.env_name.as_deref())?;
    let shipit_path = _args.shipit_path.clone().unwrap_or_else(|| {
        if _path.is_file() {
            _path.clone()
        } else {
            _path.join("Shipit")
        }
    });

    let mut regenerate = _args.regenerate;
    if !shipit_path.exists() {
        regenerate = true;
    }
    if regenerate {
        run_generate(GenerateArgs {
            path: _path.clone(),
            shipit_path: Some(shipit_path.clone()),
            provider: _args.provider.clone(),
            overrides: _args.overrides.clone(),
            ..Default::default()
        })?;
    }

    build_with_provider(
        _path,
        shipit_path,
        env,
        &_args.wasmer,
        &_args.docker,
        _args.skip_prepare,
        FinalAction::None,
    )
}

fn run_serve(_path: Utf8PathBuf, _args: ServeArgs) -> Result<()> {
    let env = load_env(&_path, _args.env_name.as_deref())?;
    let shipit_path = _args.shipit_path.clone().unwrap_or_else(|| {
        if _path.is_file() {
            _path.clone()
        } else {
            _path.join("Shipit")
        }
    });
    let mut regenerate = _args.regenerate;
    if !shipit_path.exists() {
        regenerate = true;
    }
    if regenerate {
        run_generate(GenerateArgs {
            path: _path.clone(),
            shipit_path: Some(shipit_path.clone()),
            provider: _args.provider.clone(),
            overrides: _args.overrides.clone(),
            ..Default::default()
        })?;
    }
    let mut wasmer = _args.wasmer.clone();
    if _args.wasmer_deploy.wasmer_deploy || _args.wasmer_deploy.wasmer_deploy_config.is_some() {
        wasmer.wasmer = true;
    }
    let action = if _args.wasmer_deploy.wasmer_deploy
        || _args.wasmer_deploy.wasmer_deploy_config.is_some()
    {
        FinalAction::WasmerDeploy {
            deploy: _args.wasmer_deploy.wasmer_deploy,
            config: _args.wasmer_deploy.wasmer_deploy_config.clone(),
            app_owner: _args.wasmer_deploy.wasmer_app_owner.clone(),
            app_name: _args.wasmer_deploy.wasmer_app_name.clone(),
        }
    } else if _args.start {
        FinalAction::RunStart
    } else {
        FinalAction::None
    };
    build_with_provider(
        _path,
        shipit_path,
        env,
        &wasmer,
        &_args.docker,
        false,
        action,
    )
}

fn run_deploy(_path: Utf8PathBuf, _args: DeployArgs) -> Result<()> {
    let shipit_path = _args.shipit_path.clone().unwrap_or_else(|| {
        if _path.is_file() {
            _path.clone()
        } else {
            _path.join("Shipit")
        }
    });
    let mut regenerate = _args.regenerate;
    if !shipit_path.exists() {
        regenerate = true;
    }
    if regenerate {
        run_generate(GenerateArgs {
            path: _path.clone(),
            shipit_path: Some(shipit_path.clone()),
            provider: _args.provider.clone(),
            overrides: _args.overrides.clone(),
            ..Default::default()
        })?;
    }

    let env = load_env(&_path, _args.env_name.as_deref())?;
    let mut wasmer = _args.wasmer.clone();
    let deploy_flag = if _args.wasmer_deploy.wasmer_deploy
        || _args.wasmer_deploy.wasmer_deploy_config.is_some()
    {
        _args.wasmer_deploy.wasmer_deploy
    } else {
        true
    };
    let action = FinalAction::WasmerDeploy {
        deploy: deploy_flag,
        config: _args.wasmer_deploy.wasmer_deploy_config.clone(),
        app_owner: _args.wasmer_deploy.wasmer_app_owner.clone(),
        app_name: _args.wasmer_deploy.wasmer_app_name.clone(),
    };
    wasmer.wasmer = true;

    build_with_provider(
        _path,
        shipit_path,
        env,
        &wasmer,
        &DockerArgs::default(),
        false,
        action,
    )
}

fn build_with_provider(
    path: Utf8PathBuf,
    shipit_path: Utf8PathBuf,
    env: std::collections::BTreeMap<String, String>,
    wasmer: &WasmerArgs,
    docker: &DockerArgs,
    skip_prepare: bool,
    final_action: FinalAction,
) -> Result<()> {
    use crate::builder::docker::DockerBuilder;
    use crate::builder::local::LocalBuilder;
    use crate::builder::wasmer::WasmerBuilder;
    use crate::model::Mount;

    if wasmer.wasmer && docker.docker {
        anyhow::bail!("Choose either Wasmer or Docker backend, not both");
    }

    let mut base_builder: Box<dyn Builder> = if docker.docker {
        Box::new(DockerBuilder::new(
            path.clone(),
            docker.docker_client.clone(),
        ))
    } else {
        Box::new(LocalBuilder::new(path.clone())?)
    };
    if wasmer.wasmer || matches!(final_action, FinalAction::WasmerDeploy { .. }) {
        let mut wasmer_builder = WasmerBuilder::new(
            base_builder,
            path.clone(),
            wasmer.wasmer_registry.clone(),
            wasmer.wasmer_token.clone(),
        )?;
        if let Some(bin) = wasmer.wasmer_bin.clone() {
            wasmer_builder.bin = bin;
        }
        // Registry/token placeholders kept for future deploy wiring.
        base_builder = Box::new(wasmer_builder);
    }

    // Apply loaded env vars to the process so Starlark getenv and build steps see them.
    for (k, v) in &env {
        // set_var is unsafe in this toolchain; propagate env into the process for build steps.
        unsafe {
            std::env::set_var(k, v);
        }
    }

    let (ctx, serve) = evaluate_shipit(&shipit_path, base_builder.as_mut())?;

    if docker.docker && docker.skip_docker_if_safe_build {
        let has_run = serve.build.iter().any(|s| matches!(s, Step::Run(_)));
        if !has_run {
            println!(
                "Skipping Docker backend (no run steps); building locally to match safe-build behavior"
            );
            return build_with_provider(
                path,
                shipit_path,
                env,
                wasmer,
                &DockerArgs {
                    docker: false,
                    docker_client: None,
                    skip_docker_if_safe_build: false,
                },
                skip_prepare,
                final_action.clone(),
            );
        }
    }

    let mounts: Vec<Mount> = ctx.mounts.iter().cloned().collect();

    base_builder.build(&env, &mounts, &serve.build)?;
    if let Some(prepare) = &serve.prepare {
        if !skip_prepare {
            base_builder.prepare(&env, prepare)?;
        }
    }
    base_builder.build_serve(&serve)?;
    base_builder.finalize_build(&serve)?;
    match final_action {
        FinalAction::None => {}
        FinalAction::RunStart => {
            let command = serve
                .commands
                .get("start")
                .map(|_| "start".to_string())
                .or_else(|| serve.commands.keys().next().cloned())
                .unwrap_or_else(|| "start".to_string());
            base_builder.run_serve_command(&command)?;
        }
        FinalAction::WasmerDeploy {
            deploy,
            config,
            app_owner,
            app_name,
        } => {
            let wasmer = base_builder
                .as_any()
                .downcast_mut::<WasmerBuilder>()
                .ok_or_else(|| {
                    anyhow::anyhow!("Wasmer deploy requested but Wasmer backend is not active")
                })?;
            if let Some(path) = config {
                wasmer.deploy_config(&path)?;
            }
            if deploy {
                wasmer.deploy(app_owner, app_name)?;
            }
        }
    }
    println!("Build complete ({})", serve.provider);
    Ok(())
}

/// Merge Procfile-derived defaults with explicit CLI overrides.
fn resolve_custom_commands(
    path: &Utf8PathBuf,
    provider_args: &ProviderArgs,
    overrides: &CommandOverrides,
) -> CustomCommands {
    let mut custom = CustomCommands::default();
    if provider_args.use_procfile {
        let procfile_path = path.join("Procfile");
        if procfile_path.exists() {
            if let Ok(procfile) = Procfile::load(procfile_path.as_std_path()) {
                custom.start = procfile.start_command();
            }
        }
    }
    if let Some(start) = &overrides.start_command {
        custom.start = Some(start.clone());
    }
    if let Some(install) = &overrides.install_command {
        custom.install = Some(install.clone());
    }
    if let Some(build) = &overrides.build_command {
        custom.build = Some(build.clone());
    }
    if let Some(after_deploy) = &overrides.after_deploy_command {
        custom.after_deploy = Some(after_deploy.clone());
    }
    custom
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
