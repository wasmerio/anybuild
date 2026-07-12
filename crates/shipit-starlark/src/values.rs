//! Starlark value wrappers for plan objects.
//!
//! The builtins return these; Starlark passes them around opaquely (the
//! stdlib reads a few attributes like `.path` / `.serve_path` / `.group`)
//! and `serve()` receives them back and extracts the plan data.

use std::fmt;

use allocative::Allocative;
use starlark::any::ProvidesStaticType;
use starlark::values::list::AllocList;
use starlark::values::starlark_value;
use starlark::values::{Heap, NoSerialize, StarlarkValue, Value};

use shipit_plan::{Mount, Package, Step, Volume};

fn alloc_opt_str<'v>(heap: Heap<'v>, value: &Option<String>) -> Value<'v> {
    match value {
        Some(s) => heap.alloc(s.as_str()),
        None => Value::new_none(),
    }
}

fn alloc_opt_str_list<'v>(heap: Heap<'v>, value: &Option<Vec<String>>) -> Value<'v> {
    match value {
        Some(items) => heap.alloc(AllocList(items.iter().map(|s| s.as_str()))),
        None => Value::new_none(),
    }
}

/// A `dep()` result: a package handle.
#[derive(Debug, Clone, ProvidesStaticType, NoSerialize, Allocative)]
pub struct PackageValue(#[allocative(skip)] pub Package);

impl fmt::Display for PackageValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[starlark_value(type = "package")]
impl<'v> StarlarkValue<'v> for PackageValue {
    fn get_attr(&self, attribute: &str, heap: Heap<'v>) -> Option<Value<'v>> {
        match attribute {
            "name" => Some(heap.alloc(self.0.name.as_str())),
            "version" => Some(alloc_opt_str(heap, &self.0.version)),
            "architecture" => Some(alloc_opt_str(heap, &self.0.architecture)),
            _ => None,
        }
    }
}

starlark::starlark_simple_value!(PackageValue);

/// Any build step. The stdlib probes steps with `getattr(step, "command",
/// None)` / `getattr(step, "group", None)`, so attributes project the
/// underlying dataclass fields.
#[derive(Debug, Clone, ProvidesStaticType, NoSerialize, Allocative)]
pub struct StepValue(#[allocative(skip)] pub Step);

impl fmt::Display for StepValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}(...)", self.0.type_name())
    }
}

#[starlark_value(type = "step")]
impl<'v> StarlarkValue<'v> for StepValue {
    fn get_attr(&self, attribute: &str, heap: Heap<'v>) -> Option<Value<'v>> {
        match (&self.0, attribute) {
            (Step::Run(s), "command") => Some(heap.alloc(s.command.as_str())),
            (Step::Run(s), "inputs") => Some(alloc_opt_str_list(heap, &s.inputs)),
            (Step::Run(s), "outputs") => Some(alloc_opt_str_list(heap, &s.outputs)),
            (Step::Run(s), "group") => Some(alloc_opt_str(heap, &s.group)),
            (Step::Copy(s), "source") => Some(heap.alloc(s.source.as_str())),
            (Step::Copy(s), "target") => Some(heap.alloc(s.target.as_str())),
            (Step::Copy(s), "ignore") => Some(alloc_opt_str_list(heap, &s.ignore)),
            (Step::Copy(s), "base") => Some(heap.alloc(s.base.as_str())),
            (Step::Env(s), "variables") => {
                let entries: Vec<(Value<'v>, Value<'v>)> = s
                    .variables
                    .iter()
                    .map(|(k, v)| (heap.alloc(k.as_str()), heap.alloc(v.as_str())))
                    .collect();
                Some(heap.alloc(starlark::values::dict::AllocDict(entries)))
            }
            (Step::Use(s), "dependencies") => Some(heap.alloc(AllocList(
                s.dependencies.iter().map(|d| PackageValue(d.clone())),
            ))),
            (Step::Workdir(s), "path") => Some(heap.alloc(s.path.to_string_lossy().into_owned())),
            (Step::Path(s), "path") => Some(heap.alloc(s.path.as_str())),
            (Step::WriteFile(s), "path") => Some(heap.alloc(s.path.as_str())),
            (Step::WriteFile(s), "content") => Some(heap.alloc(s.content.as_str())),
            _ => None,
        }
    }
}

starlark::starlark_simple_value!(StepValue);

/// A `mount()` handle: the plan Mount plus its string build/serve paths.
#[derive(Debug, Clone, ProvidesStaticType, NoSerialize, Allocative)]
pub struct MountValue {
    #[allocative(skip)]
    pub mount: Mount,
    pub path: String,
    pub serve_path: String,
}

impl fmt::Display for MountValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "mount({})", self.mount.name)
    }
}

#[starlark_value(type = "mount")]
impl<'v> StarlarkValue<'v> for MountValue {
    fn get_attr(&self, attribute: &str, heap: Heap<'v>) -> Option<Value<'v>> {
        match attribute {
            "name" => Some(heap.alloc(self.mount.name.as_str())),
            "path" => Some(heap.alloc(self.path.as_str())),
            "serve_path" => Some(heap.alloc(self.serve_path.as_str())),
            _ => None,
        }
    }
}

starlark::starlark_simple_value!(MountValue);

/// The opaque volume object stored under `"volume"` in the dict that the
/// `volume()` builtin returns.
#[derive(Debug, Clone, ProvidesStaticType, NoSerialize, Allocative)]
pub struct VolumeValue(#[allocative(skip)] pub Volume);

impl fmt::Display for VolumeValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "volume({})", self.0.name)
    }
}

#[starlark_value(type = "volume")]
impl<'v> StarlarkValue<'v> for VolumeValue {
    fn get_attr(&self, attribute: &str, heap: Heap<'v>) -> Option<Value<'v>> {
        match attribute {
            "name" => Some(heap.alloc(self.0.name.as_str())),
            _ => None,
        }
    }
}

starlark::starlark_simple_value!(VolumeValue);

/// A `service()` result.
#[derive(Debug, Clone, ProvidesStaticType, NoSerialize, Allocative)]
pub struct ServiceValue(#[allocative(skip)] pub shipit_plan::Service);

impl fmt::Display for ServiceValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "service({}, {})", self.0.name, self.0.provider)
    }
}

#[starlark_value(type = "service")]
impl<'v> StarlarkValue<'v> for ServiceValue {
    fn get_attr(&self, attribute: &str, heap: Heap<'v>) -> Option<Value<'v>> {
        match attribute {
            "name" => Some(heap.alloc(self.0.name.as_str())),
            "provider" => Some(heap.alloc(self.0.provider.as_str())),
            _ => None,
        }
    }
}

starlark::starlark_simple_value!(ServiceValue);
