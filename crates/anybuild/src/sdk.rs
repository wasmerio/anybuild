use std::path::{Path, PathBuf};

use anyhow::{bail, Result as AnyResult};
use indexmap::IndexMap;
use serde::Serialize;
use serde_json::Value;

use crate::plan::{Serve, Service, Step};

use crate::artifact::RuntimeArtifact;
use crate::deploy::resolve_deployer;
use crate::error::{Error, ErrorKind, Result};
pub use crate::event::ProcessIo;
use crate::event::{
    BuildPlanPackage, BuildPlanStep, DeployScript, DiagnosticLevel, Event, EventHandler,
    PackagePhase, Reporter,
};
use crate::internal::context::{
    resolve_anybuild_dir, resolve_environment, resolve_project_context,
    resolve_project_context_for_check, EnvironmentOptions, ProjectContext,
};
use crate::internal::generator::generate_anybuild;
use crate::internal::paths::{
    default_anybuild_path, migrate_legacy_anybuild, resolve_project_paths, ProjectPaths,
};
use crate::internal::volumes::{
    build_volumes, load_volume_mappings, merge_volume_mappings, parse_cli_volume_mappings,
};
use crate::operation::OperationContext;

#[derive(Debug, Clone, Default)]
pub struct CommandOverrides {
    pub start_command: Option<String>,
    pub install_command: Option<String>,
    pub build_command: Option<String>,
    pub serve_port: Option<i64>,
    pub use_provider: Option<String>,
    pub config: Option<Value>,
}

#[derive(Debug, Clone, Default)]
pub struct DockerOptions {
    pub client: Option<String>,
    pub extra_options: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct WasmerOptions {
    pub binary: Option<String>,
    pub registry: Option<String>,
    pub token: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct FlyOptions {
    pub binary: Option<String>,
    pub token: Option<String>,
    pub app: Option<String>,
    pub config: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LambdaArchitecture {
    X86_64,
    Arm64,
}

impl LambdaArchitecture {
    pub(crate) fn as_aws_value(self) -> &'static str {
        match self {
            Self::X86_64 => "x86_64",
            Self::Arm64 => "arm64",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct AwsLambdaOptions {
    pub binary: Option<String>,
    pub docker_binary: Option<String>,
    pub profile: Option<String>,
    pub region: Option<String>,
    pub function: Option<String>,
    pub role: Option<String>,
    pub repository: Option<String>,
    pub image_tag: Option<String>,
    pub architecture: Option<LambdaArchitecture>,
    pub adapter_layer: Option<String>,
}

#[derive(Debug, Clone)]
pub enum DeploymentPlatform {
    Wasmer(WasmerOptions),
    Fly(FlyOptions),
    AwsLambda(AwsLambdaOptions),
}

impl Default for DeploymentPlatform {
    fn default() -> Self {
        Self::Wasmer(WasmerOptions::default())
    }
}

#[derive(Debug, Clone, Default)]
pub enum BuildEnvironment {
    #[default]
    Local,
    Docker(DockerOptions),
}

#[derive(Debug, Clone, Default)]
pub enum RuntimeEnvironment {
    #[default]
    Local,
    Docker(DockerOptions),
    Lambda(DockerOptions),
    Wasmer(WasmerOptions),
}

#[derive(Debug, Clone, Default)]
pub struct GenerateOptions {
    pub output: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct PlanOptions {
    pub anybuild_path: Option<PathBuf>,
    pub regenerate: bool,
    pub temporary: bool,
    pub serve_port: Option<i64>,
    pub build_environment: BuildEnvironment,
    pub runtime_environment: RuntimeEnvironment,
    pub process_io: ProcessIo,
}

impl Default for PlanOptions {
    fn default() -> Self {
        Self {
            anybuild_path: None,
            regenerate: false,
            temporary: false,
            serve_port: None,
            build_environment: BuildEnvironment::Local,
            runtime_environment: RuntimeEnvironment::Local,
            process_io: ProcessIo::Inherit,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BuildOptions {
    pub anybuild_path: Option<PathBuf>,
    pub build_environment: BuildEnvironment,
    pub runtime_environment: RuntimeEnvironment,
    pub process_io: ProcessIo,
    pub skip_prepare: bool,
    pub skip_docker_if_safe: bool,
    pub env_name: Option<String>,
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            anybuild_path: None,
            build_environment: BuildEnvironment::Local,
            runtime_environment: RuntimeEnvironment::Local,
            process_io: ProcessIo::Inherit,
            skip_prepare: false,
            skip_docker_if_safe: true,
            env_name: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RunOptions {
    pub build_environment: BuildEnvironment,
    pub runtime_environment: RuntimeEnvironment,
    pub process_io: ProcessIo,
    pub commands: Vec<String>,
    pub volumes: Vec<(String, String)>,
    pub start: bool,
    pub after_deploy: bool,
    pub serve_port: Option<i64>,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            build_environment: BuildEnvironment::Local,
            runtime_environment: RuntimeEnvironment::Local,
            process_io: ProcessIo::Inherit,
            commands: Vec::new(),
            volumes: Vec::new(),
            start: false,
            after_deploy: false,
            serve_port: None,
        }
    }
}

impl RunOptions {
    pub fn start(mut self) -> Self {
        self.start = true;
        self
    }

    pub fn after_deploy(mut self) -> Self {
        self.after_deploy = true;
        self
    }

    pub fn command(mut self, command: impl Into<String>) -> Self {
        self.commands.push(command.into());
        self
    }

    pub fn volume(mut self, name: impl Into<String>, guest_path: impl Into<String>) -> Self {
        self.volumes.push((name.into(), guest_path.into()));
        self
    }
}

#[derive(Debug, Clone)]
pub enum DeployTarget {
    Publish {
        owner: Option<String>,
        name: Option<String>,
    },
    WriteConfig {
        path: PathBuf,
    },
}

#[derive(Debug, Clone)]
pub struct DeployOptions {
    pub platform: DeploymentPlatform,
    pub target: DeployTarget,
    pub process_io: ProcessIo,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum GenerationPolicy {
    #[default]
    IfMissing,
    Always,
    Temporary,
}

#[derive(Debug, Clone)]
pub struct AutoOptions {
    pub generation: GenerationPolicy,
    pub build: BuildOptions,
    pub run: Option<RunOptions>,
    pub deploy: Option<DeployOptions>,
}

impl Default for AutoOptions {
    fn default() -> Self {
        Self {
            generation: GenerationPolicy::IfMissing,
            build: BuildOptions::default(),
            run: None,
            deploy: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct GeneratedAnybuild {
    pub path: PathBuf,
    pub content: String,
    pub provider: String,
    pub config: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GenerationCheckStatus {
    Current,
    Drifted,
    Missing,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProviderConfigSnapshot {
    pub provider: String,
    pub schema: u32,
    pub values: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ConfigDifference {
    pub path: String,
    pub persisted: Option<Value>,
    pub detected: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GenerationCheck {
    pub path: PathBuf,
    pub status: GenerationCheckStatus,
    pub persisted: Option<ProviderConfigSnapshot>,
    pub detected: ProviderConfigSnapshot,
    pub differences: Vec<ConfigDifference>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectPlan {
    pub provider: String,
    pub config: Value,
    pub services: Vec<Service>,
    pub serve: Serve,
}

#[derive(Debug, Clone, Serialize)]
pub struct BuildOutcome {
    pub plan: ProjectPlan,
    pub state_dir: PathBuf,
    pub artifact: RuntimeArtifact,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct RunOutcome {
    pub executed: Vec<String>,
    pub skipped: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DeployOutcome {
    Published {
        owner: Option<String>,
        name: Option<String>,
    },
    ConfigWritten {
        path: PathBuf,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct AutoOutcome {
    pub generated: Option<GeneratedAnybuild>,
    pub build: BuildOutcome,
    pub run: Option<RunOutcome>,
    pub deploy: Option<DeployOutcome>,
}

struct PreparedDefinition {
    path: Option<PathBuf>,
    generated: Option<GeneratedAnybuild>,
    _temporary: Option<tempfile::NamedTempFile>,
}

/// Project-oriented entry point for the Anybuild SDK.
pub struct Anybuild {
    root: PathBuf,
    subdir: Option<String>,
    overrides: CommandOverrides,
    inherited_env: IndexMap<String, String>,
    env_overrides: IndexMap<String, String>,
    inherit_process_env: bool,
    reporter: Reporter,
}

/// Whether a name may be used as a build variable.
///
/// Names are rendered into generated build definitions where whitespace is
/// structural, so only the POSIX shape `[A-Za-z_][A-Za-z0-9_]*` is accepted.
pub fn is_valid_env_name(name: &str) -> bool {
    let mut characters = name.chars();
    characters
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

impl Anybuild {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            root: path.into(),
            subdir: None,
            overrides: CommandOverrides::default(),
            inherited_env: std::env::vars().collect(),
            env_overrides: IndexMap::new(),
            inherit_process_env: true,
            reporter: Reporter::default(),
        }
    }

    pub fn with_subdir(mut self, subdir: impl Into<String>) -> Self {
        self.subdir = Some(subdir.into());
        self
    }

    pub fn with_provider(mut self, provider: impl Into<String>) -> Self {
        self.overrides.use_provider = Some(provider.into());
        self
    }

    pub fn with_config(mut self, config: Value) -> Self {
        self.overrides.config = Some(config);
        self
    }

    pub fn with_command_overrides(mut self, overrides: CommandOverrides) -> Self {
        self.overrides = overrides;
        self
    }

    pub fn with_start_command(mut self, command: impl Into<String>) -> Self {
        self.overrides.start_command = Some(command.into());
        self
    }

    pub fn with_install_command(mut self, command: impl Into<String>) -> Self {
        self.overrides.install_command = Some(command.into());
        self
    }

    pub fn with_build_command(mut self, command: impl Into<String>) -> Self {
        self.overrides.build_command = Some(command.into());
        self
    }

    pub fn with_serve_port(mut self, port: i64) -> Self {
        self.overrides.serve_port = Some(port);
        self
    }

    pub fn with_env(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.env_overrides.insert(name.into(), value.into());
        self
    }

    pub fn inherit_process_env(mut self, inherit: bool) -> Self {
        self.inherit_process_env = inherit;
        self
    }

    pub fn with_event_handler(mut self, handler: impl EventHandler + 'static) -> Self {
        self.reporter = Reporter::new(handler);
        self
    }

    pub fn generate(&self, options: GenerateOptions) -> Result<GeneratedAnybuild> {
        self.operation(
            ErrorKind::Generation,
            "generate",
            ProcessIo::Inherit,
            |context| self.generate_inner(options, context),
        )
    }

    pub fn check_generation(&self, options: GenerateOptions) -> Result<GenerationCheck> {
        self.operation(
            ErrorKind::Generation,
            "check generation",
            ProcessIo::Inherit,
            |context| self.check_generation_inner(options, context),
        )
    }

    pub fn plan(&self, options: PlanOptions) -> Result<ProjectPlan> {
        let process_io = options.process_io;
        self.operation(ErrorKind::Evaluation, "plan", process_io, |context| {
            self.plan_inner(options, context)
        })
    }

    pub fn build(&self, options: BuildOptions) -> Result<BuildOutcome> {
        let process_io = options.process_io;
        self.operation(ErrorKind::Build, "build", process_io, |context| {
            self.build_inner(options, context)
        })
    }

    pub fn run(&self, options: RunOptions) -> Result<RunOutcome> {
        let process_io = options.process_io;
        self.operation(ErrorKind::Run, "run", process_io, |context| {
            self.run_inner(options, context)
        })
    }

    pub fn deploy(&self, options: DeployOptions) -> Result<DeployOutcome> {
        let process_io = options.process_io;
        self.operation(ErrorKind::Deploy, "deploy", process_io, |context| {
            self.deploy_inner(options, context)
        })
    }

    pub fn auto(&self, options: AutoOptions) -> Result<AutoOutcome> {
        let process_io = options.build.process_io;
        self.operation(ErrorKind::Build, "auto", process_io, |context| {
            self.auto_inner(options, context)
        })
    }

    fn operation<T>(
        &self,
        kind: ErrorKind,
        name: &'static str,
        process_io: ProcessIo,
        operation: impl FnOnce(&OperationContext) -> AnyResult<T>,
    ) -> Result<T> {
        let context = OperationContext::new(
            self.effective_env(),
            self.inherit_process_env,
            process_io,
            self.reporter.clone(),
        );
        operation(&context).map_err(|source| Error::new(kind, name, self.root.clone(), source))
    }

    fn effective_env(&self) -> IndexMap<String, String> {
        let mut env = if self.inherit_process_env {
            self.inherited_env.clone()
        } else {
            IndexMap::new()
        };
        env.extend(self.env_overrides.clone());
        env
    }

    fn paths(&self) -> AnyResult<ProjectPaths> {
        resolve_project_paths(&self.root, self.subdir.as_deref())
    }

    fn generate_inner(
        &self,
        options: GenerateOptions,
        context: &OperationContext,
    ) -> AnyResult<GeneratedAnybuild> {
        let paths = self.paths()?;
        let output = options
            .output
            .unwrap_or_else(|| default_anybuild_path(&paths));
        let (provider, provider_config) =
            crate::internal::context::load_project_config(&paths, &self.overrides, context)?;
        let config = provider_config.persisted_json();
        let runtime_dependencies = provider_config.runtime_dependencies();
        let content = generate_anybuild(
            provider,
            provider_config.base().name.as_deref(),
            paths.subdir.as_deref(),
            provider_config.config_schema(),
            &config,
            &runtime_dependencies,
        )?;
        context.emit(Event::AnybuildGenerating {
            path: output.clone(),
            provider: provider.to_owned(),
            config: config.clone(),
        });
        if let Some(parent) = output.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&output, &content)?;
        let path = output.canonicalize().unwrap_or(output);
        context.emit(Event::FileWritten {
            kind: "anybuild",
            path: path.clone(),
        });
        Ok(GeneratedAnybuild {
            path,
            content,
            provider: provider.to_owned(),
            config,
        })
    }

    fn check_generation_inner(
        &self,
        options: GenerateOptions,
        context: &OperationContext,
    ) -> AnyResult<GenerationCheck> {
        let paths = self.paths()?;
        let path = options
            .output
            .unwrap_or_else(|| default_anybuild_path(&paths));
        let (detected_provider, detected_config) =
            crate::internal::context::load_project_config(&paths, &self.overrides, context)?;
        let detected = ProviderConfigSnapshot {
            provider: detected_provider.to_owned(),
            schema: detected_config.config_schema(),
            values: detected_config.persisted_json(),
        };
        if !path.is_file() {
            return Ok(GenerationCheck {
                path,
                status: GenerationCheckStatus::Missing,
                persisted: None,
                detected,
                differences: Vec::new(),
            });
        }

        let mut overrides = self.overrides.clone();
        overrides.use_provider = None;
        let project = resolve_project_context_for_check(
            &paths.workspace_root,
            paths.subdir.as_deref(),
            &path,
            &overrides,
            context,
        )?;
        let persisted = ProviderConfigSnapshot {
            provider: project.persisted_config.provider.clone(),
            schema: project.persisted_config.schema,
            values: project.persisted_config.values.clone(),
        };
        let differences = config_differences(&persisted, &detected);
        let status = if differences.is_empty() {
            GenerationCheckStatus::Current
        } else {
            GenerationCheckStatus::Drifted
        };
        Ok(GenerationCheck {
            path,
            status,
            persisted: Some(persisted),
            detected,
            differences,
        })
    }

    fn plan_inner(
        &self,
        options: PlanOptions,
        context: &OperationContext,
    ) -> AnyResult<ProjectPlan> {
        let paths = self.paths()?;
        let policy = if options.temporary {
            GenerationPolicy::Temporary
        } else if options.regenerate {
            GenerationPolicy::Always
        } else {
            GenerationPolicy::IfMissing
        };
        let definition = self.prepare_definition(&paths, options.anybuild_path, policy, context)?;
        let project = resolve_project_context(
            &paths.workspace_root,
            paths.subdir.as_deref(),
            definition.path.as_deref(),
            &CommandOverrides {
                serve_port: options.serve_port,
                ..self.overrides.clone()
            },
            &environment_options(&options.build_environment, &options.runtime_environment),
            context,
        )?;
        Ok(project_plan(&project))
    }

    fn build_inner(
        &self,
        options: BuildOptions,
        operation: &OperationContext,
    ) -> AnyResult<BuildOutcome> {
        let mut path = self.root.clone();
        let mut subdir = self.subdir.clone();
        let mut build_environment = options.build_environment.clone();
        let mut skip_safe = options.skip_docker_if_safe;
        loop {
            let env_options = environment_options(&build_environment, &options.runtime_environment);
            let mut context = resolve_project_context(
                &path,
                subdir.as_deref(),
                options.anybuild_path.as_deref(),
                &self.overrides,
                &env_options,
                operation,
            )?;
            if skip_safe
                && matches!(build_environment, BuildEnvironment::Docker(_))
                && !context.serve.build.is_empty()
                && !context
                    .serve
                    .build
                    .iter()
                    .any(|step| matches!(step, Step::Run(_)))
            {
                operation.emit(Event::Diagnostic {
                    level: DiagnosticLevel::Info,
                    message: "Building locally because every build step is safe".to_owned(),
                });
                path = context.paths.workspace_root.clone();
                subdir = context.paths.subdir.clone();
                build_environment = BuildEnvironment::Local;
                skip_safe = false;
                continue;
            }

            let mut serve_env = context.serve.env.take().unwrap_or_default();
            load_env_files(&context.paths, options.env_name.as_deref(), &mut serve_env);
            context.serve.env = Some(serve_env);
            if context
                .serve
                .commands
                .get("start")
                .is_none_or(String::is_empty)
            {
                bail!("No start command could be found, please provide a start command");
            }

            let plan = project_plan(&context);
            let build_steps = context
                .runner
                .borrow()
                .prepare_build_steps(context.serve.build.clone());
            operation.emit(build_plan_event(
                &build_steps,
                &context.serve,
                !options.skip_prepare,
            ));
            let env = build_process_env(&self.effective_env());
            context.build_backend.borrow_mut().build(
                &context.serve.name,
                &env,
                &self.env_overrides,
                context.serve.mounts.as_deref().unwrap_or(&[]),
                &build_steps,
            )?;
            build_volumes(
                &context.paths.workspace_root,
                &context.serve,
                Some(&context.anybuild_dir),
            )?;
            let artifact = context.runner.borrow_mut().build(&context.serve)?;
            if !options.skip_prepare
                && context
                    .serve
                    .prepare
                    .as_ref()
                    .is_some_and(|steps| !steps.is_empty())
            {
                operation.emit(Event::SectionStarted {
                    title: "Preparing".to_owned(),
                });
                context
                    .runner
                    .borrow_mut()
                    .prepare(&env, context.serve.prepare.as_deref().unwrap_or_default())?;
            }
            artifact.persist(&context.anybuild_dir)?;
            operation.emit(Event::ArtifactCreated {
                path: context.anybuild_dir.clone(),
            });
            return Ok(BuildOutcome {
                plan,
                state_dir: context.anybuild_dir,
                artifact,
            });
        }
    }

    fn run_inner(
        &self,
        options: RunOptions,
        operation: &OperationContext,
    ) -> AnyResult<RunOutcome> {
        let paths = self.paths()?;
        let environment = resolve_environment(
            &paths,
            &environment_options(&options.build_environment, &options.runtime_environment),
            operation,
        )?;
        let mut commands = options.commands.clone();
        if options.after_deploy && !commands.iter().any(|name| name == "after_deploy") {
            commands.push("after_deploy".to_owned());
        }
        if options.start && !commands.iter().any(|name| name == "start") {
            commands.push("start".to_owned());
        }
        let volume_specs: Vec<String> = options
            .volumes
            .iter()
            .map(|(name, path)| format!("{name}:{path}"))
            .collect();
        let mappings = merge_volume_mappings(&[
            load_volume_mappings(&paths.workspace_root, Some(&environment.anybuild_dir))?,
            parse_cli_volume_mappings(&volume_specs)?,
        ]);
        let mut outcome = RunOutcome::default();
        let env = IndexMap::from([(
            "PORT".to_owned(),
            options
                .serve_port
                .map(|port| port.to_string())
                .or_else(|| self.effective_env().get("PORT").cloned())
                .unwrap_or_else(|| "8080".to_owned()),
        )]);
        for command in commands {
            if matches!(command.as_str(), "start" | "after_deploy")
                && !environment.runner.borrow().has_serve_command(&command)
            {
                outcome.skipped.push(command);
                continue;
            }
            operation.emit(Event::CommandStarted {
                name: command.clone(),
                command: None,
            });
            environment.runner.borrow_mut().run_serve_command(
                &command,
                Some(&mappings),
                &[],
                Some(&env),
            )?;
            outcome.executed.push(command);
        }
        Ok(outcome)
    }

    fn deploy_inner(
        &self,
        options: DeployOptions,
        operation: &OperationContext,
    ) -> AnyResult<DeployOutcome> {
        let paths = self.paths()?;
        let anybuild_dir = resolve_anybuild_dir(&paths, operation)?;
        let mut deployer = resolve_deployer(options.platform, operation.clone());
        let artifact = match RuntimeArtifact::load(&anybuild_dir)? {
            Some(artifact) => artifact,
            None => deployer.load_legacy_artifact(&anybuild_dir)?,
        };
        anyhow::ensure!(
            deployer.accepts_artifact(&artifact),
            "{} deployment requires a {} artifact, found {:?}",
            deployer.platform_name(),
            deployer.artifact_requirement(),
            artifact.kind()
        );
        let outcome = deployer.deploy(&artifact, options.target)?;
        operation.emit(Event::Deployment {
            description: match &outcome {
                DeployOutcome::Published { .. } => {
                    format!("Published {} application", deployer.platform_name())
                }
                DeployOutcome::ConfigWritten { path } => {
                    format!("Wrote deployment config to {}", path.display())
                }
            },
        });
        Ok(outcome)
    }

    fn auto_inner(
        &self,
        mut options: AutoOptions,
        operation: &OperationContext,
    ) -> AnyResult<AutoOutcome> {
        let paths = self.paths()?;
        let definition = self.prepare_definition(
            &paths,
            options.build.anybuild_path.take(),
            options.generation,
            operation,
        )?;
        options.build.anybuild_path = definition.path.clone();
        let build = self.build_inner(options.build, operation)?;
        let run = options
            .run
            .map(|options| {
                let context = operation.with_process_io(options.process_io);
                self.run_inner(options, &context)
            })
            .transpose()?;
        let deploy = options
            .deploy
            .map(|options| {
                let context = operation.with_process_io(options.process_io);
                self.deploy_inner(options, &context)
            })
            .transpose()?;
        Ok(AutoOutcome {
            generated: definition.generated,
            build,
            run,
            deploy,
        })
    }

    fn prepare_definition(
        &self,
        paths: &ProjectPaths,
        explicit_path: Option<PathBuf>,
        policy: GenerationPolicy,
        operation: &OperationContext,
    ) -> AnyResult<PreparedDefinition> {
        if policy == GenerationPolicy::Temporary && explicit_path.is_some() {
            bail!("Cannot use both a temporary Anybuild file and an explicit path");
        }

        let temporary = if policy == GenerationPolicy::Temporary {
            Some(tempfile::Builder::new().prefix("Anybuild").tempfile()?)
        } else {
            None
        };
        let output = temporary
            .as_ref()
            .map(|file| file.path().to_path_buf())
            .or(explicit_path);
        let should_generate = match policy {
            GenerationPolicy::Always | GenerationPolicy::Temporary => true,
            GenerationPolicy::IfMissing => match &output {
                Some(path) => !path.exists(),
                None => migrate_legacy_anybuild(paths, operation)?.is_none(),
            },
        };
        let generated = should_generate
            .then(|| {
                self.generate_inner(
                    GenerateOptions {
                        output: output.clone(),
                    },
                    operation,
                )
            })
            .transpose()?;
        let path = generated
            .as_ref()
            .map(|generated| generated.path.clone())
            .or(output);
        Ok(PreparedDefinition {
            path,
            generated,
            _temporary: temporary,
        })
    }
}

fn environment_options(
    build: &BuildEnvironment,
    runtime: &RuntimeEnvironment,
) -> EnvironmentOptions {
    let (docker, docker_client, docker_opts) = match build {
        BuildEnvironment::Local => (false, None, None),
        BuildEnvironment::Docker(options) => {
            (true, options.client.clone(), options.extra_options.clone())
        }
    };
    let (
        docker_runner,
        docker_runner_client,
        docker_runner_opts,
        lambda_runner,
        wasmer,
        wasmer_bin,
        wasmer_registry,
        wasmer_token,
    ) = match runtime {
        RuntimeEnvironment::Local => (false, None, None, false, false, None, None, None),
        RuntimeEnvironment::Docker(options) => (
            true,
            options.client.clone(),
            options.extra_options.clone(),
            false,
            false,
            None,
            None,
            None,
        ),
        RuntimeEnvironment::Lambda(options) => (
            false,
            options.client.clone(),
            options.extra_options.clone(),
            true,
            false,
            None,
            None,
            None,
        ),
        RuntimeEnvironment::Wasmer(options) => (
            false,
            None,
            None,
            false,
            true,
            options.binary.clone(),
            options.registry.clone(),
            options.token.clone(),
        ),
    };
    EnvironmentOptions {
        wasmer,
        wasmer_bin,
        wasmer_registry,
        wasmer_token,
        docker_runner,
        docker_runner_client,
        docker_runner_opts,
        lambda_runner,
        docker,
        docker_client,
        docker_opts,
    }
}

fn config_differences(
    persisted: &ProviderConfigSnapshot,
    detected: &ProviderConfigSnapshot,
) -> Vec<ConfigDifference> {
    let mut differences = Vec::new();
    if persisted.provider != detected.provider {
        differences.push(ConfigDifference {
            path: "provider".to_owned(),
            persisted: Some(Value::String(persisted.provider.clone())),
            detected: Some(Value::String(detected.provider.clone())),
        });
    }
    if persisted.schema != detected.schema {
        differences.push(ConfigDifference {
            path: "schema".to_owned(),
            persisted: Some(Value::from(persisted.schema)),
            detected: Some(Value::from(detected.schema)),
        });
    }
    diff_values(
        "config",
        Some(&persisted.values),
        Some(&detected.values),
        &mut differences,
    );
    differences
}

fn diff_values(
    path: &str,
    persisted: Option<&Value>,
    detected: Option<&Value>,
    differences: &mut Vec<ConfigDifference>,
) {
    match (persisted, detected) {
        (Some(Value::Object(left)), Some(Value::Object(right))) => {
            let mut keys = left.keys().chain(right.keys()).collect::<Vec<_>>();
            keys.sort();
            keys.dedup();
            for key in keys {
                diff_values(
                    &format!("{path}.{key}"),
                    left.get(key),
                    right.get(key),
                    differences,
                );
            }
        }
        (left, right) if left == right => {}
        (left, right) => differences.push(ConfigDifference {
            path: path.to_owned(),
            persisted: left.cloned(),
            detected: right.cloned(),
        }),
    }
}

fn project_plan(context: &ProjectContext) -> ProjectPlan {
    let mut config = context.provider_config.clone();
    let collect_group = |group: &str| -> Option<String> {
        let commands: Vec<&str> = context
            .serve
            .build
            .iter()
            .filter_map(|step| match step {
                Step::Run(run) if run.group.as_deref() == Some(group) => Some(run.command.as_str()),
                _ => None,
            })
            .collect();
        (!commands.is_empty()).then(|| commands.join(" && "))
    };
    if let Some(start) = context.serve.commands.get("start") {
        config.base_mut().commands.start = Some(start.clone());
    }
    if let Some(after_deploy) = context.serve.commands.get("after_deploy") {
        config.base_mut().commands.after_deploy = Some(after_deploy.clone());
    }
    if let Some(install) = collect_group("install") {
        config.base_mut().commands.install = Some(install);
    }
    if let Some(build) = collect_group("build") {
        config.base_mut().commands.build = Some(build);
    }
    ProjectPlan {
        provider: context.provider.to_owned(),
        config: crate::providers::exclude_defaults_json(&config),
        services: context.serve.services.clone().unwrap_or_default(),
        serve: context.serve.clone(),
    }
}

fn build_plan_event(build: &[Step], serve: &Serve, include_prepare: bool) -> Event {
    type PackageKey = (String, Option<String>, Option<String>);
    let mut packages: IndexMap<PackageKey, (bool, bool)> = IndexMap::new();
    let mut mark_package = |package: &crate::plan::Package, build: bool, deploy: bool| {
        let key = (
            package.name.clone(),
            package.version.clone(),
            package.architecture.clone(),
        );
        let phases = packages.entry(key).or_insert((false, false));
        phases.0 |= build;
        phases.1 |= deploy;
    };

    for step in build {
        if let Step::Use(step) = step {
            for package in &step.dependencies {
                mark_package(package, true, false);
            }
        }
    }
    for package in &serve.deps {
        mark_package(package, false, true);
    }

    let packages = packages
        .into_iter()
        .map(
            |((name, version, architecture), (build, deploy))| BuildPlanPackage {
                name,
                version,
                architecture,
                phase: match (build, deploy) {
                    (true, true) => PackagePhase::Both,
                    (true, false) => PackagePhase::Build,
                    (false, true) => PackagePhase::Deploy,
                    (false, false) => unreachable!("a package belongs to at least one phase"),
                },
            },
        )
        .collect();

    let steps = build
        .iter()
        .filter_map(|step| match step {
            Step::Use(_) => None,
            Step::Run(step) => Some(BuildPlanStep::Run {
                command: step.command.clone(),
                group: step.group.clone(),
            }),
            Step::Copy(step) => Some(BuildPlanStep::Copy {
                source: step.source.clone(),
                target: step.target.clone(),
                base: step.base.clone(),
            }),
            Step::Env(step) => Some(BuildPlanStep::Environment {
                variables: step.variables.keys().cloned().collect(),
            }),
            Step::Path(step) => Some(BuildPlanStep::Path {
                path: step.path.clone(),
            }),
            Step::Workdir(step) => Some(BuildPlanStep::Workdir {
                path: step.path.clone(),
            }),
            Step::WriteFile(step) => Some(BuildPlanStep::WriteFile {
                path: step.path.clone(),
            }),
        })
        .collect();
    let deploy_scripts = serve
        .commands
        .iter()
        .map(|(name, command)| DeployScript {
            name: name.clone(),
            command: command.clone(),
        })
        .collect();
    let prepare_steps = if include_prepare {
        serve
            .prepare
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|step| BuildPlanStep::Run {
                command: step.command.clone(),
                group: step.group.clone(),
            })
            .collect()
    } else {
        Vec::new()
    };

    Event::BuildPlan {
        packages,
        steps,
        prepare_steps,
        deploy_scripts,
    }
}

fn load_env_files(
    paths: &ProjectPaths,
    env_name: Option<&str>,
    target: &mut IndexMap<String, String>,
) {
    let mut directories: Vec<&Path> = vec![&paths.workspace_root];
    if paths.subdir.is_some() {
        directories.push(&paths.app_path);
    }
    let mut names = vec![".env".to_owned()];
    if let Some(env_name) = env_name {
        names.push(format!(".env.{env_name}"));
    }
    for directory in directories {
        for name in &names {
            if let Ok(entries) = dotenvy::from_path_iter(directory.join(name)) {
                target.extend(entries.flatten());
            }
        }
    }
}

fn build_process_env(environment: &IndexMap<String, String>) -> IndexMap<String, String> {
    let mut result = environment.clone();
    result.insert("PATH".to_owned(), String::new());
    for name in ["LSCOLORS", "LS_COLORS", "CLICOLOR"] {
        result
            .entry(name.to_owned())
            .or_insert_with(|| "0".to_owned());
    }
    result.entry("COLORTERM".to_owned()).or_default();
    result
}
