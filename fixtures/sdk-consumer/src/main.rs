use anybuild::{Anybuild, BuildOptions, RunOptions};

fn main() -> Result<(), anybuild::Error> {
    let project = Anybuild::new(".")
        .with_subdir("apps/web")
        .with_env("ANYBUILD_NODE_VERSION", "22");

    let _plan = project.plan(Default::default())?;
    let _build = project.build(BuildOptions::default())?;
    let _run = project.run(RunOptions::default().start())?;
    Ok(())
}
