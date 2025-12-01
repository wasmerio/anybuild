use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::Path;

use camino::Utf8PathBuf;
use regex::Regex;
use walkdir::WalkDir;

use crate::Result;
use crate::model::{
    CustomCommands, DependencySpec, DetectResult, MountSpec, ProviderPlan, ServiceProvider,
    ServiceSpec,
};
use crate::provider::{Provider, ProviderDescriptor, apply_custom_commands};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PythonFramework {
    Django,
    Streamlit,
    FastAPI,
    Flask,
    FastHTML,
    MCP,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PythonServer {
    Hypercorn,
    Uvicorn,
    Daphne,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DatabaseType {
    MySQL,
    PostgreSQL,
}

pub struct PythonProvider {
    path: Utf8PathBuf,
    custom_commands: CustomCommands,
    framework: Option<PythonFramework>,
    server: Option<PythonServer>,
    database: Option<DatabaseType>,
    extra_dependencies: HashSet<String>,
    asgi_application: Option<String>,
    wsgi_application: Option<String>,
    uses_ffmpeg: bool,
    uses_pandoc: bool,
    only_build: bool,
    install_requires_all_files: bool,
    default_python_version: String,
    main_file: Option<String>,
}

impl PythonProvider {
    /// Construct a PythonProvider while configuring a couple of commonly toggled
    /// options (`only_build` and `extra_dependencies`). Providers which compose
    /// PythonProvider (e.g. Mkdocs) can use this helper to request a build-only
    /// Python plan and to inject extra dependencies (like `mkdocs`) that should
    /// be installed during the Python install steps.
    pub fn with_options(
        path: &Path,
        custom_commands: &CustomCommands,
        only_build: bool,
        extra_dependencies: Option<HashSet<String>>,
    ) -> Result<Self> {
        let mut provider = PythonProvider::new(path, custom_commands)?;
        provider.only_build = only_build;
        if let Some(extra) = extra_dependencies {
            provider.extra_dependencies = extra;
        }
        Ok(provider)
    }
    pub fn new(path: &Path, custom_commands: &CustomCommands) -> Result<Self> {
        let path_buf = Utf8PathBuf::from_path_buf(path.to_path_buf())
            .map_err(|_| anyhow::anyhow!("Invalid UTF-8 path"))?;
        let python_version = if path_buf.join(".python-version").exists() {
            fs::read_to_string(path_buf.join(".python-version"))?
                .trim()
                .to_string()
        } else {
            "3.13".to_string()
        };

        let mut extra_dependencies = HashSet::new();
        let mut custom_commands = custom_commands.clone();
        let only_build = false;
        let install_requires_all_files;
        let mut uses_ffmpeg = false;
        let mut uses_pandoc = false;
        let mut server = None;
        let mut framework = None;
        let mut asgi_application = None;
        let mut wsgi_application = None;
        let mut database = None;

        let found_deps = {
            let deps = [
                "file://",
                "streamlit",
                "django",
                "mcp",
                "fastapi",
                "flask",
                "python-fasthtml",
                "daphne",
                "hypercorn",
                "uvicorn",
                "ffmpeg",
                "pandoc",
                "mysqlclient",
                "pymysql",
                "mysql-connector-python",
                "aiomysql",
                "asyncmy",
                "asyncpg",
                "aiopg",
                "psycopg",
                "psycopg2",
                "psycopg-binary",
                "psycopg2-binary",
            ];
            Self::check_deps(&path_buf, &deps)
        };

        install_requires_all_files = found_deps.contains("file://");
        if found_deps.contains("uvicorn") {
            server = Some(PythonServer::Uvicorn);
        } else if found_deps.contains("hypercorn") {
            server = Some(PythonServer::Hypercorn);
        } else if found_deps.contains("daphne") {
            server = Some(PythonServer::Daphne);
        }
        if found_deps.contains("ffmpeg") {
            uses_ffmpeg = true;
        }
        if found_deps.contains("pandoc") {
            uses_pandoc = true;
        }

        if let Some(start_val) = custom_commands.start.clone() {
            if start_val.starts_with("uvicorn ") {
                server = Some(PythonServer::Uvicorn);
                custom_commands.start =
                    Some(start_val.replacen("uvicorn ", "python -m uvicorn ", 1));
                extra_dependencies.insert("uvicorn".to_string());
            } else if start_val.starts_with("uv ") {
                server = Some(PythonServer::Uvicorn);
            }
        }

        // Framework detection
        if path_buf.join("manage.py").exists() && found_deps.contains("django") {
            framework = Some(PythonFramework::Django);
            if let Some(settings_file) = find_settings_file(&path_buf) {
                let contents = fs::read_to_string(&settings_file).unwrap_or_default();
                if let Some(cap) = Regex::new(r#"ASGI_APPLICATION\s*=\s*['"](.+)['"]"#)
                    .unwrap()
                    .captures(&contents)
                {
                    asgi_application = Some(cap[1].to_string());
                } else if let Some(cap) = Regex::new(r#"WSGI_APPLICATION\s*=\s*['"](.+)['"]"#)
                    .unwrap()
                    .captures(&contents)
                {
                    wsgi_application = Some(cap[1].to_string());
                }
            }
            if server.is_none() {
                if asgi_application.is_some() {
                    extra_dependencies.insert("uvicorn".to_string());
                    server = Some(PythonServer::Uvicorn);
                } else if wsgi_application.is_some() {
                    extra_dependencies.insert("uvicorn".to_string());
                    server = Some(PythonServer::Uvicorn);
                }
            }
        } else if found_deps.contains("streamlit") {
            framework = Some(PythonFramework::Streamlit);
        } else if found_deps.contains("mcp") {
            framework = Some(PythonFramework::MCP);
            extra_dependencies.insert("mcp[cli]".to_string());
        } else if found_deps.contains("fastapi") {
            framework = Some(PythonFramework::FastAPI);
            if server.is_none() {
                extra_dependencies.insert("uvicorn".to_string());
                server = Some(PythonServer::Uvicorn);
            }
        } else if found_deps.contains("flask") {
            framework = Some(PythonFramework::Flask);
            if server.is_none() {
                extra_dependencies.insert("uvicorn".to_string());
                server = Some(PythonServer::Uvicorn);
            }
        } else if found_deps.contains("python-fasthtml") {
            framework = Some(PythonFramework::FastHTML);
        }

        let mysql_deps: HashSet<&str> = [
            "mysqlclient",
            "pymysql",
            "mysql-connector-python",
            "aiomysql",
            "asyncmy",
        ]
        .into_iter()
        .collect();
        let pg_deps: HashSet<&str> = [
            "asyncpg",
            "aiopg",
            "psycopg",
            "psycopg2",
            "psycopg-binary",
            "psycopg2-binary",
        ]
        .into_iter()
        .collect();
        if mysql_deps.iter().any(|d| found_deps.contains(*d)) {
            database = Some(DatabaseType::MySQL);
        } else if pg_deps.iter().any(|d| found_deps.contains(*d)) {
            database = Some(DatabaseType::PostgreSQL);
        }

        let main_file = Self::detect_main_file(&path_buf);

        Ok(Self {
            path: path_buf,
            custom_commands,
            framework,
            server,
            database,
            extra_dependencies,
            asgi_application,
            wsgi_application,
            uses_ffmpeg,
            uses_pandoc,
            only_build,
            install_requires_all_files,
            default_python_version: python_version,
            main_file,
        })
    }

    fn check_deps(path: &Utf8PathBuf, deps: &[&str]) -> HashSet<String> {
        let mut remaining: HashSet<String> = deps.iter().map(|d| d.to_lowercase()).collect();
        let mut found = HashSet::new();
        for file in ["requirements.txt", "pyproject.toml"] {
            let f = path.join(file);
            if !f.exists() {
                continue;
            }
            if let Ok(contents) = fs::read_to_string(&f) {
                for line in contents.lines() {
                    let lower = line.to_lowercase();
                    let matches: Vec<String> = remaining
                        .iter()
                        .filter(|dep| lower.contains(dep.as_str()))
                        .cloned()
                        .collect();
                    for dep in matches {
                        remaining.remove(&dep);
                        found.insert(dep);
                    }
                    if remaining.is_empty() {
                        return deps.iter().map(|d| d.to_lowercase()).collect();
                    }
                }
            }
        }
        found
    }

    fn detect_main_file(root: &Utf8PathBuf) -> Option<String> {
        let candidates = ["main.py", "app.py", "streamlit_app.py", "Home.py"];
        for path in &candidates {
            if root.join(path).exists() {
                return Some(path.to_string());
            }
            if root.join("src").join(path).exists() {
                return Some(format!("src/{path}"));
            }
        }
        for entry in WalkDir::new(root) {
            let entry = entry.ok()?;
            if !entry.file_type().is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy();
            if name.ends_with("_app.py") || candidates.contains(&name.as_ref()) {
                let rel = entry
                    .path()
                    .strip_prefix(root.as_std_path())
                    .ok()?
                    .to_string_lossy()
                    .to_string();
                return Some(rel);
            }
        }
        None
    }

    fn framework_platform(&self) -> Option<&str> {
        self.framework.map(|f| match f {
            PythonFramework::Django => "django",
            PythonFramework::Streamlit => "streamlit",
            PythonFramework::FastAPI => "fastapi",
            PythonFramework::Flask => "flask",
            PythonFramework::FastHTML => "python-fasthtml",
            PythonFramework::MCP => "mcp",
        })
    }

    pub fn declarations(&self) -> Option<String> {
        if self.only_build {
            return Some(
                "cross_platform = getenv(\"SHIPIT_PYTHON_CROSS_PLATFORM\")\nvenv = local_venv\n"
                    .to_string(),
            );
        }
        Some(
            "cross_platform = getenv(\"SHIPIT_PYTHON_CROSS_PLATFORM\")\n\
             python_extra_index_url = getenv(\"SHIPIT_PYTHON_EXTRA_INDEX_URL\")\n\
             precompile_python = getenv(\"SHIPIT_PYTHON_PRECOMPILE\") in [\"true\", \"True\", \"TRUE\", \"1\", \"on\", \"yes\", \"y\", \"Y\", \"YES\", \"On\", \"ON\"]\n\
             python_cross_packages_path = venv[\"build\"] + f\"/lib/python{python_version}/site-packages\"\n\
             python_serve_site_packages_path = \"{}/lib/python{}/site-packages\".format(venv[\"serve\"], python_version)\n\
             app_serve_path = app[\"serve\"]\n"
                .to_string(),
        )
    }

    fn dependencies(&self) -> Vec<DependencySpec> {
        let mut deps = vec![
            DependencySpec {
                name: "python".to_string(),
                env_var: Some("SHIPIT_PYTHON_VERSION".to_string()),
                default_version: Some(self.default_python_version.clone()),
                architecture_var: None,
                alias: None,
                use_in_build: true,
                use_in_serve: true,
            },
            DependencySpec {
                name: "uv".to_string(),
                env_var: Some("SHIPIT_UV_VERSION".to_string()),
                default_version: Some("0.8.15".to_string()),
                architecture_var: None,
                alias: None,
                use_in_build: true,
                use_in_serve: false,
            },
        ];
        if self.uses_pandoc {
            deps.push(DependencySpec {
                name: "pandoc".to_string(),
                env_var: Some("SHIPIT_PANDOC_VERSION".to_string()),
                default_version: None,
                architecture_var: None,
                alias: None,
                use_in_build: false,
                use_in_serve: true,
            });
        }
        if self.uses_ffmpeg {
            deps.push(DependencySpec {
                name: "ffmpeg".to_string(),
                env_var: Some("SHIPIT_FFMPEG_VERSION".to_string()),
                default_version: None,
                architecture_var: None,
                alias: None,
                use_in_build: false,
                use_in_serve: true,
            });
        }
        deps
    }

    pub fn build_steps(&self) -> Vec<String> {
        let mut steps = if self.only_build {
            vec!["workdir(temp[\"build\"])".to_string()]
        } else {
            vec!["workdir(app[\"build\"])".to_string()]
        };
        let extra_deps: Vec<String> = self.extra_dependencies.iter().cloned().collect();
        let extra_deps_str = extra_deps.join(" ");
        let has_requirements = self.path.join("requirements.txt").exists();

        if self.path.join("pyproject.toml").exists() {
            let mut input_files = vec!["pyproject.toml".to_string()];
            let mut extra_args = String::new();
            if self.path.join("uv.lock").exists() {
                input_files.push("uv.lock".to_string());
                extra_args = " --locked".to_string();
            }
            for glob in ["README", "LICENSE", "LICENCE", "MAINTAINERS", "AUTHORS"] {
                for entry in glob_variants(&self.path, glob) {
                    input_files.push(entry);
                }
            }
            let inputs = input_files
                .iter()
                .map(|s| serde_json::to_string(s).unwrap())
                .collect::<Vec<_>>()
                .join(", ");
            steps.push("env(UV_PROJECT_ENVIRONMENT=local_venv[\"build\"] if cross_platform else venv[\"build\"], UV_PYTHON_PREFERENCE=\"only-system\", UV_PYTHON=f\"python{python_version}\")".to_string());
            if self.install_requires_all_files {
                steps.push("copy(\".\", \".\")".to_string());
            }
            steps.push(format!(
                "run(f\"uv sync{extra_args}\", inputs=[{inputs}], group=\"install\")"
            ));
            if !self.install_requires_all_files {
                steps.push("copy(\"pyproject.toml\", \"pyproject.toml\")".to_string());
            }
            if !extra_deps_str.is_empty() {
                steps.push(format!(
                    "run(\"uv add {extra_deps_str}\", group=\"install\")"
                ));
            }
            if !self.only_build {
                steps.push("run(f\"uv pip compile pyproject.toml --universal --extra-index-url {python_extra_index_url} --index-url=https://pypi.org/simple --emit-index-url --no-deps -o cross-requirements.txt\", outputs=[\"cross-requirements.txt\"]) if cross_platform else None".to_string());
                steps.push(format!("run(f\"uvx pip install -r cross-requirements.txt {extra_deps_str} --target {{python_cross_packages_path}} --platform {{cross_platform}} --only-binary=:all: --python-version={{python_version}} --compile\") if cross_platform else None"));
                steps.push(
                    "run(\"rm cross-requirements.txt\") if cross_platform else None".to_string(),
                );
            }
        } else if has_requirements || !extra_deps_str.is_empty() {
            steps.push(
                "env(UV_PROJECT_ENVIRONMENT=local_venv[\"build\"] if cross_platform else venv[\"build\"])"
                    .to_string(),
            );
            steps.push(
                "run(\"uv init\", inputs=[], outputs=[\"uv.lock\"], group=\"install\")".to_string(),
            );
            if self.install_requires_all_files {
                steps.push(
                    "copy(\".\", \".\", ignore=[\".venv\", \".git\", \"__pycache__\"])".to_string(),
                );
            }
            if has_requirements {
                steps.push(format!(
                    "run(\"uv add -r requirements.txt {extra_deps_str}\", inputs=[\"requirements.txt\"], group=\"install\")"
                ));
            } else if !extra_deps_str.is_empty() {
                steps.push(format!(
                    "run(\"uv add {extra_deps_str}\", group=\"install\")"
                ));
            }
            if !self.only_build {
                steps.push("run(f\"uv pip compile requirements.txt --python-version={python_version} --universal --extra-index-url {python_extra_index_url} --index-url=https://pypi.org/simple --emit-index-url --no-deps -o cross-requirements.txt\", inputs=[\"requirements.txt\"], outputs=[\"cross-requirements.txt\"]) if cross_platform else None".to_string());
                steps.push(format!("run(f\"uvx pip install -r cross-requirements.txt {extra_deps_str} --target {{python_cross_packages_path}} --platform {{cross_platform}} --only-binary=:all: --python-version={{python_version}} --compile\") if cross_platform else None"));
                steps.push(
                    "run(\"rm cross-requirements.txt\") if cross_platform else None".to_string(),
                );
            }
        }

        steps.push(
            "path((local_venv[\"build\"] if cross_platform else venv[\"build\"]) + \"/bin\")"
                .to_string(),
        );
        if !self.install_requires_all_files {
            steps.push(
                "copy(\".\", \".\", ignore=[\".venv\", \".git\", \"__pycache__\"])".to_string(),
            );
        }
        if self.framework == Some(PythonFramework::MCP) {
            steps.push(
                "run(\"mkdir -p {}/bin\".format(venv[\"build\"])) if cross_platform else None"
                    .to_string(),
            );
            steps.push("run(\"cp {}/bin/mcp {}/bin/mcp\".format(local_venv[\"build\"], venv[\"build\"])) if cross_platform else None".to_string());
        }
        if self.framework == Some(PythonFramework::Django) {
            steps.push(
                "run(\"python manage.py collectstatic --noinput\", group=\"build\")".to_string(),
            );
        }
        steps
    }

    fn prepare_steps(&self) -> Option<Vec<String>> {
        if self.only_build {
            return Some(Vec::new());
        }
        Some(vec![
            "run(\"echo \\\"Precompiling Python code...\\\"\") if precompile_python else None"
                .to_string(),
            "run(f\"python -m compileall -o 2 {python_serve_site_packages_path}\") if precompile_python else None"
                .to_string(),
            "run(\"echo \\\"Precompiling package code...\\\"\") if precompile_python else None"
                .to_string(),
            "run(f\"python -m compileall -o 2 {app_serve_path}\") if precompile_python else None"
                .to_string(),
        ])
    }

    fn base_commands(&self) -> BTreeMap<String, String> {
        let mut commands = BTreeMap::new();
        if self.only_build {
            return commands;
        }
        if self.framework == Some(PythonFramework::Django) {
            let start_cmd = if self.server == Some(PythonServer::Daphne) {
                if let Some(app) = &self.asgi_application {
                    format!(
                        "f\"python -m daphne {} --bind 0.0.0.0 --port {{PORT}}\"",
                        format_app_import(app)
                    )
                } else {
                    "f\"python manage.py runserver 0.0.0.0:{PORT}\"".to_string()
                }
            } else if self.server == Some(PythonServer::Uvicorn) {
                if let Some(app) = &self.asgi_application {
                    format!(
                        "f\"python -m uvicorn {} --host 0.0.0.0 --port {{PORT}}\"",
                        format_app_import(app)
                    )
                } else if let Some(wsgi) = &self.wsgi_application {
                    format!(
                        "f\"python -m uvicorn {} --interface=wsgi --host 0.0.0.0 --port {{PORT}}\"",
                        format_app_import(wsgi)
                    )
                } else {
                    "f\"python manage.py runserver 0.0.0.0:{PORT}\"".to_string()
                }
            } else {
                "f\"python manage.py runserver 0.0.0.0:{PORT}\"".to_string()
            };
            commands.insert("start".to_string(), start_cmd);
            commands.insert(
                "after_deploy".to_string(),
                "\"python manage.py migrate\"".to_string(),
            );
            return commands;
        }

        let main_file = match &self.main_file {
            Some(file) => file.clone(),
            None => {
                commands.insert(
                    "start".to_string(),
                    "\"python -c 'print(\\\"No start command detected, please provide a start command manually\\\")'\"".to_string(),
                );
                return commands;
            }
        };

        match self.framework {
            Some(PythonFramework::FastAPI) => {
                let path = format!("{}:app", file_to_python_path(&main_file));
                let start_cmd = if self.server == Some(PythonServer::Uvicorn) {
                    format!("f\"python -m uvicorn {path} --host 0.0.0.0 --port {{PORT}}\"")
                } else if self.server == Some(PythonServer::Hypercorn) {
                    format!("f\"python -m hypercorn {path} --bind 0.0.0.0:{{PORT}}\"")
                } else {
                    "\"python -c 'print(\\\"No start command detected, please provide a start command manually\\\")'\"".to_string()
                };
                commands.insert("start".to_string(), start_cmd);
            }
            Some(PythonFramework::Streamlit) => {
                commands.insert(
                    "start".to_string(),
                    format!("f\"python -m streamlit run {main_file} --server.port {{PORT}} --server.address 0.0.0.0 --server.headless true\""),
                );
            }
            Some(PythonFramework::Flask) => {
                let path = format!("{}:app", file_to_python_path(&main_file));
                commands.insert(
                    "start".to_string(),
                    format!(
                        "f\"python -m uvicorn {path} --interface=wsgi --host 0.0.0.0 --port {{PORT}}\""
                    ),
                );
            }
            Some(PythonFramework::MCP) => {
                let contents = fs::read_to_string(self.path.join(&main_file)).unwrap_or_default();
                if contents.contains("if __name__ == \"__main__\"") || contents.contains("mcp.run")
                {
                    commands.insert("start".to_string(), format!("\"python {main_file}\""));
                } else {
                    commands.insert(
                        "start".to_string(),
                        "f\"python {}/bin/mcp run {main_file} --transport=streamable-http\".format(venv[\"serve\"])".
                        to_string(),
                    );
                }
            }
            Some(PythonFramework::FastHTML) => {
                let path = format!("{}:app", file_to_python_path(&main_file));
                commands.insert(
                    "start".to_string(),
                    format!("f\"python -m uvicorn {path} --host 0.0.0.0 --port {{PORT}}\""),
                );
            }
            _ => {
                commands.insert("start".to_string(), format!("\"python {main_file}\""));
            }
        }
        commands
    }

    fn commands(&self) -> Result<BTreeMap<String, String>> {
        apply_custom_commands(&self.custom_commands, self.base_commands())
    }

    fn mounts(&self) -> Vec<MountSpec> {
        if self.only_build {
            return vec![
                MountSpec {
                    name: "temp".to_string(),
                    attach_to_build: true,
                    attach_to_serve: false,
                },
                MountSpec {
                    name: "local_venv".to_string(),
                    attach_to_build: true,
                    attach_to_serve: false,
                },
            ];
        }
        vec![
            MountSpec {
                name: "app".to_string(),
                attach_to_build: true,
                attach_to_serve: true,
            },
            MountSpec {
                name: "venv".to_string(),
                attach_to_build: true,
                attach_to_serve: true,
            },
            MountSpec {
                name: "local_venv".to_string(),
                attach_to_build: true,
                attach_to_serve: false,
            },
        ]
    }

    fn env_map(&self) -> Option<BTreeMap<String, String>> {
        if self.only_build {
            return Some(BTreeMap::new());
        }
        let mut env_vars = BTreeMap::new();
        let python_path = if let Some(main) = &self.main_file {
            if main.starts_with("src/") {
                "f\"{app_serve_path}:{app_serve_path}/src:{python_serve_site_packages_path}\""
                    .to_string()
            } else {
                "f\"{app_serve_path}:{python_serve_site_packages_path}\"".to_string()
            }
        } else {
            "f\"{app_serve_path}:{python_serve_site_packages_path}\"".to_string()
        };
        env_vars.insert("PYTHONPATH".to_string(), python_path);
        env_vars.insert("HOME".to_string(), "app[\"serve\"]".to_string());
        if self.framework == Some(PythonFramework::Streamlit) {
            env_vars.insert(
                "STREAMLIT_SERVER_HEADLESS".to_string(),
                "\"true\"".to_string(),
            );
        } else if self.framework == Some(PythonFramework::MCP) {
            env_vars.insert("FASTMCP_HOST".to_string(), "\"0.0.0.0\"".to_string());
            env_vars.insert("FASTMCP_PORT".to_string(), "PORT".to_string());
        }
        Some(env_vars)
    }

    fn services(&self) -> Vec<ServiceSpec> {
        match self.database {
            Some(DatabaseType::MySQL) => vec![ServiceSpec {
                name: "database".to_string(),
                provider: ServiceProvider::Mysql,
            }],
            Some(DatabaseType::PostgreSQL) => vec![ServiceSpec {
                name: "database".to_string(),
                provider: ServiceProvider::Postgres,
            }],
            None => Vec::new(),
        }
    }
}

impl Provider for PythonProvider {
    fn name(&self) -> &'static str {
        "python"
    }

    fn platform(&self) -> Option<&str> {
        self.framework_platform()
    }

    fn plan(&self) -> Result<ProviderPlan> {
        Ok(ProviderPlan {
            serve_name: self
                .path
                .file_name()
                .map(|s| s.to_string())
                .unwrap_or_else(|| "app".to_string()),
            cwd: None,
            provider: self.name().to_string(),
            mounts: self.mounts(),
            platform: self.framework_platform().map(|s| s.to_string()),
            volumes: Vec::new(),
            declarations: self.declarations(),
            dependencies: self.dependencies(),
            build_steps: self.build_steps(),
            prepare: self.prepare_steps(),
            services: self.services(),
            commands: self.commands()?,
            env: self.env_map(),
        })
    }
}

pub struct PythonDescriptor;

impl ProviderDescriptor for PythonDescriptor {
    fn detect(&self, path: &Path, custom: &CustomCommands) -> Option<DetectResult> {
        let has_pyproject = path.join("pyproject.toml").exists();
        let has_requirements = path.join("requirements.txt").exists();
        if has_pyproject || has_requirements {
            if path.join("manage.py").exists() {
                return Some(DetectResult {
                    name: "python".to_string(),
                    score: 70,
                });
            }
            return Some(DetectResult {
                name: "python".to_string(),
                score: 50,
            });
        }
        if let Some(start) = &custom.start {
            if start.starts_with("python ")
                || start.starts_with("uv ")
                || start.starts_with("uvicorn ")
                || start.starts_with("gunicorn ")
            {
                return Some(DetectResult {
                    name: "python".to_string(),
                    score: 80,
                });
            }
        }
        let path_buf = Utf8PathBuf::from_path_buf(path.to_path_buf()).ok()?;
        if PythonProvider::detect_main_file(&path_buf).is_some() {
            return Some(DetectResult {
                name: "python".to_string(),
                score: 10,
            });
        }
        None
    }

    fn create(&self, path: &Path, custom: &CustomCommands) -> Result<Box<dyn Provider>> {
        Ok(Box::new(PythonProvider::new(path, custom)?))
    }

    fn name(&self) -> &'static str {
        "python"
    }
}

fn format_app_import(app: &str) -> String {
    Regex::new(r"\.([^.]+)$")
        .unwrap()
        .replace(app, ":$1")
        .to_string()
}

fn file_to_python_path(path: &str) -> String {
    path.trim_end_matches(".py")
        .replace('/', ".")
        .replace('\\', ".")
}

fn find_settings_file(path: &Utf8PathBuf) -> Option<std::path::PathBuf> {
    for entry in WalkDir::new(path) {
        let entry = entry.ok()?;
        if entry.file_type().is_file() && entry.file_name().to_string_lossy() == "settings.py" {
            return Some(entry.path().to_path_buf());
        }
    }
    None
}

fn glob_variants(root: &Utf8PathBuf, prefix: &str) -> Vec<String> {
    let mut res = Vec::new();
    let patterns = ["", "*"];
    let suffixes = ["", ".md", ".txt", ".rst"];
    for p in &patterns {
        for s in &suffixes {
            let name = format!("{prefix}{p}{s}");
            for entry in WalkDir::new(root) {
                if let Ok(entry) = entry {
                    if !entry.file_type().is_file() {
                        continue;
                    }
                    if entry.file_name().to_string_lossy().starts_with(&name) {
                        if let Ok(rel) = entry.path().strip_prefix(root.as_std_path()) {
                            res.push(rel.to_string_lossy().to_string());
                        }
                    }
                }
            }
        }
    }
    res
}
