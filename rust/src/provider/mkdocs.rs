use std::path::Path;

use camino::Utf8PathBuf;

use crate::Result;
use crate::model::{CustomCommands, DependencySpec, DetectResult, MountSpec, ProviderPlan};
use crate::provider::{Provider, ProviderDescriptor, apply_custom_commands};

pub struct MkdocsProvider {
    path: Utf8PathBuf,
    custom_commands: CustomCommands,
}

impl MkdocsProvider {
    pub fn new(path: &Path, custom: &CustomCommands) -> Result<Self> {
        let path_buf = Utf8PathBuf::from_path_buf(path.to_path_buf())
            .map_err(|_| anyhow::anyhow!("Invalid UTF-8 path"))?;
        Ok(Self {
            path: path_buf,
            custom_commands: custom.clone(),
        })
    }
}

impl Provider for MkdocsProvider {
    fn name(&self) -> &'static str {
        "mkdocs"
    }

    fn plan(&self) -> Result<ProviderPlan> {
        let mut mounts = vec![MountSpec {
            name: "app".to_string(),
            attach_to_build: true,
            attach_to_serve: true,
        }];
        mounts.push(MountSpec {
            name: "temp".to_string(),
            attach_to_build: true,
            attach_to_serve: false,
        });
        // Include `local_venv` mount so generated Shipit declarations (from the
        // Python provider) which reference `local_venv` are valid at runtime.
        mounts.push(MountSpec {
            name: "local_venv".to_string(),
            attach_to_build: true,
            attach_to_serve: false,
        });

        let mut deps = vec![
            DependencySpec {
                name: "python".to_string(),
                env_var: Some("SHIPIT_PYTHON_VERSION".to_string()),
                default_version: Some("3.13".to_string()),
                architecture_var: None,
                alias: None,
                use_in_build: true,
                use_in_serve: true,
            },
            DependencySpec {
                name: "static-web-server".to_string(),
                env_var: Some("SHIPIT_SWS_VERSION".to_string()),
                default_version: Some("2.38.0".to_string()),
                architecture_var: None,
                alias: None,
                use_in_build: false,
                use_in_serve: true,
            },
        ];
        deps.push(DependencySpec {
            name: "mkdocs".to_string(),
            env_var: Some("SHIPIT_MKDOCS_VERSION".to_string()),
            default_version: Some("1.6.1".to_string()),
            architecture_var: None,
            alias: None,
            use_in_build: true,
            use_in_serve: false,
        });

        // Add dependencies from requirements.txt if it exists
        let requirements_path = self.path.join("requirements.txt");
        if requirements_path.exists() {
            let req_content = std::fs::read_to_string(&requirements_path)
                .map_err(|e| anyhow::anyhow!("Failed to read requirements.txt: {}", e))?;
            for line in req_content.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                let parts: Vec<&str> = line.split("==").collect();
                if parts.len() == 2 {
                    let name = parts[0].to_string();
                    let version = parts[1].to_string();
                    if name != "mkdocs" {
                        deps.push(DependencySpec {
                            env_var: Some(format!(
                                "SHIPIT_{}_VERSION",
                                name.to_uppercase().replace("-", "_")
                            )),
                            name,
                            default_version: Some(version),
                            architecture_var: None,
                            alias: None,
                            use_in_build: true,
                            use_in_serve: false,
                        });
                    }
                }
            }
        }

        let mut commands = std::collections::BTreeMap::new();
        commands.insert(
            "start".to_string(),
            "\"static-web-server --root={} --log-level=info --port={}\".format(app[\"serve\"], PORT)".to_string(),
        );
        let commands = apply_custom_commands(&self.custom_commands, commands)?;

        Ok(ProviderPlan {
            serve_name: self
                .path
                .file_name()
                .map(|s| s.to_string())
                .unwrap_or_else(|| "app".to_string()),
            cwd: None,
            provider: self.name().to_string(),
            mounts,
            platform: None,
            volumes: Vec::new(),
            // Compose declarations from the Python provider so Python-related
            // variables (like `cross_platform`, `venv`, etc.) are available in
            // the generated Shipit file for mkdocs build steps. If we can't
            // construct the Python provider for some reason, fall back to the
            // simple mkdocs_version declaration.
            declarations: {
                let mut decls = String::new();
                if let Ok(py) = crate::provider::python::PythonProvider::with_options(
                    std::path::Path::new(self.path.as_str()),
                    &self.custom_commands,
                    true,
                    Some({
                        let mut s = std::collections::HashSet::new();
                        s.insert("mkdocs".to_string());
                        s
                    }),
                ) {
                    if let Some(d) = py.declarations() {
                        decls.push_str(&d);
                    }
                }
                if !decls.is_empty() {
                    decls.push_str("mkdocs_version = getenv(\"SHIPIT_MKDOCS_VERSION\") or \"1.6.1\"\n");
                    Some(decls)
                } else {
                    Some("mkdocs_version = getenv(\"SHIPIT_MKDOCS_VERSION\") or \"1.6.1\"".to_string())
                }
            },
            dependencies: deps,
            build_steps: {
                // Compose build steps from the Python provider so Python-specific
                // install steps (uv add / uv sync / uv add -r requirements.txt)
                // are included (this ensures mkdocs and any plugins in
                // requirements.txt are installed during local builds).
                let mut python_steps: Vec<String> = Vec::new();
                // Construct a PythonProvider configured for build-only mode and
                // request `mkdocs` as an extra dependency so the provider emits
                // the proper `uv add` / `uv sync` steps to install mkdocs and
                // any declared plugins.
                if let Ok(py) = crate::provider::python::PythonProvider::with_options(
                    std::path::Path::new(self.path.as_str()),
                    &self.custom_commands,
                    true,
                    Some({
                        let mut s = std::collections::HashSet::new();
                        s.insert("mkdocs".to_string());
                        s
                    }),
                ) {
                    python_steps = py.build_steps();
                    // Normalize workdir to use the temporary build mount (mkdocs
                    // expects to build in temp["build"]). If the python provider
                    // produced a workdir for app["build"], convert it to temp.
                    if !python_steps.is_empty() {
                        if python_steps[0] == "workdir(app[\"build\"])".to_string() {
                            python_steps[0] = "workdir(temp[\"build\"])".to_string();
                        }
                    }
                } else {
                    // Fallback: ensure we at least set the workdir to temp build.
                    python_steps.push("workdir(temp[\"build\"])".to_string());
                }
                // Ensure the source is copied into the build directory and then
                // invoke mkdocs using the installed uv environment.
                python_steps.push("copy(\".\", \".\", ignore=[\".git\"])".to_string());
                python_steps.push("run(\"uv run mkdocs build --site-dir={}\".format(app[\"build\"]), outputs=[\".\"], group=\"build\")".to_string());
                python_steps
            },
            prepare: None,
            services: Vec::new(),
            commands,
            env: None,
        })
    }
}

pub struct MkdocsDescriptor;

impl ProviderDescriptor for MkdocsDescriptor {
    fn detect(&self, path: &Path, custom: &CustomCommands) -> Option<DetectResult> {
        if exists(path, &["mkdocs.yml", "mkdocs.yaml"]) {
            return Some(DetectResult {
                name: "mkdocs".to_string(),
                score: 85,
            });
        }
        if let Some(build) = &custom.build {
            if build.starts_with("mkdocs ") {
                return Some(DetectResult {
                    name: "mkdocs".to_string(),
                    score: 85,
                });
            }
        }
        None
    }

    fn create(&self, path: &Path, custom: &CustomCommands) -> Result<Box<dyn Provider>> {
        Ok(Box::new(MkdocsProvider::new(path, custom)?))
    }

    fn name(&self) -> &'static str {
        "mkdocs"
    }
}

fn exists(path: &Path, names: &[&str]) -> bool {
    names.iter().any(|n| path.join(n).exists())
}
