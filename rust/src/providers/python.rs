//! Python web framework provider.
//!
//! Detects and builds Python-based web applications using frameworks like
//! Django, FastAPI, Flask, Streamlit, etc.

use crate::providers::base::Provider;
use crate::providers::specs::{DependencySpec, DetectResult, ProviderPlan};
use anyhow::{Context, Result};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

/// Python web frameworks
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PythonFramework {
    Django,
    FastAPI,
    Flask,
    Streamlit,
}

impl PythonFramework {
    fn name(&self) -> &str {
        match self {
            Self::Django => "Django",
            Self::FastAPI => "FastAPI",
            Self::Flask => "Flask",
            Self::Streamlit => "Streamlit",
        }
    }
}

/// Provider for Python web applications.
pub struct PythonProvider;

impl PythonProvider {
    /// Detect Python framework from dependencies.
    fn detect_framework(deps: &HashSet<String>) -> Option<PythonFramework> {
        if deps.contains("django") {
            Some(PythonFramework::Django)
        } else if deps.contains("fastapi") {
            Some(PythonFramework::FastAPI)
        } else if deps.contains("flask") {
            Some(PythonFramework::Flask)
        } else if deps.contains("streamlit") {
            Some(PythonFramework::Streamlit)
        } else {
            None
        }
    }

    /// Parse requirements.txt for dependencies.
    fn parse_requirements(path: &Path) -> Result<HashSet<String>> {
        let req_path = path.join("requirements.txt");
        if !req_path.exists() {
            return Ok(HashSet::new());
        }

        let content = fs::read_to_string(&req_path)?;
        let mut deps = HashSet::new();

        for line in content.lines() {
            let line = line.trim();
            // Skip comments and empty lines
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Extract package name (before ==, >=, etc.)
            if let Some(pkg) = line.split(&['=', '>', '<', '!', '~', '['][..]).next() {
                deps.insert(pkg.trim().to_lowercase());
            }
        }

        Ok(deps)
    }

    /// Check if project uses Django.
    #[allow(dead_code)]
    fn has_django_files(path: &Path) -> bool {
        path.join("manage.py").exists()
            || path.join("wsgi.py").exists()
            || path.join("asgi.py").exists()
    }

    /// Check if project uses Flask.
    #[allow(dead_code)]
    fn has_flask_files(path: &Path) -> bool {
        for entry in fs::read_dir(path).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("py") {
                if let Ok(content) = fs::read_to_string(&path) {
                    if content.contains("Flask(__name__)") {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Check if project has Python files.
    fn has_python_files(path: &Path) -> bool {
        for entry in fs::read_dir(path).into_iter().flatten().flatten() {
            if entry.path().extension().and_then(|s| s.to_str()) == Some("py") {
                return true;
            }
        }
        false
    }

    /// Detect Python version from .python-version or pyproject.toml.
    fn detect_python_version(path: &Path) -> Option<String> {
        // Check .python-version
        if let Ok(content) = fs::read_to_string(path.join(".python-version")) {
            return Some(content.trim().to_string());
        }

        // Default to 3.13
        None
    }
}

impl Provider for PythonProvider {
    fn name(&self) -> &str {
        "python"
    }

    fn priority(&self) -> i32 {
        5
    }

    fn detect(&self, path: &Path) -> Result<Option<DetectResult>> {
        // Check for Python indicators
        let has_requirements = path.join("requirements.txt").exists();
        let has_pyproject = path.join("pyproject.toml").exists();
        let has_py_files = Self::has_python_files(path);

        if !has_requirements && !has_pyproject && !has_py_files {
            return Ok(None);
        }

        // Parse dependencies
        let deps = Self::parse_requirements(path)?;
        let framework = Self::detect_framework(&deps);

        // Calculate confidence
        let confidence = if framework.is_some() {
            1.0
        } else if has_requirements {
            0.9
        } else if has_py_files {
            0.7
        } else {
            0.5
        };

        // Build reason
        let reason = if let Some(fw) = framework {
            format!("Detected {} project", fw.name())
        } else {
            "Found Python project files".to_string()
        };

        Ok(Some(DetectResult::new(self.name(), confidence, reason)))
    }

    fn plan(&self, path: &Path) -> Result<ProviderPlan> {
        let deps = Self::parse_requirements(path)?;
        let framework =
            Self::detect_framework(&deps).context("Could not detect Python framework")?;

        let mut plan = ProviderPlan::new("app", self.name());

        // Add Python dependency
        let python_version =
            Self::detect_python_version(path).unwrap_or_else(|| "3.13".to_string());
        let mut python_dep = DependencySpec::new("python");
        python_dep.default_version = Some(python_version);
        python_dep.use_in_build = true;
        python_dep.use_in_serve = true;
        plan.dependencies.push(python_dep);

        // Build steps (install dependencies)
        plan.build_steps
            .push("run(\"pip install -r requirements.txt\")".to_string());

        // Framework-specific configuration
        match framework {
            PythonFramework::Django => {
                // Add Django-specific steps
                plan.build_steps
                    .push("run(\"python manage.py collectstatic --noinput\")".to_string());

                // Serve command
                plan.commands.insert(
                    "web".to_string(),
                    "gunicorn mysite.wsgi:application --bind 0.0.0.0:$PORT".to_string(),
                );
            }
            PythonFramework::FastAPI => {
                // Serve command
                plan.commands.insert(
                    "web".to_string(),
                    "uvicorn main:app --host 0.0.0.0 --port $PORT".to_string(),
                );
            }
            PythonFramework::Flask => {
                // Serve command
                plan.commands.insert(
                    "web".to_string(),
                    "gunicorn app:app --bind 0.0.0.0:$PORT".to_string(),
                );
            }
            PythonFramework::Streamlit => {
                // Serve command
                plan.commands.insert(
                    "web".to_string(),
                    "streamlit run app.py --server.port $PORT".to_string(),
                );
            }
        }

        Ok(plan)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_requirements(path: &Path, content: &str) {
        fs::write(path.join("requirements.txt"), content).unwrap();
    }

    #[test]
    fn test_no_python_files() {
        let tmp = TempDir::new().unwrap();
        let provider = PythonProvider;

        let result = provider.detect(tmp.path()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_detect_django() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path();

        create_requirements(path, "Django==4.2.0\npsycopg2-binary==2.9.0\n");
        fs::write(path.join("manage.py"), "#!/usr/bin/env python\n").unwrap();

        let provider = PythonProvider;
        let result = provider.detect(path).unwrap().unwrap();

        assert_eq!(result.confidence, 1.0);
        assert!(result.reason.contains("Django"));
    }

    #[test]
    fn test_detect_fastapi() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path();

        create_requirements(path, "fastapi==0.104.0\nuvicorn==0.24.0\n");

        let provider = PythonProvider;
        let result = provider.detect(path).unwrap().unwrap();

        assert_eq!(result.confidence, 1.0);
        assert!(result.reason.contains("FastAPI"));
    }

    #[test]
    fn test_detect_flask() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path();

        create_requirements(path, "Flask==3.0.0\ngunicorn==21.0.0\n");

        let provider = PythonProvider;
        let result = provider.detect(path).unwrap().unwrap();

        assert_eq!(result.confidence, 1.0);
        assert!(result.reason.contains("Flask"));
    }

    #[test]
    fn test_detect_streamlit() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path();

        create_requirements(path, "streamlit==1.28.0\npandas==2.1.0\n");

        let provider = PythonProvider;
        let result = provider.detect(path).unwrap().unwrap();

        assert_eq!(result.confidence, 1.0);
        assert!(result.reason.contains("Streamlit"));
    }

    #[test]
    fn test_parse_requirements() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path();

        create_requirements(
            path,
            "Django==4.2.0\n# Comment\nfastapi>=0.100.0\nflask~=3.0\n",
        );

        let deps = PythonProvider::parse_requirements(path).unwrap();
        assert!(deps.contains("django"));
        assert!(deps.contains("fastapi"));
        assert!(deps.contains("flask"));
    }

    #[test]
    fn test_plan_django() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path();

        create_requirements(path, "Django==4.2.0\n");
        fs::write(path.join("manage.py"), "").unwrap();

        let provider = PythonProvider;
        let plan = provider.plan(path).unwrap();

        assert!(plan.build_steps.len() >= 2);
        assert!(plan.build_steps[0].contains("pip install"));
        assert!(plan.build_steps[1].contains("collectstatic"));
        assert!(plan.commands.contains_key("web"));
    }

    #[test]
    fn test_plan_fastapi() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path();

        create_requirements(path, "fastapi==0.104.0\n");

        let provider = PythonProvider;
        let plan = provider.plan(path).unwrap();

        assert!(plan.build_steps[0].contains("pip install"));
        assert!(plan.commands.get("web").unwrap().contains("uvicorn"));
    }

    #[test]
    fn test_python_version_detection() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path();

        fs::write(path.join(".python-version"), "3.11.5\n").unwrap();

        let version = PythonProvider::detect_python_version(path);
        assert_eq!(version, Some("3.11.5".to_string()));
    }
}
