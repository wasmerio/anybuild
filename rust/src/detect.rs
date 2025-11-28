//! Provider detection logic and scoring.

use std::path::Path;

use crate::Result;
use crate::model::CustomCommands;
use crate::provider::{Provider, ProviderDescriptor, registry};

/// Run detection across an ordered provider registry, returning the chosen
/// provider instance and its detect result.
pub fn detect_provider(
    providers: &[Box<dyn ProviderDescriptor>],
    path: &Path,
    custom: &CustomCommands,
) -> Result<Option<(Box<dyn Provider>, i32)>> {
    let mut best: Option<(Box<dyn Provider>, i32, usize)> = None;
    for (idx, descriptor) in providers.iter().enumerate() {
        if let Some(result) = descriptor.detect(path, custom) {
            if let Some((_, best_score, best_idx)) = &best {
                if result.score < *best_score || (result.score == *best_score && idx > *best_idx) {
                    continue;
                }
            }
            let provider = descriptor.create(path, custom)?;
            best = Some((provider, result.score, idx));
        }
    }

    Ok(best.map(|(p, score, _)| (p, score)))
}

/// Convenience wrapper using the global registry.
pub fn detect_registered_provider(
    path: &Path,
    custom: &CustomCommands,
) -> Result<Option<(Box<dyn Provider>, i32)>> {
    let providers = registry::providers();
    detect_provider(&providers, path, custom)
}
