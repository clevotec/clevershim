pub mod catalog;
pub mod install;
pub mod layout;
pub mod overrides;
pub mod platform;
pub mod repair;
pub mod report;
pub mod resolve;
pub mod scan_eval;
#[cfg(feature = "scan")]
pub mod scan_index;
pub mod sidecar_format;
pub mod sync;
#[cfg(test)]
pub mod testutil;
pub mod winget_list;

pub use catalog::{CatalogFile, PackageEntry};
pub use layout::{Layout, Scope};
pub use sidecar_format::Sidecar;
