use std::path::PathBuf;

use anybuild::GenerateOptions;
use anyhow::Result;

use crate::commands::client;
use crate::SharedProjectArgs;

pub fn run(shared: SharedProjectArgs, out: Option<PathBuf>) -> Result<()> {
    let generated = client(&shared, None)?.generate(GenerateOptions { output: out })?;
    if generated.config != serde_json::json!({}) {
        eprintln!("{}", serde_json::to_string_pretty(&generated.config)?);
    }
    Ok(())
}
