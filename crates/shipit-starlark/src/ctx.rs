//! The evaluation context and the builtins exposed to Shipit files.
//!
//! Port of `evaluator.py`'s `Ctx`: builtins construct plan objects
//! directly; `serve()` receives them back and registers the final plan.

#![allow(clippy::too_many_arguments)]

use std::cell::RefCell;
use std::path::{Component, Path, PathBuf};

use anyhow::{anyhow, bail, Result};
use indexmap::IndexMap;
use starlark::any::ProvidesStaticType;
use starlark::environment::GlobalsBuilder;
use starlark::eval::Evaluator;
use starlark::values::dict::{AllocDict, DictRef, UnpackDictEntries};
use starlark::values::list_or_tuple::UnpackListOrTuple;
use starlark::values::none::{NoneOr, NoneType};
use starlark::values::tuple::UnpackTuple;
use starlark::values::{Value, ValueLike};

use shipit_plan::layout::MountLayout;
use shipit_plan::{
    CopyStep, EnvStep, Mount, Package, PathStep, RunStep, Serve, Service, Step, UseStep, Volume,
    WorkdirStep, WriteFileStep,
};

use crate::values::{MountValue, PackageValue, ServiceValue, StepValue, VolumeValue};

/// Per-run evaluation context, reachable from builtins via `eval.extra`.
#[derive(ProvidesStaticType)]
pub struct Ctx {
    pub layout: Box<dyn MountLayout>,
    pub source_dir: Option<PathBuf>,
    pub serves: RefCell<IndexMap<String, Serve>>,
}

impl Ctx {
    pub fn new(layout: Box<dyn MountLayout>, source_dir: Option<PathBuf>) -> Self {
        Self {
            layout,
            source_dir,
            serves: RefCell::new(IndexMap::new()),
        }
    }

    /// Port of `Ctx.file_exists`: app-dir-scoped, lexical escape check
    /// (in-project symlinks may point outside the tree, so the target is
    /// never resolved), follows symlinks for the existence probe.
    pub fn file_exists(&self, path: &str) -> Result<bool> {
        let source_dir = self
            .source_dir
            .as_ref()
            .ok_or_else(|| anyhow!("file_exists() is not available in this context"))?;
        let candidate = Path::new(path);
        if candidate.is_absolute() {
            bail!("file_exists() requires a project-relative path, got {path:?}");
        }
        if candidate
            .components()
            .any(|c| matches!(c, Component::ParentDir))
        {
            bail!("file_exists() path escapes the project: {path:?}");
        }
        Ok(source_dir.join(candidate).exists())
    }
}

/// Python's `Path(s)` normalizes trailing slashes and duplicate
/// separators; rebuilding from components matches that.
fn python_path(s: &str) -> PathBuf {
    Path::new(s).components().collect()
}

fn ctx<'a>(eval: &Evaluator<'_, 'a, '_>) -> Result<&'a Ctx> {
    eval.extra
        .ok_or_else(|| anyhow!("shipit evaluation context missing"))?
        .downcast_ref::<Ctx>()
        .ok_or_else(|| anyhow!("shipit evaluation context has the wrong type"))
}

fn flat<T>(value: Option<NoneOr<T>>) -> Option<T> {
    match value {
        Some(NoneOr::Other(v)) => Some(v),
        _ => None,
    }
}

fn step_from(value: Value) -> Result<Option<Step>> {
    if value.is_none() {
        return Ok(None);
    }
    let step = value
        .downcast_ref::<StepValue>()
        .ok_or_else(|| anyhow!("expected a build step, got {}", value.get_type()))?;
    Ok(Some(step.0.clone()))
}

fn package_from(value: Value) -> Result<Option<Package>> {
    if value.is_none() {
        return Ok(None);
    }
    let package = value
        .downcast_ref::<PackageValue>()
        .ok_or_else(|| anyhow!("expected a dep(), got {}", value.get_type()))?;
    Ok(Some(package.0.clone()))
}

fn mount_from(value: Value) -> Result<Option<Mount>> {
    if value.is_none() {
        return Ok(None);
    }
    let mount = value
        .downcast_ref::<MountValue>()
        .ok_or_else(|| anyhow!("expected a mount(), got {}", value.get_type()))?;
    Ok(Some(mount.mount.clone()))
}

fn volume_from(value: Value) -> Result<Option<Volume>> {
    if value.is_none() {
        return Ok(None);
    }
    let dict = DictRef::from_value(value)
        .ok_or_else(|| anyhow!("expected a volume(), got {}", value.get_type()))?;
    let inner = dict
        .get_str("volume")
        .ok_or_else(|| anyhow!("volume dict is missing its 'volume' entry"))?;
    let volume = inner
        .downcast_ref::<VolumeValue>()
        .ok_or_else(|| anyhow!("volume entry has the wrong type"))?;
    Ok(Some(volume.0.clone()))
}

fn service_from(value: Value) -> Result<Option<Service>> {
    if value.is_none() {
        return Ok(None);
    }
    let service = value
        .downcast_ref::<ServiceValue>()
        .ok_or_else(|| anyhow!("expected a service(), got {}", value.get_type()))?;
    Ok(Some(service.0.clone()))
}

#[allow(clippy::too_many_arguments)]
fn build_serve(
    eval: &Evaluator<'_, '_, '_>,
    name: &str,
    provider: &str,
    build: &UnpackListOrTuple<Value>,
    deps: &UnpackListOrTuple<Value>,
    commands: &UnpackDictEntries<String, String>,
    cwd: Option<&str>,
    prepare: Option<&UnpackListOrTuple<Value>>,
    mounts: Option<&UnpackListOrTuple<Value>>,
    volumes: Option<&UnpackListOrTuple<Value>>,
    env: Option<&UnpackDictEntries<String, String>>,
    services: Option<&UnpackListOrTuple<Value>>,
) -> Result<()> {
    // Conditional steps evaluate to None; prepare only runs commands.
    let prepare_steps: Option<Vec<RunStep>> = prepare.map(|items| {
        items
            .items
            .iter()
            .filter_map(|v| match step_from(*v).ok().flatten() {
                Some(Step::Run(run)) => Some(run),
                _ => None,
            })
            .collect()
    });
    let build_steps: Vec<Step> = build
        .items
        .iter()
        .map(|v| step_from(*v))
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect();
    let dep_list: Vec<Package> = deps
        .items
        .iter()
        .map(|v| package_from(*v))
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect();
    let command_map: IndexMap<String, String> = commands.entries.iter().cloned().collect();

    // Python: `x if x else None` — empty collections collapse to None.
    let mounts = match mounts {
        Some(items) if !items.items.is_empty() => Some(
            items
                .items
                .iter()
                .map(|v| mount_from(*v))
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .flatten()
                .collect::<Vec<Mount>>(),
        ),
        _ => None,
    };
    let volumes = match volumes {
        Some(items) if !items.items.is_empty() => Some(
            items
                .items
                .iter()
                .map(|v| volume_from(*v))
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .flatten()
                .collect::<Vec<Volume>>(),
        ),
        _ => None,
    };
    let services = match services {
        Some(items) if !items.items.is_empty() => Some(
            items
                .items
                .iter()
                .map(|v| service_from(*v))
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .flatten()
                .collect::<Vec<Service>>(),
        ),
        _ => None,
    };
    let env: Option<IndexMap<String, String>> =
        env.map(|entries| entries.entries.iter().cloned().collect());

    let serve = Serve {
        name: name.to_owned(),
        provider: provider.to_owned(),
        build: build_steps,
        deps: dep_list,
        commands: command_map,
        cwd: cwd.map(str::to_owned),
        prepare: prepare_steps,
        mounts,
        volumes,
        env,
        services,
    };
    ctx(eval)?
        .serves
        .borrow_mut()
        .insert(name.to_owned(), serve);
    Ok(())
}

#[starlark::starlark_module]
pub fn shipit_builtins(builder: &mut GlobalsBuilder) {
    fn dep(
        name: &str,
        version: Option<NoneOr<&str>>,
        architecture: Option<NoneOr<&str>>,
    ) -> anyhow::Result<PackageValue> {
        Ok(PackageValue(Package {
            name: name.to_owned(),
            version: flat(version).map(str::to_owned),
            architecture: flat(architecture).map(str::to_owned),
        }))
    }

    fn service(name: &str, provider: &str) -> anyhow::Result<ServiceValue> {
        Ok(ServiceValue(Service {
            name: name.to_owned(),
            provider: provider.to_owned(),
        }))
    }

    fn run(
        command: &str,
        inputs: Option<NoneOr<UnpackListOrTuple<String>>>,
        outputs: Option<NoneOr<UnpackListOrTuple<String>>>,
        group: Option<NoneOr<&str>>,
    ) -> anyhow::Result<StepValue> {
        Ok(StepValue(Step::Run(RunStep {
            command: command.to_owned(),
            inputs: flat(inputs).map(|l| l.items),
            outputs: flat(outputs).map(|l| l.items),
            group: flat(group).map(str::to_owned),
        })))
    }

    fn workdir(path: &str) -> anyhow::Result<StepValue> {
        Ok(StepValue(Step::Workdir(WorkdirStep {
            path: python_path(path),
        })))
    }

    fn copy(
        source: &str,
        target: Option<NoneOr<&str>>,
        ignore: Option<NoneOr<UnpackListOrTuple<String>>>,
        base: Option<NoneOr<&str>>,
    ) -> anyhow::Result<StepValue> {
        Ok(StepValue(Step::Copy(CopyStep {
            source: source.to_owned(),
            target: flat(target).unwrap_or(source).to_owned(),
            ignore: flat(ignore).map(|l| l.items),
            base: flat(base).unwrap_or("source").to_owned(),
        })))
    }

    fn env(
        #[starlark(kwargs)] kwargs: UnpackDictEntries<String, String>,
    ) -> anyhow::Result<StepValue> {
        Ok(StepValue(Step::Env(EnvStep {
            variables: kwargs.entries.into_iter().collect(),
        })))
    }

    fn write(path: &str, content: &str) -> anyhow::Result<StepValue> {
        Ok(StepValue(Step::WriteFile(WriteFileStep {
            path: path.to_owned(),
            content: content.to_owned(),
        })))
    }

    fn path(path: &str) -> anyhow::Result<StepValue> {
        Ok(StepValue(Step::Path(PathStep {
            path: path.to_owned(),
        })))
    }

    fn r#use(#[starlark(args)] args: UnpackTuple<Value>) -> anyhow::Result<StepValue> {
        let dependencies = args
            .items
            .iter()
            .map(|v| package_from(*v)?.ok_or_else(|| anyhow!("use() does not accept None")))
            .collect::<Result<Vec<Package>>>()?;
        Ok(StepValue(Step::Use(UseStep { dependencies })))
    }

    fn mount(name: &str, eval: &mut Evaluator) -> anyhow::Result<MountValue> {
        let ctx = ctx(eval)?;
        let build_path = ctx.layout.build_mount_path(name);
        let serve_path = ctx.layout.serve_mount_path(name);
        Ok(MountValue {
            path: build_path.to_string_lossy().into_owned(),
            serve_path: serve_path.to_string_lossy().into_owned(),
            mount: Mount {
                name: name.to_owned(),
                build_path,
                serve_path,
            },
        })
    }

    fn volume<'v>(
        name: &str,
        serve: &str,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let heap = eval.heap();
        let ctx = ctx(eval)?;
        let volume = Volume {
            name: name.to_owned(),
            path: ctx.layout.volume_path(name),
            serve_path: python_path(serve),
        };
        let path = volume.path.to_string_lossy().into_owned();
        let serve_path = volume.serve_path.to_string_lossy().into_owned();
        let entries: Vec<(Value<'v>, Value<'v>)> = vec![
            (heap.alloc("volume"), heap.alloc(VolumeValue(volume))),
            (heap.alloc("name"), heap.alloc(name)),
            (heap.alloc("path"), heap.alloc(path)),
            (heap.alloc("serve"), heap.alloc(serve_path.as_str())),
            (heap.alloc("serve_path"), heap.alloc(serve_path.as_str())),
        ];
        Ok(heap.alloc(AllocDict(entries)))
    }

    fn file_exists(path: &str, eval: &mut Evaluator) -> anyhow::Result<bool> {
        ctx(eval)?.file_exists(path)
    }

    fn serve(
        name: &str,
        provider: &str,
        build: UnpackListOrTuple<Value>,
        deps: UnpackListOrTuple<Value>,
        commands: UnpackDictEntries<String, String>,
        cwd: Option<NoneOr<&str>>,
        prepare: Option<NoneOr<UnpackListOrTuple<Value>>>,
        mounts: Option<NoneOr<UnpackListOrTuple<Value>>>,
        volumes: Option<NoneOr<UnpackListOrTuple<Value>>>,
        env: Option<NoneOr<UnpackDictEntries<String, String>>>,
        services: Option<NoneOr<UnpackListOrTuple<Value>>>,
        eval: &mut Evaluator,
    ) -> anyhow::Result<NoneType> {
        build_serve(
            eval,
            name,
            provider,
            &build,
            &deps,
            &commands,
            flat(cwd),
            flat(prepare).as_ref(),
            flat(mounts).as_ref(),
            flat(volumes).as_ref(),
            flat(env).as_ref(),
            flat(services).as_ref(),
        )?;
        Ok(NoneType)
    }

    /// The stdlib's `serve()` assembler (//shipit:serve.shipit) shadows the
    /// builtin by design; it reaches the raw builtin through this alias.
    fn _serve(
        name: &str,
        provider: &str,
        build: UnpackListOrTuple<Value>,
        deps: UnpackListOrTuple<Value>,
        commands: UnpackDictEntries<String, String>,
        cwd: Option<NoneOr<&str>>,
        prepare: Option<NoneOr<UnpackListOrTuple<Value>>>,
        mounts: Option<NoneOr<UnpackListOrTuple<Value>>>,
        volumes: Option<NoneOr<UnpackListOrTuple<Value>>>,
        env: Option<NoneOr<UnpackDictEntries<String, String>>>,
        services: Option<NoneOr<UnpackListOrTuple<Value>>>,
        eval: &mut Evaluator,
    ) -> anyhow::Result<NoneType> {
        build_serve(
            eval,
            name,
            provider,
            &build,
            &deps,
            &commands,
            flat(cwd),
            flat(prepare).as_ref(),
            flat(mounts).as_ref(),
            flat(volumes).as_ref(),
            flat(env).as_ref(),
            flat(services).as_ref(),
        )?;
        Ok(NoneType)
    }
}
