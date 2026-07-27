use std::path::PathBuf;

use anybuild::{GenerateOptions, GenerationCheckStatus};
use anyhow::{bail, Result};

use crate::commands::client;
use crate::SharedProjectArgs;

pub fn run(shared: SharedProjectArgs, out: Option<PathBuf>, check: bool) -> Result<()> {
    let client = client(&shared, None)?;
    if check {
        let checked = client.check_generation(GenerateOptions { output: out })?;
        match checked.status {
            GenerationCheckStatus::Current => {
                eprintln!("Anybuild is up to date at {}", checked.path.display());
                return Ok(());
            }
            GenerationCheckStatus::Missing => {
                bail!(
                    "No Anybuild file exists at {}; run `anybuild generate`",
                    checked.path.display()
                );
            }
            GenerationCheckStatus::Drifted => {
                eprintln!("Anybuild provider config is out of date:");
                for difference in checked.differences {
                    eprintln!(
                        "  {}: {} -> {}",
                        difference.path,
                        display_value(difference.persisted.as_ref()),
                        display_value(difference.detected.as_ref()),
                    );
                }
                bail!(
                    "run `anybuild generate` to update {}",
                    checked.path.display()
                );
            }
        }
    }
    client.generate(GenerateOptions { output: out })?;
    Ok(())
}

fn display_value(value: Option<&serde_json::Value>) -> String {
    value.map_or_else(
        || "<missing>".to_owned(),
        |value| serde_json::to_string(value).unwrap_or_else(|_| "<invalid>".to_owned()),
    )
}
