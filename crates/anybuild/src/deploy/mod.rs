//! Deployment platform adapters.

use std::path::Path;

use anyhow::Result;

use crate::artifact::{ArtifactKind, RuntimeArtifact};
use crate::operation::OperationContext;
use crate::sdk::{DeployOutcome, DeployTarget, DeploymentPlatform};

pub mod aws_lambda;
pub mod fly;
pub mod wasmer;

pub(crate) trait Deployer {
    fn platform_name(&self) -> &'static str;
    fn artifact_kind(&self) -> ArtifactKind;
    fn accepts_artifact(&self, artifact: &RuntimeArtifact) -> bool {
        artifact.contains_kind(self.artifact_kind())
    }
    fn load_legacy_artifact(&self, anybuild_dir: &Path) -> Result<RuntimeArtifact>;
    fn deploy(&mut self, artifact: &RuntimeArtifact, target: DeployTarget)
        -> Result<DeployOutcome>;
}

pub(crate) fn resolve_deployer(
    platform: DeploymentPlatform,
    operation: OperationContext,
) -> Box<dyn Deployer> {
    match platform {
        DeploymentPlatform::Wasmer(options) => {
            Box::new(wasmer::WasmerDeployer::new(options, operation))
        }
        DeploymentPlatform::Fly(options) => Box::new(fly::FlyDeployer::new(options, operation)),
        DeploymentPlatform::AwsLambda(options) => {
            Box::new(aws_lambda::AwsLambdaDeployer::new(options, operation))
        }
    }
}
