//! Local runtime implementation.
//!
//! Console strings are verbatim (plain-text) ports of the Python
//! `console.print` calls; they go to stderr like rich's
//! `Console(stderr=True)`.

use std::cell::RefCell;
use std::path::PathBuf;
use std::process::Command;
use std::rc::Rc;

use anyhow::{bail, Context, Result};
use indexmap::IndexMap;

use crate::build::report::{console_print, print_panel};
use crate::build::BuildBackend;
use crate::operation::OperationContext;
use crate::plan::{RunStep, Serve, Step};
use crate::providers::ProviderConfig;

use crate::run::{HostMount, Runner};

pub struct LocalRunner {
    build_backend: Rc<RefCell<dyn BuildBackend>>,
    #[allow(dead_code)]
    src_dir: PathBuf,
    #[allow(dead_code)]
    anybuild_dir: PathBuf,
    runner_path: PathBuf,
    serve_bin_path: PathBuf,
    prepare_bash_script: PathBuf,
    operation: OperationContext,
}

impl LocalRunner {
    pub fn new(
        build_backend: Rc<RefCell<dyn BuildBackend>>,
        src_dir: PathBuf,
        anybuild_dir: Option<PathBuf>,
        operation: OperationContext,
    ) -> Self {
        let anybuild_dir = anybuild_dir.unwrap_or_else(|| src_dir.join(".anybuild"));
        let runner_path = anybuild_dir.join("runner").join("local");
        let serve_bin_path = runner_path.join("bin");
        let prepare_bash_script = runner_path.join("prepare").join("prepare.sh");
        Self {
            build_backend,
            src_dir,
            anybuild_dir,
            runner_path,
            serve_bin_path,
            prepare_bash_script,
            operation,
        }
    }

    fn build_prepare(&mut self, serve: &Serve) -> Result<()> {
        let prepare = match &serve.prepare {
            Some(steps) if !steps.is_empty() => steps,
            _ => return Ok(()),
        };
        std::fs::create_dir_all(self.prepare_bash_script.parent().unwrap())?;
        let mut commands: Vec<String> = Vec::new();
        if let Some(cwd) = &serve.cwd {
            commands.push(format!("cd {cwd}"));
        }
        for step in prepare {
            commands.push(step.command.clone());
        }
        let content = format!("#!/bin/bash\n{}", commands.join("\n"));
        console_print(
            &self.operation,
            "\nCreated prepare.sh script to run before packaging ✅",
        );
        print_panel(&self.operation, &content);
        std::fs::write(&self.prepare_bash_script, &content)?;
        set_executable(&self.prepare_bash_script)?;
        Ok(())
    }

    fn build_serve(&mut self, serve: &Serve) -> Result<()> {
        console_print(&self.operation, "\nBuilding serve");
        std::fs::create_dir_all(&self.serve_bin_path)?;
        let runtime_path = self
            .build_backend
            .borrow()
            .get_runtime_path()
            .unwrap_or_default();
        let path_prefix = if runtime_path.is_empty() {
            String::new()
        } else {
            format!("PATH={runtime_path}:$PATH ")
        };
        console_print(&self.operation, "Serve Commands:");
        for (command, command_body) in &serve.commands {
            console_print(&self.operation, &format!("* {command}"));
            let command_path = self.serve_bin_path.join(command);
            let env_vars = match &serve.env {
                Some(env) if !env.is_empty() => env
                    .iter()
                    .map(|(k, v)| format!("{k}={v}"))
                    .collect::<Vec<_>>()
                    .join(" "),
                _ => String::new(),
            };
            let mut lines: Vec<String> = vec!["#!/bin/bash".to_owned()];
            if let Some(cwd) = &serve.cwd {
                lines.push(format!("cd {cwd}"));
            }
            let cmd_body = format!("{path_prefix}{env_vars} {command_body}")
                .trim()
                .to_owned();
            lines.push(cmd_body);
            let content = format!("{}\n", lines.join("\n"));
            std::fs::write(&command_path, &content)?;
            print_panel(&self.operation, content.trim());
            set_executable(&command_path)?;
        }
        Ok(())
    }
}

fn set_executable(path: &std::path::Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

impl Runner for LocalRunner {
    fn as_any(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn prepare_config(&mut self, config: ProviderConfig) -> ProviderConfig {
        config
    }

    fn prepare_build_steps(&self, steps: Vec<Step>) -> Vec<Step> {
        steps
    }

    fn build(&mut self, serve: &Serve) -> Result<()> {
        match std::fs::remove_dir_all(&self.runner_path) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => {
                return Err(err)
                    .with_context(|| format!("Failed to clear {}", self.runner_path.display()));
            }
        }
        self.build_prepare(serve)?;
        self.build_serve(serve)?;
        Ok(())
    }

    fn prepare(&mut self, _env: &IndexMap<String, String>, _prepare: &[RunStep]) -> Result<()> {
        let mut command = Command::new(&self.prepare_bash_script);
        self.operation.prepare_command(&mut command);
        let status = self
            .operation
            .command_status(&mut command)
            .with_context(|| format!("Failed to run {}", self.prepare_bash_script.display()))?;
        if !status.success() {
            bail!(
                "Prepare script failed with exit code {}",
                status.code().unwrap_or(-1)
            );
        }
        Ok(())
    }

    fn has_serve_command(&self, command: &str) -> bool {
        self.serve_bin_path.join(command).is_file()
    }

    fn run_serve_command(
        &mut self,
        command: &str,
        _volume_mappings: Option<&IndexMap<String, String>>,
        _host_mounts: &[HostMount<'_>],
        env: Option<&IndexMap<String, String>>,
    ) -> Result<()> {
        let command_path = self.serve_bin_path.join(command);
        let mut process = Command::new(&command_path);
        self.operation.prepare_command(&mut process);
        if let Some(env) = env {
            process.envs(env.iter());
        }
        let status = self
            .operation
            .command_status(&mut process)
            .with_context(|| format!("Failed to run {}", command_path.display()))?;
        if !status.success() {
            bail!(
                "Command {command} failed with exit code {}",
                status.code().unwrap_or(-1)
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::build::local::LocalBuildBackend;

    use super::*;

    fn serve(prepare: Option<Vec<RunStep>>) -> Serve {
        Serve {
            name: "test".to_owned(),
            provider: "test".to_owned(),
            build: Vec::new(),
            deps: Vec::new(),
            commands: IndexMap::from([("start".to_owned(), "true".to_owned())]),
            cwd: None,
            prepare,
            mounts: None,
            volumes: None,
            env: None,
            services: None,
        }
    }

    #[test]
    fn prepare_script_survives_build_and_is_removed_when_stale() {
        let tmp = tempfile::tempdir().unwrap();
        let anybuild_dir = tmp.path().join(".anybuild");
        let backend: Rc<RefCell<dyn BuildBackend>> = Rc::new(RefCell::new(LocalBuildBackend::new(
            tmp.path().to_path_buf(),
            tmp.path().join("assets"),
            Some(anybuild_dir.clone()),
            OperationContext::for_test(),
        )));
        let mut runner = LocalRunner::new(
            backend,
            tmp.path().to_path_buf(),
            Some(anybuild_dir),
            OperationContext::for_test(),
        );
        let marker = tmp.path().join("prepared");
        let with_prepare = serve(Some(vec![RunStep {
            command: format!("touch {}", marker.display()),
            inputs: None,
            outputs: None,
            group: None,
        }]));

        runner.build(&with_prepare).unwrap();
        assert!(runner.prepare_bash_script.is_file());
        runner
            .prepare(&IndexMap::new(), with_prepare.prepare.as_deref().unwrap())
            .unwrap();
        assert!(marker.is_file());

        runner.build(&serve(None)).unwrap();
        assert!(!runner.prepare_bash_script.exists());
    }
}
