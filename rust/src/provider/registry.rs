//! Provider registry ordering and construction.
//!
//! This will list all provider descriptors once ported.

use crate::provider::ProviderDescriptor;
use crate::provider::staticfile::StaticFileDescriptor;

/// Return registered providers in priority order.
pub fn providers() -> Vec<Box<dyn ProviderDescriptor>> {
    vec![Box::new(StaticFileDescriptor)]
}
