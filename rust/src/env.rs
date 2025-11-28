//! Environment loading and resolution helpers (.env handling, overrides).

use camino::Utf8Path;
use dotenvy::Error as EnvError;
use std::collections::BTreeMap;

use crate::Result;

/// Load environment variables from `.env` and optionally `.env.<env_name>`,
/// merging them with process env (process wins).
pub fn load_env(base_dir: &Utf8Path, env_name: Option<&str>) -> Result<BTreeMap<String, String>> {
    let mut env = BTreeMap::new();
    let base_path = base_dir.join(".env");
    merge_env_file(&base_path, &mut env)?;

    if let Some(name) = env_name {
        let scoped = base_dir.join(format!(".env.{name}"));
        merge_env_file(&scoped, &mut env)?;
    }

    // Overlay process env last
    for (k, v) in std::env::vars() {
        env.insert(k, v);
    }

    Ok(env)
}

fn merge_env_file(path: &Utf8Path, target: &mut BTreeMap<String, String>) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let iter = match dotenvy::from_path_iter(path) {
        Ok(iter) => iter,
        Err(EnvError::Io(err)) => return Err(err.into()),
        Err(EnvError::LineParse(line, idx)) => {
            return Err(anyhow::anyhow!(
                "Invalid line in {} (index {}): {}",
                path,
                idx,
                line
            ));
        }
        Err(EnvError::EnvVar(err)) => return Err(err.into()),
        Err(err) => return Err(err.into()),
    };

    for item in iter {
        let (k, v) = item?;
        target.insert(k, v);
    }
    Ok(())
}
