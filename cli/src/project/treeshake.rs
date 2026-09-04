//! File-level semantic tree shaking for generated function crates.
//!
//! Reachability scans every retained file as a whole. Full retention keeps every recognized
//! module file, while the caller copies files outside the module graph without modification.

mod database;
mod emission;
mod reachability;
mod treeshaker;

pub(super) use emission::{EmissionPlan, FileEmission};
pub(super) use treeshaker::TreeShaker;
