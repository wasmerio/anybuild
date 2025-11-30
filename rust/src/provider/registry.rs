//! Provider registry ordering and construction.
//!
//! This will list all provider descriptors once ported.

use crate::provider::ProviderDescriptor;
use crate::provider::hugo::HugoDescriptor;
use crate::provider::jekyll::JekyllDescriptor;
use crate::provider::laravel::LaravelDescriptor;
use crate::provider::mkdocs::MkdocsDescriptor;
use crate::provider::node_static::NodeStaticDescriptor;
use crate::provider::php::PhpDescriptor;
use crate::provider::python::PythonDescriptor;
use crate::provider::staticfile::StaticFileDescriptor;
use crate::provider::wordpress::WordPressDescriptor;

/// Return registered providers in priority order.
pub fn providers() -> Vec<Box<dyn ProviderDescriptor>> {
    vec![
        Box::new(LaravelDescriptor),
        Box::new(HugoDescriptor),
        Box::new(MkdocsDescriptor),
        Box::new(PythonDescriptor),
        Box::new(WordPressDescriptor),
        Box::new(PhpDescriptor),
        Box::new(NodeStaticDescriptor),
        Box::new(JekyllDescriptor),
        Box::new(StaticFileDescriptor),
    ]
}
