//! AWS Lambda managed-runtime packaging for Docker-built artifacts.

use std::fs::File;
use std::io::{Read, Seek, Write};
use std::path::{Component, Path, PathBuf};

use anyhow::{bail, Context, Result};
use walkdir::WalkDir;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

use crate::artifact::RuntimeArtifact;
use crate::build::BuildBackend;
use crate::plan::{Package, Serve};

struct ManagedRuntime {
    identifier: String,
    python_version: Option<String>,
}

pub(crate) fn package(
    serve: &Serve,
    backend: &dyn BuildBackend,
    archive: &Path,
) -> Result<Option<RuntimeArtifact>> {
    if backend.artifact_platform().is_none() {
        return Ok(None);
    }
    let Some(runtime) = managed_runtime(&serve.deps) else {
        return Ok(None);
    };
    let start = serve
        .commands
        .get("start")
        .context("A Lambda runtime artifact requires a start command")?;
    let mappings = mount_mappings(serve, backend)?;
    let parent = archive
        .parent()
        .context("Lambda archive path has no parent directory")?;
    std::fs::create_dir_all(parent)?;

    let file = File::create(archive)
        .with_context(|| format!("failed to create Lambda archive {}", archive.display()))?;
    let mut zip = ZipWriter::new(file);
    for (source, destination) in &mappings {
        add_tree(
            &mut zip,
            source,
            destination,
            runtime.python_version.as_deref(),
        )?;
    }

    let script = start_script(serve, start, &mappings);
    zip.start_file(
        "run.sh",
        SimpleFileOptions::default().unix_permissions(0o755),
    )?;
    zip.write_all(script.as_bytes())?;
    zip.finish()?;

    let mut environment = serve.env.clone().unwrap_or_default();
    if !environment.contains_key("HOST") {
        environment.insert("HOST".to_owned(), "0.0.0.0".to_owned());
    }
    for value in environment.values_mut() {
        *value = rewrite_mount_paths(value, &mappings, "/var/task");
    }
    environment.insert(
        "AWS_LAMBDA_EXEC_WRAPPER".to_owned(),
        "/opt/bootstrap".to_owned(),
    );
    environment.insert(
        "AWS_LWA_PORT".to_owned(),
        serve.runtime_port.unwrap_or(8080).to_string(),
    );

    Ok(Some(RuntimeArtifact::LambdaZip {
        archive: archive.to_path_buf(),
        runtime: runtime.identifier,
        handler: "run.sh".to_owned(),
        environment,
        platform: backend.artifact_platform().map(str::to_owned),
    }))
}

fn managed_runtime(dependencies: &[Package]) -> Option<ManagedRuntime> {
    let mut selected: Option<ManagedRuntime> = None;
    for dependency in dependencies {
        let runtime = match dependency.name.as_str() {
            "bash" => continue,
            "python" => python_runtime(dependency.version.as_deref()?),
            "node" => node_runtime(dependency.version.as_deref()?),
            _ => return None,
        }?;
        if selected.is_some() {
            return None;
        }
        selected = Some(runtime);
    }
    selected
}

fn python_runtime(version: &str) -> Option<ManagedRuntime> {
    let components = numeric_version(version);
    let [major, minor, ..] = components.as_slice() else {
        return None;
    };
    if *major != 3 || !matches!(*minor, 10..=14) {
        return None;
    }
    let version = format!("{major}.{minor}");
    Some(ManagedRuntime {
        identifier: format!("python{version}"),
        python_version: Some(version),
    })
}

fn node_runtime(version: &str) -> Option<ManagedRuntime> {
    let major = *numeric_version(version).first()?;
    if !matches!(major, 22 | 24) {
        return None;
    }
    Some(ManagedRuntime {
        identifier: format!("nodejs{major}.x"),
        python_version: None,
    })
}

fn numeric_version(version: &str) -> Vec<u32> {
    version
        .trim_start_matches('v')
        .split('.')
        .map_while(|component| component.parse().ok())
        .collect()
}

fn mount_mappings(serve: &Serve, backend: &dyn BuildBackend) -> Result<Vec<(PathBuf, PathBuf)>> {
    let mut mappings = Vec::new();
    for mount in serve.mounts.as_deref().unwrap_or_default() {
        let destination = archive_path(&mount.serve_path)?;
        mappings.push((backend.get_artifact_mount_path(&mount.name), destination));
    }
    mappings.sort_by(|left, right| {
        right
            .1
            .components()
            .count()
            .cmp(&left.1.components().count())
    });
    Ok(mappings)
}

fn archive_path(path: &Path) -> Result<PathBuf> {
    let mut output = PathBuf::new();
    for component in path.components() {
        match component {
            Component::RootDir => {}
            Component::Normal(component) => output.push(component),
            _ => bail!(
                "Lambda mount path must be absolute and normalized: {}",
                path.display()
            ),
        }
    }
    if !path.is_absolute() || output.as_os_str().is_empty() {
        bail!(
            "Lambda mount path must be absolute and non-root: {}",
            path.display()
        );
    }
    Ok(output)
}

fn add_tree<W: Write + Seek>(
    zip: &mut ZipWriter<W>,
    source: &Path,
    destination: &Path,
    python_version: Option<&str>,
) -> Result<()> {
    let mut entries = WalkDir::new(source)
        .follow_links(false)
        .into_iter()
        .collect::<std::result::Result<Vec<_>, _>>()?;
    entries.sort_by(|left, right| left.path().cmp(right.path()));

    for entry in entries {
        let relative = entry.path().strip_prefix(source)?;
        let archive_path = destination.join(relative);
        let name = zip_name(&archive_path);
        let metadata = std::fs::symlink_metadata(entry.path())?;
        let mode = unix_mode(&metadata);
        let options = SimpleFileOptions::default().unix_permissions(mode);
        if metadata.is_dir() {
            zip.add_directory(format!("{}/", name.trim_end_matches('/')), options)?;
            continue;
        }
        if is_venv_python(&archive_path) {
            let version =
                python_version.context("Python virtualenv found in non-Python runtime")?;
            zip.add_symlink(name, format!("/var/lang/bin/python{version}"), options)?;
            continue;
        }
        if metadata.file_type().is_symlink() {
            let target = std::fs::read_link(entry.path())?;
            zip.add_symlink(name, target.to_string_lossy(), options)?;
            continue;
        }

        zip.start_file(name, options)?;
        let mut contents = Vec::new();
        File::open(entry.path())?.read_to_end(&mut contents)?;
        if let Some(version) = python_version {
            rewrite_python_shebang(&archive_path, &mut contents, version);
        }
        zip.write_all(&contents)?;
    }
    Ok(())
}

fn start_script(serve: &Serve, start: &str, mappings: &[(PathBuf, PathBuf)]) -> String {
    let mut script = String::from("#!/bin/bash\nset -e\n");
    if mappings
        .iter()
        .any(|(_, destination)| destination == Path::new("opt/venv"))
    {
        script.push_str("export PATH=\"/var/task/opt/venv/bin:${PATH}\"\n");
    }
    if let Some(cwd) = &serve.cwd {
        let cwd = rewrite_mount_paths(cwd, mappings, "/var/task");
        script.push_str(&format!("cd {}\n", shell_quote(&cwd)));
    }
    script.push_str(&rewrite_mount_paths(start, mappings, "/var/task"));
    script.push('\n');
    script
}

fn rewrite_mount_paths(value: &str, mappings: &[(PathBuf, PathBuf)], task_root: &str) -> String {
    mappings.iter().fold(value.to_owned(), |value, (_, path)| {
        let serve_path = format!("/{}", zip_name(path));
        value.replace(&serve_path, &format!("{task_root}/{}", zip_name(path)))
    })
}

fn is_venv_python(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    path.parent() == Some(Path::new("opt/venv/bin"))
        && (name == "python"
            || name == "python3"
            || name
                .strip_prefix("python3.")
                .is_some_and(|minor| minor.chars().all(|character| character.is_ascii_digit())))
}

fn rewrite_python_shebang(path: &Path, contents: &mut Vec<u8>, version: &str) {
    if path.parent() != Some(Path::new("opt/venv/bin")) || !contents.starts_with(b"#!") {
        return;
    }
    let Some(newline) = contents.iter().position(|byte| *byte == b'\n') else {
        return;
    };
    if !String::from_utf8_lossy(&contents[..newline]).contains("python") {
        return;
    }
    let mut rewritten = format!("#!/var/lang/bin/python{version}\n").into_bytes();
    rewritten.extend_from_slice(&contents[newline + 1..]);
    *contents = rewritten;
}

fn zip_name(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(unix)]
fn unix_mode(metadata: &std::fs::Metadata) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode()
}

#[cfg(not(unix))]
fn unix_mode(metadata: &std::fs::Metadata) -> u32 {
    if metadata.is_dir() {
        0o755
    } else {
        0o644
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::{Mount, Step};
    use indexmap::IndexMap;

    struct TestBackend {
        root: PathBuf,
    }

    impl BuildBackend for TestBackend {
        fn build(
            &mut self,
            _name: &str,
            _env: &IndexMap<String, String>,
            _mounts: &[Mount],
            _steps: &[Step],
        ) -> Result<()> {
            Ok(())
        }

        fn get_build_mount_path(&self, name: &str) -> PathBuf {
            self.root.join(name)
        }

        fn get_artifact_mount_path(&self, name: &str) -> PathBuf {
            self.root.join(name)
        }

        fn get_volume_path(&self, name: &str) -> PathBuf {
            self.root.join(name)
        }

        fn get_runtime_path(&self) -> Option<String> {
            None
        }

        fn artifact_platform(&self) -> Option<&str> {
            Some("linux/amd64")
        }
    }

    #[test]
    fn selects_only_supported_single_language_runtimes() {
        let package = |name: &str, version: Option<&str>| Package {
            name: name.to_owned(),
            version: version.map(str::to_owned),
            architecture: None,
        };

        assert_eq!(
            managed_runtime(&[package("python", Some("3.13.4")), package("bash", None)])
                .unwrap()
                .identifier,
            "python3.13"
        );
        assert_eq!(
            managed_runtime(&[package("node", Some("v24.1.0"))])
                .unwrap()
                .identifier,
            "nodejs24.x"
        );
        assert!(managed_runtime(&[package("python", Some("3.9"))]).is_none());
        assert!(managed_runtime(&[package("node", Some("20"))]).is_none());
        assert!(
            managed_runtime(&[package("python", Some("3.13")), package("node", Some("24")),])
                .is_none()
        );
        assert!(managed_runtime(&[
            package("python", Some("3.13")),
            package("ffmpeg", Some("7")),
        ])
        .is_none());
        assert!(managed_runtime(&[package("bash", None)]).is_none());
    }

    #[test]
    fn rewrites_runtime_paths_without_partial_mount_matches() {
        let mappings = vec![
            (PathBuf::from("source"), PathBuf::from("opt/venv")),
            (PathBuf::from("source"), PathBuf::from("app")),
        ];
        assert_eq!(
            rewrite_mount_paths(
                "/opt/venv/bin/python /app/main.py",
                &mappings,
                "${LAMBDA_TASK_ROOT}"
            ),
            "${LAMBDA_TASK_ROOT}/opt/venv/bin/python ${LAMBDA_TASK_ROOT}/app/main.py"
        );
    }

    #[test]
    fn packages_python_artifacts_for_the_managed_runtime() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("artifacts");
        std::fs::create_dir_all(root.join("app")).unwrap();
        std::fs::create_dir_all(root.join("venv/bin")).unwrap();
        std::fs::write(root.join("app/main.py"), "print('ready')\n").unwrap();
        std::fs::write(
            root.join("venv/bin/uvicorn"),
            "#!/mise/installs/python/3.13.4/bin/python\nprint('uvicorn')\n",
        )
        .unwrap();
        let backend = TestBackend { root };
        let serve = Serve {
            name: "api".to_owned(),
            provider: "python".to_owned(),
            runtime_port: Some(8000),
            build: Vec::new(),
            deps: vec![
                Package {
                    name: "python".to_owned(),
                    version: Some("3.13.4".to_owned()),
                    architecture: None,
                },
                Package {
                    name: "bash".to_owned(),
                    version: None,
                    architecture: None,
                },
            ],
            commands: IndexMap::from([(
                "start".to_owned(),
                "/opt/venv/bin/uvicorn /app/main.py".to_owned(),
            )]),
            cwd: Some("/app".to_owned()),
            prepare: None,
            mounts: Some(vec![
                Mount {
                    name: "app".to_owned(),
                    build_path: PathBuf::new(),
                    serve_path: PathBuf::from("/app"),
                },
                Mount {
                    name: "venv".to_owned(),
                    build_path: PathBuf::new(),
                    serve_path: PathBuf::from("/opt/venv"),
                },
            ]),
            volumes: None,
            env: Some(IndexMap::from([(
                "PYTHONPATH".to_owned(),
                "/app".to_owned(),
            )])),
            services: None,
        };
        let archive = temporary.path().join("function.zip");

        let artifact = package(&serve, &backend, &archive).unwrap().unwrap();

        let RuntimeArtifact::LambdaZip {
            runtime,
            environment,
            ..
        } = artifact
        else {
            panic!("expected Lambda ZIP artifact")
        };
        assert_eq!(runtime, "python3.13");
        assert_eq!(environment["AWS_LWA_PORT"], "8000");
        assert_eq!(environment["PYTHONPATH"], "/var/task/app");
        let file = File::open(archive).unwrap();
        let mut zip = zip::ZipArchive::new(file).unwrap();
        let mut run = String::new();
        let mut run_file = zip.by_name("run.sh").unwrap();
        assert_eq!(run_file.unix_mode().unwrap() & 0o777, 0o755);
        run_file.read_to_string(&mut run).unwrap();
        drop(run_file);
        assert!(run.contains("cd '/var/task/app'"));
        assert!(run.contains("/var/task/opt/venv/bin/uvicorn"));
        let mut executable = String::new();
        zip.by_name("opt/venv/bin/uvicorn")
            .unwrap()
            .read_to_string(&mut executable)
            .unwrap();
        assert!(executable.starts_with("#!/var/lang/bin/python3.13\n"));
        assert!(zip.by_name("app/main.py").is_ok());
    }
}
