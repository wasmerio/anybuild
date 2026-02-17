//! Starlark functions for the Shipit DSL.
//!
//! This module defines all the Starlark functions available in Shipit files:
//! dep(), service(), run(), workdir(), copy(), env(), path(), use(), mount(),
//! volume(), and serve().

use crate::starlark::ctx::Serve;
use crate::starlark::eval::with_ctx;
use crate::types::package::{Architecture, Package};
use crate::types::service::{Service, ServiceProvider};
use crate::types::steps::{CopyBase, CopyStep, EnvStep, PathStep, RunStep, UseStep, WorkdirStep};
use crate::types::Step;
use allocative::Allocative;
use anyhow::anyhow;
use starlark::environment::GlobalsBuilder;
use starlark::eval::Evaluator;
use starlark::starlark_module;
use starlark::starlark_simple_value;
use starlark::values::dict::DictRef;
use starlark::values::none::NoneOr;
use starlark::values::starlark_value;
use starlark::values::tuple::UnpackTuple;
use starlark::values::{Heap, NoSerialize, ProvidesStaticType, StarlarkValue, Value, ValueLike};
use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;

/// A mount value in Starlark with field access.
#[derive(Debug, Clone, ProvidesStaticType, NoSerialize, Allocative)]
pub struct CtxMount {
    /// Mount reference string
    pub reference: String,
    /// Mount name
    pub name: String,
    /// Build path
    pub build_path: String,
    /// Serve path
    pub serve_path: String,
}

starlark_simple_value!(CtxMount);

impl fmt::Display for CtxMount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.reference)
    }
}

#[starlark_value(type = "Mount")]
impl<'v> StarlarkValue<'v> for CtxMount {
    fn get_attr(&self, attribute: &str, heap: &'v Heap) -> Option<Value<'v>> {
        match attribute {
            "name" => Some(heap.alloc(self.name.as_str())),
            "path" => Some(heap.alloc(self.build_path.as_str())),
            "build_path" => Some(heap.alloc(self.build_path.as_str())),
            "serve_path" => Some(heap.alloc(self.serve_path.as_str())),
            _ => None,
        }
    }

    fn has_attr(&self, attribute: &str, _heap: &'v Heap) -> bool {
        matches!(attribute, "name" | "path" | "build_path" | "serve_path")
    }

    fn dir_attr(&self) -> Vec<String> {
        vec![
            "name".to_string(),
            "path".to_string(),
            "build_path".to_string(),
            "serve_path".to_string(),
        ]
    }
}

/// A volume value in Starlark with field access.
#[derive(Debug, Clone, ProvidesStaticType, NoSerialize, Allocative)]
pub struct CtxVolume {
    /// Volume reference string
    pub reference: String,
    /// Volume name
    pub name: String,
    /// Serve path
    pub serve_path: String,
}

starlark_simple_value!(CtxVolume);

impl fmt::Display for CtxVolume {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.reference)
    }
}

#[starlark_value(type = "Volume")]
impl<'v> StarlarkValue<'v> for CtxVolume {
    fn get_attr(&self, attribute: &str, heap: &'v Heap) -> Option<Value<'v>> {
        match attribute {
            "name" => Some(heap.alloc(self.name.as_str())),
            "serve_path" => Some(heap.alloc(self.serve_path.as_str())),
            _ => None,
        }
    }

    fn has_attr(&self, attribute: &str, _heap: &'v Heap) -> bool {
        matches!(attribute, "name" | "serve_path")
    }

    fn dir_attr(&self) -> Vec<String> {
        vec!["name".to_string(), "serve_path".to_string()]
    }
}

/// Helper to extract a list of strings from a Starlark Value.
/// Skips `None` items, matching the Python evaluator which uses
/// `filter(None, ...)` and `if index is not None` patterns to
/// handle conditional expressions like `run(...) if cond else None`.
fn value_to_string_list<'v>(value: Value<'v>, heap: &'v Heap) -> anyhow::Result<Vec<String>> {
    let mut result = Vec::new();

    // Try to iterate through the list/tuple
    match value.iterate(heap) {
        Ok(iter) => {
            for item in iter {
                if item.is_none() {
                    // Skip None items (from conditional expressions)
                    continue;
                } else if let Some(s) = item.unpack_str() {
                    result.push(s.to_string());
                } else if let Some(mount) = item.downcast_ref::<CtxMount>() {
                    // Handle mount objects - use their reference string
                    result.push(mount.reference.clone());
                } else if let Some(volume) = item.downcast_ref::<CtxVolume>() {
                    // Handle volume objects - use their reference string
                    result.push(volume.reference.clone());
                } else {
                    return Err(anyhow!(
                        "List item must be a string, Mount, or Volume, got {:?}",
                        item
                    ));
                }
            }
            Ok(result)
        }
        Err(e) => Err(anyhow!("Expected a list: {}", e)),
    }
}

/// Helper to extract a single string-like reference from a Starlark value.
fn value_to_string<'v>(value: Value<'v>) -> anyhow::Result<String> {
    if let Some(s) = value.unpack_str() {
        Ok(s.to_string())
    } else if let Some(mount) = value.downcast_ref::<CtxMount>() {
        Ok(mount.reference.clone())
    } else if let Some(volume) = value.downcast_ref::<CtxVolume>() {
        Ok(volume.reference.clone())
    } else {
        Err(anyhow!(
            "Expected a string, Mount, or Volume, got {:?}",
            value
        ))
    }
}

/// Helper to extract a hashmap from a Starlark dict Value.
fn value_to_string_dict<'v>(value: Value<'v>) -> anyhow::Result<HashMap<String, String>> {
    use starlark::values::dict::DictRef;

    let mut result = HashMap::new();

    // Try to unpack as a dict
    if let Some(dict) = DictRef::from_value(value) {
        for (k, v) in dict.iter() {
            if let Some(key_str) = k.unpack_str() {
                if let Some(val_str) = v.unpack_str() {
                    result.insert(key_str.to_string(), val_str.to_string());
                } else {
                    return Err(anyhow!("Dict value must be a string"));
                }
            } else {
                return Err(anyhow!("Dict key must be a string"));
            }
        }
        Ok(result)
    } else {
        Err(anyhow!("Expected a dict"))
    }
}

/// Register all Shipit functions in the Starlark environment.
///
/// Note: The type_complexity and too_many_arguments warnings are allowed here because
/// they arise from the starlark_module macro's generated code and the Starlark library's
/// type system (Value<'v>, Evaluator<'v, '_>). These complex types are necessary for
/// Starlark's runtime type checking and evaluation system.
#[starlark_module]
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn shipit_functions(builder: &mut GlobalsBuilder) {
    /// Create a package dependency.
    ///
    /// # Arguments
    /// * `name` - Package name (e.g., "python", "node")
    /// * `version` - Optional version string
    /// * `architecture` - Optional architecture ("64-bit" or "32-bit")
    ///
    /// # Returns
    /// Reference string to the package
    fn dep(
        name: String,
        #[starlark(default = NoneOr::None)] version: NoneOr<String>,
        #[starlark(default = NoneOr::None)] architecture: NoneOr<String>,
        _eval: &mut Evaluator,
    ) -> anyhow::Result<String> {
        let version = match version {
            NoneOr::Other(v) => Some(v),
            NoneOr::None => None,
        };
        let architecture = match architecture {
            NoneOr::Other(a) => Some(a),
            NoneOr::None => None,
        };

        // Parse architecture
        let arch = architecture
            .map(|a| {
                if a == "64-bit" {
                    Ok(Architecture::Bit64)
                } else if a == "32-bit" {
                    Ok(Architecture::Bit32)
                } else {
                    Err(anyhow!("Invalid architecture: {}", a))
                }
            })
            .transpose()?;

        let _package = Package {
            name: name.clone(),
            version: version.clone(),
            architecture: arch,
        };

        // Add to context and return reference
        with_ctx(|ctx| {
            let package = Package {
                name,
                version,
                architecture: arch,
            };
            Ok(ctx.add_package(package))
        })
    }

    /// Create a service dependency.
    ///
    /// # Arguments
    /// * `name` - Service name
    /// * `provider` - Provider type ("postgres", "mysql", "redis")
    ///
    /// # Returns
    /// Reference string to the service
    fn service(name: String, provider: String) -> anyhow::Result<String> {
        let svc_provider = match provider.as_str() {
            "postgres" => ServiceProvider::Postgres,
            "mysql" => ServiceProvider::Mysql,
            "redis" => ServiceProvider::Redis,
            _ => return Err(anyhow!("Unknown service provider: {}", provider)),
        };

        with_ctx(|ctx| {
            let service = Service {
                name,
                provider: svc_provider,
            };
            Ok(ctx.add_service(service))
        })
    }

    /// Create a run step that executes a command.
    ///
    /// # Arguments
    /// * `command` - Shell command to execute
    /// * `inputs` - Optional list of input files
    /// * `outputs` - Optional list of output files
    /// * `group` - Optional group name
    ///
    /// # Returns
    /// Reference string to the step
    fn run<'v>(
        command: String,
        inputs: Option<Value<'v>>,
        outputs: Option<Value<'v>>,
        group: Option<String>,
        eval: &mut Evaluator<'v, '_>,
    ) -> anyhow::Result<String> {
        // Parse optional list parameters
        let inputs_vec = if let Some(val) = inputs {
            if val.is_none() {
                None
            } else {
                Some(value_to_string_list(val, eval.heap())?)
            }
        } else {
            None
        };

        let outputs_vec = if let Some(val) = outputs {
            if val.is_none() {
                None
            } else {
                Some(value_to_string_list(val, eval.heap())?)
            }
        } else {
            None
        };

        with_ctx(|ctx| {
            let step = Step::Run(RunStep {
                command,
                inputs: inputs_vec,
                outputs: outputs_vec,
                group,
            });
            Ok(ctx.add_step(step))
        })
    }

    /// Create a workdir step that changes the working directory.
    ///
    /// # Arguments
    /// * `path` - Directory path
    ///
    /// # Returns
    /// Reference string to the step
    fn workdir(path: String) -> anyhow::Result<String> {
        with_ctx(|ctx| {
            let step = Step::Workdir(WorkdirStep {
                path: PathBuf::from(path),
            });
            Ok(ctx.add_step(step))
        })
    }

    /// Create a copy step that copies files or downloads from URL.
    ///
    /// # Arguments
    /// * `source` - Source path or URL
    /// * `target` - Target path (optional, defaults to source)
    /// * `ignore` - Optional list of patterns to ignore
    ///
    /// # Returns
    /// Reference string to the step
    fn copy<'v>(
        source: String,
        target: Option<String>,
        ignore: Option<Value<'v>>,
        base: Option<String>,
        eval: &mut Evaluator<'v, '_>,
    ) -> anyhow::Result<String> {
        let target = target.unwrap_or_else(|| source.clone());

        // Parse optional ignore list
        let ignore_vec = if let Some(val) = ignore {
            if val.is_none() {
                None
            } else {
                Some(value_to_string_list(val, eval.heap())?)
            }
        } else {
            None
        };

        let copy_base = match base.as_deref() {
            Some("assets") => CopyBase::Assets,
            Some("source") | None => CopyBase::Source,
            Some(other) => {
                return Err(anyhow!(
                    "Invalid copy base '{}'. Expected 'source' or 'assets'",
                    other
                ))
            }
        };

        with_ctx(|ctx| {
            let step = Step::Copy(CopyStep {
                source,
                target,
                ignore: ignore_vec,
                base: copy_base,
            });
            Ok(ctx.add_step(step))
        })
    }

    /// Create an env step that sets environment variables.
    ///
    /// # Arguments
    /// * `kwargs` - Key-value pairs of environment variables
    ///
    /// # Returns
    /// Reference string to the step
    fn env<'v>(
        vars: Option<Value<'v>>,
        #[starlark(kwargs)] kwargs: DictRef<'v>,
    ) -> anyhow::Result<String> {
        let mut variables = HashMap::new();

        if let Some(value) = vars {
            variables.extend(value_to_string_dict(value)?);
        }

        for (key, value) in kwargs.iter() {
            let key_str = key
                .unpack_str()
                .ok_or_else(|| anyhow!("Environment variable name must be a string"))?;
            let value_str = value
                .unpack_str()
                .ok_or_else(|| anyhow!("Environment variable value must be a string"))?;
            variables.insert(key_str.to_string(), value_str.to_string());
        }

        with_ctx(|ctx| {
            let step = Step::Env(EnvStep { variables });
            Ok(ctx.add_step(step))
        })
    }

    /// Create a path step that adds to PATH.
    ///
    /// # Arguments
    /// * `path` - Path to add
    ///
    /// # Returns
    /// Reference string to the step
    fn path(path: String) -> anyhow::Result<String> {
        with_ctx(|ctx| {
            let step = Step::Path(PathStep { path });
            Ok(ctx.add_step(step))
        })
    }

    /// Create a use step that declares dependencies.
    ///
    /// # Arguments
    /// * `deps` - Variable number of package references
    ///
    /// # Returns
    /// Reference string to the step
    fn r#use<'v>(
        #[starlark(args)] deps: UnpackTuple<Value<'v>>,
        eval: &mut Evaluator<'v, '_>,
    ) -> anyhow::Result<String> {
        let dependencies = if deps.items.len() == 1 {
            let first = deps.items[0];
            match value_to_string_list(first, eval.heap()) {
                Ok(list) => list,
                Err(_) => vec![value_to_string(first)?],
            }
        } else {
            deps.items
                .iter()
                .map(|value| value_to_string(*value))
                .collect::<anyhow::Result<Vec<_>>>()?
        };

        with_ctx(|ctx| {
            let step = Step::Use(UseStep { dependencies });
            Ok(ctx.add_step(step))
        })
    }

    /// Create a mount.
    ///
    /// # Arguments
    /// * `name` - Mount name
    ///
    /// # Returns
    /// Mount object with fields: name, path, build_path, serve_path
    fn mount<'v>(name: String, eval: &mut Evaluator<'v, '_>) -> anyhow::Result<Value<'v>> {
        use crate::types::Mount;
        use std::path::PathBuf;

        let (build_path, serve_path) = if name == "app" {
            ("/build/app".to_string(), "/app".to_string())
        } else {
            (format!("/build/opt/{}", name), format!("/opt/{}", name))
        };

        let reference = with_ctx(|ctx| {
            let mount = Mount {
                name: name.clone(),
                build_path: PathBuf::from(build_path.clone()),
                serve_path: PathBuf::from(serve_path.clone()),
            };
            Ok(ctx.add_mount(mount))
        })?;

        let ctx_mount = CtxMount {
            reference: reference.clone(),
            name: name.clone(),
            build_path,
            serve_path,
        };

        Ok(eval.heap().alloc(ctx_mount))
    }

    /// Create a volume.
    ///
    /// # Arguments
    /// * `name` - Volume name
    /// * `serve` - Serve path
    ///
    /// # Returns
    /// Volume object with fields: name, serve_path
    fn volume<'v>(
        name: String,
        serve: String,
        eval: &mut Evaluator<'v, '_>,
    ) -> anyhow::Result<Value<'v>> {
        use crate::types::Volume;
        use std::path::PathBuf;

        let reference = with_ctx(|ctx| {
            let volume = Volume {
                name: name.clone(),
                serve_path: PathBuf::from(serve.clone()),
            };
            Ok(ctx.add_volume(volume))
        })?;

        let ctx_volume = CtxVolume {
            reference: reference.clone(),
            name: name.clone(),
            serve_path: serve.clone(),
        };

        Ok(eval.heap().alloc(ctx_volume))
    }

    /// Create a serve configuration.
    ///
    /// # Arguments
    /// * `name` - Serve name
    /// * `provider` - Provider name
    /// * `build` - List of build step references
    /// * `deps` - List of package references
    /// * `commands` - Dictionary of commands
    /// * `cwd` - Optional working directory
    /// * `prepare` - Optional list of prepare step references
    /// * `workers` - Optional list of worker command names
    /// * `mounts` - Optional list of mount references
    /// * `volumes` - Optional list of volume references
    /// * `env` - Optional environment variables dictionary
    /// * `services` - Optional list of service references
    ///
    /// # Returns
    /// Reference string to the serve configuration
    fn serve<'v>(
        name: String,
        provider: String,
        build: Value<'v>,
        deps: Value<'v>,
        commands: Value<'v>,
        cwd: Option<String>,
        prepare: Option<Value<'v>>,
        workers: Option<Value<'v>>,
        mounts: Option<Value<'v>>,
        volumes: Option<Value<'v>>,
        env: Option<Value<'v>>,
        services: Option<Value<'v>>,
        eval: &mut Evaluator<'v, '_>,
    ) -> anyhow::Result<String> {
        let heap = eval.heap();

        // Parse required list parameters
        let build_vec = value_to_string_list(build, heap)?;
        let deps_vec = value_to_string_list(deps, heap)?;
        let commands_map = value_to_string_dict(commands)?;

        // Parse optional list parameters
        let prepare_vec = if let Some(val) = prepare {
            if val.is_none() {
                Vec::new()
            } else {
                value_to_string_list(val, heap)?
            }
        } else {
            Vec::new()
        };

        let workers_vec = if let Some(val) = workers {
            if val.is_none() {
                Vec::new()
            } else {
                value_to_string_list(val, heap)?
            }
        } else {
            Vec::new()
        };

        let mounts_vec = if let Some(val) = mounts {
            if val.is_none() {
                Vec::new()
            } else {
                value_to_string_list(val, heap)?
            }
        } else {
            Vec::new()
        };

        let volumes_vec = if let Some(val) = volumes {
            if val.is_none() {
                Vec::new()
            } else {
                value_to_string_list(val, heap)?
            }
        } else {
            Vec::new()
        };

        let services_vec = if let Some(val) = services {
            if val.is_none() {
                Vec::new()
            } else {
                value_to_string_list(val, heap)?
            }
        } else {
            Vec::new()
        };

        // Parse optional dict parameter
        let env_map = if let Some(val) = env {
            if val.is_none() {
                HashMap::new()
            } else {
                value_to_string_dict(val)?
            }
        } else {
            HashMap::new()
        };

        with_ctx(|ctx| {
            let serve = Serve {
                name: name.clone(),
                provider,
                build: build_vec,
                deps: deps_vec,
                commands: commands_map,
                cwd,
                prepare: prepare_vec,
                workers: workers_vec,
                mounts: mounts_vec,
                volumes: volumes_vec,
                env: env_map,
                services: services_vec,
            };
            Ok(ctx.add_serve(serve))
        })
    }
}

#[cfg(test)]
mod tests {
    // TODO: Add tests that call functions through Starlark evaluator
}
