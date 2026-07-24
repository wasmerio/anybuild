/// CLI-only collection of execution flags translated into SDK options.
#[derive(Debug, Clone, Default)]
pub struct EnvironmentOptions {
    pub wasmer: bool,
    pub wasmer_bin: Option<String>,
    pub wasmer_registry: Option<String>,
    pub wasmer_token: Option<String>,
    pub docker: bool,
    pub docker_client: Option<String>,
    pub docker_opts: Option<String>,
}
