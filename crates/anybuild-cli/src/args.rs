//! Shared clap argument groups, composed into the command arg structs with
//! `#[command(flatten)]`.
//!
//! Groups are sized to the *exact* subsets the Python CLI shares between
//! commands — surfaces are deliberately non-uniform (run takes no
//! --wasmer-token, deploy takes no --docker), and that parity is preserved
//! by composing small groups rather than one flag superset. AutoArgs
//! additionally flattens the whole of BuildArgs, which is a strict subset
//! of auto's surface.

use std::path::PathBuf;

/// Positional project path + `--subdir` (every command).
#[derive(clap::Args, Debug, Clone, Default)]
pub struct ProjectArgs {
    /// Project path (defaults to current directory).
    #[arg(default_value = ".")]
    pub path: PathBuf,
    /// App subdirectory relative to the project path.
    #[arg(long)]
    pub subdir: Option<String>,
}

/// Wasmer connection settings shared by `build`, `deploy` (and `auto`
/// through BuildArgs). `run` deliberately has no `--wasmer-token`.
#[derive(clap::Args, Debug, Clone, Default)]
pub struct WasmerConnArgs {
    /// The path to the Wasmer binary.
    #[arg(long)]
    pub wasmer_bin: Option<String>,
    /// Wasmer registry.
    #[arg(long)]
    pub wasmer_registry: Option<String>,
    /// Wasmer token.
    #[arg(long)]
    pub wasmer_token: Option<String>,
}

/// Command selection shared by `run` and `auto`.
#[derive(clap::Args, Debug, Clone, Default)]
pub struct RunSelectionArgs {
    /// Run one or more commands. Can be passed multiple times.
    #[arg(short = 'c', long = "command")]
    pub command_names: Vec<String>,
    /// Attach one or more volumes as NAME:/guest/path. Can be passed multiple times.
    #[arg(long = "volume")]
    pub volume_specs: Vec<String>,
    /// Equivalent to `--command=start`.
    #[arg(long, overrides_with = "no_start")]
    pub start: bool,
    #[arg(long = "no-start", hide = true)]
    pub no_start: bool,
    /// Equivalent to `--command=after_deploy`.
    #[arg(long, overrides_with = "no_after_deploy")]
    pub after_deploy: bool,
    #[arg(long = "no-after-deploy", hide = true)]
    pub no_after_deploy: bool,
}

impl RunSelectionArgs {
    pub fn effective_start(&self) -> bool {
        self.start && !self.no_start
    }

    pub fn effective_after_deploy(&self) -> bool {
        self.after_deploy && !self.no_after_deploy
    }
}

/// Deploy target shared by `deploy` and `auto`. The `--wasmer-deploy` flag
/// itself stays per-command: it defaults True on `deploy` but False on
/// `auto`, exactly like the Python CLI.
#[derive(clap::Args, Debug, Clone, Default)]
pub struct DeployTargetArgs {
    /// Save the output of the Wasmer build to a json file
    #[arg(long)]
    pub wasmer_deploy_config: Option<PathBuf>,
    /// Override the owner of the Wasmer app (otherwise Wasmer prompts).
    #[arg(long)]
    pub wasmer_app_owner: Option<String>,
    /// Override the name of the Wasmer app (otherwise Wasmer prompts).
    #[arg(long)]
    pub wasmer_app_name: Option<String>,
}
