//! Starlark evaluator setup and builtins for Shipit.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs;

use anyhow::{Context, anyhow, bail};
use camino::Utf8PathBuf;
use starlark::any::ProvidesStaticType;
use starlark::collections::SmallMap;
use starlark::environment::{GlobalsBuilder, Module};
use starlark::eval::Evaluator;
use starlark::starlark_module;
use starlark::syntax::{AstModule, Dialect};
use starlark::values::Value;
use starlark::values::dict::{AllocDict, DictRef};
use starlark::values::list::UnpackList;
use starlark::values::none::NoneOr;
use starlark::values::tuple::UnpackTuple;

use crate::Result;
use crate::builder::Builder;
use crate::context::{Ctx, RefId};
use crate::model::{CopyBase, CopyStep, Env, RunStep, Serve, ServiceProvider};

pub fn shipit_dialect() -> Dialect {
    let mut dialect = Dialect::Extended;
    dialect.enable_f_strings = true;
    dialect
}

#[derive(ProvidesStaticType)]
struct ShipitEvalContext<'a> {
    ctx: RefCell<Ctx<'a>>,
}

impl<'a> ShipitEvalContext<'a> {
    fn new(builder: &'a mut dyn Builder) -> Self {
        Self {
            ctx: RefCell::new(Ctx::new(builder)),
        }
    }

    fn into_ctx(self) -> Ctx<'a> {
        self.ctx.into_inner()
    }
}

fn with_ctx_only<'v, 'a, T>(
    eval: &mut Evaluator<'v, '_, 'a>,
    f: impl FnOnce(&mut Ctx<'a>) -> anyhow::Result<T>,
) -> anyhow::Result<T> {
    let extra = eval
        .extra
        .and_then(|extra| extra.downcast_ref::<ShipitEvalContext<'a>>())
        .context("Shipit context missing in evaluator")?;
    let mut ctx = extra.ctx.borrow_mut();
    f(&mut ctx)
}

fn parse_service_provider(raw: &str) -> anyhow::Result<ServiceProvider> {
    match raw {
        "postgres" => Ok(ServiceProvider::Postgres),
        "mysql" => Ok(ServiceProvider::Mysql),
        "redis" => Ok(ServiceProvider::Redis),
        other => bail!("Unsupported service provider {other}"),
    }
}

fn parse_copy_base(raw: Option<String>) -> anyhow::Result<CopyBase> {
    match raw.as_deref().unwrap_or("source") {
        "source" => Ok(CopyBase::Source),
        "assets" => Ok(CopyBase::Assets),
        other => bail!("Invalid copy base: {other}"),
    }
}

fn list_to_strings(list: NoneOr<UnpackList<String>>) -> Option<Vec<String>> {
    match list {
        NoneOr::None => None,
        NoneOr::Other(list) => Some(list.items),
    }
}

fn value_list_to_refs<'v>(
    value: Option<UnpackList<Value<'v>>>,
) -> anyhow::Result<Option<Vec<String>>> {
    let Some(value) = value else {
        return Ok(None);
    };

    let mut refs = Vec::new();
    for item in value.items {
        if let Some(s) = item.unpack_str() {
            refs.push(s.to_owned());
            continue;
        }
        if let Some(dict) = DictRef::from_value(item) {
            if let Some(reference) = dict.get_str("ref").and_then(Value::unpack_str) {
                refs.push(reference.to_owned());
                continue;
            }
        }
        bail!("Expected reference strings or mount/volume dictionaries");
    }

    Ok(Some(refs))
}

fn value_list_to_strings<'v>(list: Option<Vec<String>>) -> anyhow::Result<Option<Vec<String>>> {
    Ok(list)
}

fn env_from_value<'v>(value: Option<DictRef<'v>>) -> anyhow::Result<Option<Env>> {
    let Some(dict) = value else {
        return Ok(None);
    };
    let mut env = Env::new();
    for (key, value) in dict.iter() {
        let key = key
            .unpack_str()
            .ok_or_else(|| anyhow!("Environment keys must be strings"))?;
        let value = value
            .unpack_str()
            .ok_or_else(|| anyhow!("Environment values must be strings"))?;
        env.insert(key.to_owned(), value.to_owned());
    }
    Ok(Some(env))
}

#[starlark_module]
fn shipit_builtins(_builder: &mut GlobalsBuilder) {
    fn getenv<'v>(name: String, eval: &mut Evaluator<'v, '_, '_>) -> anyhow::Result<Value<'v>> {
        let value = with_ctx_only(eval, |ctx| Ok(ctx.getenv(&name)))?;
        Ok(match value {
            Some(v) => eval.heap().alloc(v),
            None => Value::new_none(),
        })
    }

    fn dep<'v>(
        name: String,
        #[starlark(default = NoneOr::None)] version: NoneOr<String>,
        #[starlark(default = NoneOr::None)] architecture: NoneOr<String>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<String> {
        with_ctx_only(eval, |ctx| {
            Ok(ctx.dep(name, version.into_option(), architecture.into_option()))
        })
    }

    fn service<'v>(
        name: String,
        provider: String,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<String> {
        let provider = parse_service_provider(&provider)?;
        with_ctx_only(eval, |ctx| Ok(ctx.service(name, provider)))
    }

    #[allow(clippy::too_many_arguments)]
    fn serve<'v>(
        name: String,
        provider: String,
        #[starlark(default = NoneOr::None)] build: NoneOr<UnpackList<String>>,
        #[starlark(default = NoneOr::None)] deps: NoneOr<UnpackList<String>>,
        #[starlark(default = SmallMap::new())] commands: SmallMap<String, String>,
        #[starlark(default = NoneOr::None)] cwd: NoneOr<String>,
        #[starlark(default = NoneOr::None)] prepare: NoneOr<UnpackList<String>>,
        #[starlark(default = NoneOr::None)] workers: NoneOr<UnpackList<String>>,
        #[starlark(default = NoneOr::None)] mounts: NoneOr<UnpackList<Value<'v>>>,
        #[starlark(default = NoneOr::None)] volumes: NoneOr<UnpackList<Value<'v>>>,
        #[starlark(default = NoneOr::None)] env: NoneOr<DictRef<'v>>,
        #[starlark(default = NoneOr::None)] services: NoneOr<UnpackList<String>>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<String> {
        let build_refs = value_list_to_strings(list_to_strings(build))?.unwrap_or_default();
        let dep_refs = value_list_to_strings(list_to_strings(deps))?.unwrap_or_default();
        let prepare_steps = value_list_to_strings(list_to_strings(prepare))?;
        let worker_list = value_list_to_strings(list_to_strings(workers))?;
        let mount_refs = value_list_to_refs(mounts.into_option())?;
        let volume_refs = value_list_to_refs(volumes.into_option())?;
        let env_map = env_from_value(env.into_option())?;
        let service_refs = value_list_to_strings(list_to_strings(services))?;

        with_ctx_only(eval, |ctx| {
            Ok(ctx.serve(
                name,
                provider,
                build_refs,
                dep_refs,
                commands.into_iter().collect(),
                cwd.into_option(),
                prepare_steps,
                worker_list,
                mount_refs,
                volume_refs,
                env_map,
                service_refs,
            ))
        })
    }

    fn run<'v>(
        command: String,
        #[starlark(default = NoneOr::None)] inputs: NoneOr<UnpackList<String>>,
        #[starlark(default = NoneOr::None)] outputs: NoneOr<UnpackList<String>>,
        #[starlark(default = NoneOr::None)] group: NoneOr<String>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<String> {
        let inputs = value_list_to_strings(list_to_strings(inputs))?.unwrap_or_default();
        let outputs = value_list_to_strings(list_to_strings(outputs))?.unwrap_or_default();
        let step = RunStep {
            command,
            inputs,
            outputs,
            group: group.into_option(),
        };
        with_ctx_only(eval, |ctx| Ok(ctx.run(step)))
    }

    fn mount<'v>(name: String, eval: &mut Evaluator<'v, '_, '_>) -> anyhow::Result<Value<'v>> {
        let (ref_id, build_path, serve_path) = with_ctx_only(eval, |ctx| Ok(ctx.mount(name)))?;
        Ok(eval.heap().alloc(AllocDict([
            ("ref", ref_id),
            ("build", build_path.to_string()),
            ("serve", serve_path.to_string()),
        ])))
    }

    fn volume<'v>(
        name: String,
        serve: String,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let serve_path = Utf8PathBuf::from(serve);
        let (ref_id, volume_name) =
            with_ctx_only(eval, |ctx| Ok(ctx.volume(name, serve_path.clone())))?;
        Ok(eval.heap().alloc(AllocDict([
            ("ref", ref_id),
            ("name", volume_name),
            ("serve", serve_path.to_string()),
        ])))
    }

    fn workdir<'v>(path: String, eval: &mut Evaluator<'v, '_, '_>) -> anyhow::Result<String> {
        let path = Utf8PathBuf::from(path);
        with_ctx_only(eval, |ctx| Ok(ctx.workdir(path)))
    }

    fn copy<'v>(
        source: String,
        #[starlark(default = NoneOr::None)] target: NoneOr<String>,
        #[starlark(default = NoneOr::None)] ignore: NoneOr<UnpackList<String>>,
        #[starlark(default = NoneOr::None)] base: NoneOr<String>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<String> {
        let target = target.into_option().unwrap_or_else(|| source.clone());
        let ignore = value_list_to_strings(list_to_strings(ignore))?.unwrap_or_default();
        let base = parse_copy_base(base.into_option())?;
        let step = CopyStep {
            source,
            target,
            ignore,
            base,
        };
        with_ctx_only(eval, |ctx| Ok(ctx.copy(step)))
    }

    fn path<'v>(path: String, eval: &mut Evaluator<'v, '_, '_>) -> anyhow::Result<String> {
        with_ctx_only(eval, |ctx| Ok(ctx.path(path)))
    }

    fn env<'v>(
        #[starlark(kwargs)] env_vars: SmallMap<String, Value<'v>>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<String> {
        let mut vars: Env = BTreeMap::new();
        for (k, v) in env_vars {
            let v = v
                .unpack_str()
                .ok_or_else(|| anyhow!("Environment values must be strings"))?;
            vars.insert(k, v.to_owned());
        }
        with_ctx_only(eval, |ctx| Ok(ctx.env_step(vars)))
    }

    fn r#use<'v>(
        #[starlark(args)] dependencies: UnpackTuple<Value<'v>>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<String> {
        let deps = dependencies
            .into_iter()
            .map(|v| {
                v.unpack_str()
                    .map(|s| s.to_owned())
                    .ok_or_else(|| anyhow!("use() expects dependency references as strings"))
            })
            .collect::<anyhow::Result<Vec<RefId>>>()?;
        with_ctx_only(eval, |ctx| Ok(ctx.use_deps(deps)))
    }
}

pub fn evaluate_shipit<'a>(
    shipit_file: &Utf8PathBuf,
    builder: &'a mut dyn Builder,
) -> Result<(Ctx<'a>, Serve)> {
    let source = fs::read_to_string(shipit_file)
        .with_context(|| format!("Failed to read Shipit file at {shipit_file}"))?;

    let ast = AstModule::parse("shipit", source, &shipit_dialect())
        .map_err(|e| anyhow!(e.to_string()))?;
    let globals = GlobalsBuilder::new().with(shipit_builtins).build();
    let module = Module::new();
    let extra = ShipitEvalContext::new(builder);
    {
        let mut eval: Evaluator<'_, '_, '_> = Evaluator::new(&module);
        eval.extra = Some(&extra);
        eval.eval_module(ast, &globals)
            .map_err(|e| anyhow!(e.to_string()))?;
        drop(eval);
    }

    let ctx = extra.into_ctx();
    if ctx.serves.is_empty() {
        bail!("No serve definition found in {shipit_file}");
    }
    if ctx.serves.len() > 1 {
        bail!("Only one serve is allowed for now");
    }
    let serve = ctx
        .serves
        .values()
        .next()
        .cloned()
        .expect("serve missing after non-empty check");
    Ok((ctx, serve))
}
